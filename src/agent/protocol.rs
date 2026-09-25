use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

macro_rules! define_protocol_version {
    ($version:literal) => {
        /// プロトコルバージョン（破壊的変更時にインクリメント）
        ///
        /// v2 → v3: HashFiles コマンド追加、FileContents に is_last フィールド追加
        /// v3 → v4: PathInspection の Symlink に real_path フィールド追加
        pub const PROTOCOL_VERSION: u32 = $version;

        /// CLI のバージョン表示。配置済み Agent の互換性判定にも使う。
        pub const CLI_VERSION: &str = concat!(
            env!("CARGO_PKG_VERSION"),
            " (protocol v",
            stringify!($version),
            ")"
        );
    };
}

define_protocol_version!(4);

/// ハンドシェイク行のプレフィックス
pub const HANDSHAKE_PREFIX: &str = "remote-merge agent";

// ---------------------------------------------------------------------------
// Request / Response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentRequest {
    ListTree {
        root: String,
        exclude: Vec<String>,
        #[serde(default)]
        include: Vec<String>,
        max_entries: usize,
    },
    ReadFiles {
        paths: Vec<String>,
        chunk_size_limit: usize,
    },
    HashFiles {
        paths: Vec<String>,
    },
    WriteFile {
        path: String,
        content: Vec<u8>,
        is_binary: bool,
        more_to_follow: bool,
    },
    StatFiles {
        paths: Vec<String>,
    },
    InspectPath {
        path: String,
    },
    Symlink {
        path: String,
        target: String,
    },
    Shutdown,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentResponse {
    TreeChunk {
        nodes: Vec<AgentFileEntry>,
        is_last: bool,
        total_scanned: usize,
        /// max_entries に達してスキャンが打ち切られた場合 true。
        /// 後方互換性のため、古いプロトコルからデシリアライズ時は false にフォールバック。
        #[serde(default)]
        truncated: bool,
    },
    FileContents {
        results: Vec<FileReadResult>,
        /// ストリーミング対応: true で最後のチャンク。
        /// v2 エージェントとの後方互換性のため、未設定時は true にフォールバック。
        #[serde(default = "default_is_last")]
        is_last: bool,
    },
    FileHashes {
        results: Vec<FileHashResult>,
        /// ストリーミング対応: true で最後のチャンク。
        is_last: bool,
    },
    WriteResult {
        success: bool,
        error: Option<String>,
    },
    Stats {
        entries: Vec<AgentFileStat>,
    },
    PathInspection {
        result: AgentPathInspection,
    },
    SymlinkResult {
        success: bool,
        error: Option<String>,
    },
    Pong,
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentFileEntry {
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
    pub permissions: u32,
    pub symlink_target: Option<String>,
    #[serde(default)]
    pub link_is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileReadResult {
    Ok {
        path: String,
        content: Vec<u8>,
        more_to_follow: bool,
    },
    Error {
        path: String,
        message: String,
    },
}

/// `is_last` フィールドのデフォルト値。
/// v2 エージェントは `is_last` フィールドを送信しないため、
/// デシリアライズ時に true にフォールバックする（単一チャンクとして扱う）。
fn default_is_last() -> bool {
    true
}

/// HashFiles レスポンス用のハッシュ結果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileHashResult {
    /// 通常ファイル: SHA-256 ハッシュ（hex string）
    Ok { path: String, hash: String },
    /// シンボリックリンク: リンクターゲットパス
    Symlink { path: String, target: String },
    /// エラー
    Error { path: String, reason: String },
}

/// StatFiles レスポンス用のメタデータ。
/// permissions を含む（楽観的ロック時にパーミッション変化も検知するため）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentFileStat {
    pub path: String,
    pub size: u64,
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
    pub permissions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentPathInspection {
    Missing {
        real_parent: String,
    },
    File {
        real_path: String,
    },
    Symlink {
        link_target: String,
        real_path: String,
    },
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Handshake helpers
// ---------------------------------------------------------------------------

/// ハンドシェイク文字列を生成する
pub fn format_handshake() -> String {
    format!("{HANDSHAKE_PREFIX} v{PROTOCOL_VERSION}")
}

/// ハンドシェイク文字列をパースし、プロトコルバージョンを返す。
/// バージョン一致チェックは行わない — 呼び出し側の責務。
pub fn parse_handshake(line: &str) -> Result<u32> {
    let line = line.trim();
    let rest = line.strip_prefix(HANDSHAKE_PREFIX).ok_or_else(|| {
        anyhow::anyhow!("invalid handshake: expected prefix \"{HANDSHAKE_PREFIX}\"")
    })?;
    let rest = rest.trim();
    let version_str = rest
        .strip_prefix('v')
        .ok_or_else(|| anyhow::anyhow!("invalid handshake: missing version prefix 'v'"))?;
    let version: u32 = version_str.parse().map_err(|_| {
        anyhow::anyhow!("invalid handshake: cannot parse version \"{version_str}\"")
    })?;
    Ok(version)
}

/// パースしたバージョンが現在のプロトコルバージョン以上であるか検証し、
/// ネゴシエーション済みバージョン（両者の最小値）を返す。
///
/// クライアントとサーバーのバージョンが異なる場合、低い方の機能セットで動作する。
/// サーバーがクライアントより古い場合（例: サーバー v2、クライアント v3）でも接続を許可し、
/// v2 の機能セットで動作する。ただしサーバーが v1 以下の場合は拒否する。
pub fn check_protocol_version(remote_version: u32) -> Result<u32> {
    if remote_version < 2 {
        bail!("protocol version too old: got {remote_version}, minimum supported is 2");
    }
    // ネゴシエーション: 両者の最小値
    Ok(remote_version.min(PROTOCOL_VERSION))
}

// ---------------------------------------------------------------------------
// Serialize / Deserialize helpers
// ---------------------------------------------------------------------------

pub fn serialize_request(req: &AgentRequest) -> Result<Vec<u8>> {
    rmp_serde::to_vec(req).map_err(Into::into)
}

pub fn deserialize_request(data: &[u8]) -> Result<AgentRequest> {
    rmp_serde::from_slice(data).map_err(Into::into)
}

pub fn serialize_response(resp: &AgentResponse) -> Result<Vec<u8>> {
    rmp_serde::to_vec(resp).map_err(Into::into)
}

pub fn deserialize_response(data: &[u8]) -> Result<AgentResponse> {
    rmp_serde::from_slice(data).map_err(Into::into)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_agent_entries_without_directory_link_metadata_remain_readable() {
        #[derive(Serialize)]
        struct PreviousEntry {
            path: String,
            kind: FileKind,
            size: u64,
            mtime_secs: i64,
            mtime_nanos: u32,
            permissions: u32,
            symlink_target: Option<String>,
        }
        let old = PreviousEntry {
            path: "linked".into(),
            kind: FileKind::Symlink,
            size: 0,
            mtime_secs: 0,
            mtime_nanos: 0,
            permissions: 0,
            symlink_target: Some("folder".into()),
        };
        let bytes = rmp_serde::to_vec(&old).unwrap();
        let decoded: AgentFileEntry = rmp_serde::from_slice(&bytes).unwrap();
        assert!(!decoded.link_is_dir);
        assert_eq!(decoded.symlink_target.as_deref(), Some("folder"));
    }

    // ---- Handshake ----

    #[test]
    fn handshake_format_and_parse() {
        let hs = format_handshake();
        assert_eq!(hs, format!("remote-merge agent v{PROTOCOL_VERSION}"));
        let ver = parse_handshake(&hs).unwrap();
        assert_eq!(ver, PROTOCOL_VERSION);
    }

    #[test]
    fn handshake_parse_with_trailing_whitespace() {
        let handshake = format!("  remote-merge agent v{PROTOCOL_VERSION}  ");
        let ver = parse_handshake(&handshake).unwrap();
        assert_eq!(ver, PROTOCOL_VERSION);
    }

    #[test]
    fn handshake_parse_returns_version_without_validation() {
        // parse_handshake はバージョン番号を返すだけ（バリデーションしない）
        let ver = parse_handshake("remote-merge agent v999").unwrap();
        assert_eq!(ver, 999);
    }

    #[test]
    fn check_protocol_version_too_old() {
        let err = check_protocol_version(1).unwrap_err();
        assert!(err.to_string().contains("too old"));
    }

    #[test]
    fn check_protocol_version_match() {
        let negotiated = check_protocol_version(PROTOCOL_VERSION).unwrap();
        assert_eq!(negotiated, PROTOCOL_VERSION);
    }

    #[test]
    fn check_protocol_version_v2_accepted() {
        // v2 Agent はまだ接続可能（HashFiles は使えないが ReadFiles は動作する）
        let negotiated = check_protocol_version(2).unwrap();
        assert_eq!(negotiated, 2);
    }

    #[test]
    fn check_protocol_version_newer_agent() {
        // Agent がクライアントより新しい場合 → クライアント側バージョンにネゴシエーション
        let negotiated = check_protocol_version(999).unwrap();
        assert_eq!(negotiated, PROTOCOL_VERSION);
    }

    #[test]
    fn handshake_parse_invalid_prefix() {
        let err = parse_handshake("wrong-tool agent v1").unwrap_err();
        assert!(err.to_string().contains("invalid handshake"));
    }

    #[test]
    fn handshake_parse_missing_version() {
        let err = parse_handshake("remote-merge agent").unwrap_err();
        assert!(err.to_string().contains("invalid handshake"));
    }

    #[test]
    fn handshake_parse_non_numeric_version() {
        let err = parse_handshake("remote-merge agent vabc").unwrap_err();
        assert!(err.to_string().contains("cannot parse version"));
    }

    /// include フィールドなしのデータをデシリアライズ → デフォルト空配列（後方互換性）
    #[test]
    fn list_tree_without_include_backward_compat() {
        // include フィールドなしの ListTree を JSON でシミュレート
        // （msgpack でもフィールドが省略されたときに #[serde(default)] が効く）
        let json =
            r#"{"ListTree":{"root":"/var/www","exclude":["node_modules"],"max_entries":5000}}"#;
        let req: AgentRequest = serde_json::from_str(json).unwrap();
        match req {
            AgentRequest::ListTree { include, .. } => {
                assert!(include.is_empty(), "include should default to empty vec");
            }
            other => panic!("expected ListTree, got {other:?}"),
        }
    }

    #[test]
    fn response_file_contents_is_last_default_true() {
        // v2 Agent が is_last を含めずに送信した場合、default_is_last() で true になること
        let json = r#"{"FileContents":{"results":[]}}"#;
        let resp: AgentResponse = serde_json::from_str(json).unwrap();
        match resp {
            AgentResponse::FileContents { is_last, .. } => {
                assert!(
                    is_last,
                    "is_last should default to true for backward compatibility"
                );
            }
            other => panic!("expected FileContents, got {other:?}"),
        }
    }

    // ---- Protocol version ----

    #[test]
    fn protocol_version_is_4() {
        assert_eq!(PROTOCOL_VERSION, 4);
    }
}
