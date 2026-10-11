use anyhow::Result;
use serde::{Deserialize, Serialize};

macro_rules! define_protocol_version {
    ($version:literal) => {
        /// プロトコルバージョン（破壊的変更時にインクリメント）
        ///
        /// v2 → v3: HashFiles コマンド追加、FileContents に is_last フィールド追加
        /// v3 → v4: PathInspection の Symlink に real_path フィールド追加
        /// v4 → v5: PathInspection を "/" から要素ごとに辿る確認の結果の種類に変更
        /// v5 → v6: 手元と同じ版だけを受け入れ、古い版のための既定値をなくした。
        ///          Remove・Chmod・ListDir を追加
        /// v6 → v7: ListTree の応答に、循環で走査を止めたことを示す TreeIncomplete を追加
        /// v7 → v8: 一つの項目を辿らずに読む ReadEntryLink を追加
        pub const PROTOCOL_VERSION: u32 = $version;

        /// CLI のバージョン表示。配置済み Agent の互換性判定にも使う。
        #[macro_export]
        macro_rules! product_cli_version {
            () => {
                concat!(
                    env!("CARGO_PKG_VERSION"),
                    " (protocol v",
                    stringify!($version),
                    ")"
                )
            };
        }
    };
}

define_protocol_version!(8);

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
    /// root_dir からの `path` を消す。末尾の symlink ならリンクだけを消す
    Remove {
        path: String,
    },
    /// root_dir からの `path` の権限を `mode` にする
    Chmod {
        path: String,
        mode: u32,
    },
    /// root_dir からの `path` の直下の子を一覧する。`exclude` は子の root_dir からのパスに当てる
    ListDir {
        path: String,
        exclude: Vec<String>,
        max_entries: usize,
    },
    /// root_dir からの `path` の項目そのもの（最後の要素のリンクは辿らない）が symlink なら、その
    /// リンク先を読む。親ディレクトリは一覧しない
    ReadEntryLink {
        path: String,
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
        truncated: bool,
    },
    /// ディレクトリを指す symlink が祖先へ戻る循環で走査を止めた。一覧は返さない
    TreeIncomplete {
        reason: String,
    },
    FileContents {
        results: Vec<FileReadResult>,
        /// ストリーミング対応: true で最後のチャンク。
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
    /// Remove・Chmod の結果。失敗なら文言（操作・パス・原因を含む）
    OperationResult {
        error: Option<String>,
    },
    /// ListDir の応答。直下の子が多いときは、子を分けて複数の応答で返す（`is_last` で最後）
    DirListing {
        result: AgentDirListing,
        is_last: bool,
    },
    /// ReadEntryLink の応答
    EntryLink {
        result: AgentEntryLink,
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

/// InspectPath の応答。"/" から要素ごとに辿った確認の結果の四つの種類か、辿れなかった原因の文言
/// （`Unresolved`）か、確認の前に拒んだ理由（`Error`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentPathInspection {
    Missing {
        real_parent: String,
        parent_exists: bool,
    },
    Exists {
        real_path: String,
    },
    Symlink {
        link_target: String,
        real_path: String,
    },
    AncestorNotDirectory {
        real_path: String,
        regular_file: bool,
    },
    Unresolved {
        cause: String,
    },
    Error {
        message: String,
    },
}

/// ReadEntryLink の結果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentEntryLink {
    /// 項目が symlink。リンク先は読んだ文字列そのまま
    Symlink { target: String },
    /// symlink でないか、ないか、途中がディレクトリでない
    NotSymlink,
    /// パスを拒んだか、項目を読めなかった。値は文言
    Error { message: String },
}

/// ListDir の応答。一覧を止めるエラー（リンク先か更新時刻を読めない）は `AgentResponse::Error` で返す
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentDirListing {
    /// 直下の子。`path` は子の名前
    Children { entries: Vec<AgentFileEntry> },
    /// 通常ファイル、ディレクトリでないものへの symlink、リンク切れ、ないパス
    NotDirectory,
    /// ディレクトリかもしれないのに中身が分からない
    Unlistable { reason: String },
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

/// エージェントが答えた通信の版が手元と違う。違う版のエージェントは使わない
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersionMismatch {
    pub remote: u32,
    pub local: u32,
}

impl std::fmt::Display for ProtocolVersionMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the agent speaks protocol v{}, but this remote-merge speaks protocol v{}",
            self.remote, self.local
        )
    }
}

impl std::error::Error for ProtocolVersionMismatch {}

/// エージェントが答えた通信の版が手元と同じときだけ受け入れる
pub fn check_protocol_version(remote_version: u32) -> Result<u32> {
    if remote_version != PROTOCOL_VERSION {
        return Err(ProtocolVersionMismatch {
            remote: remote_version,
            local: PROTOCOL_VERSION,
        }
        .into());
    }
    Ok(remote_version)
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
    fn check_protocol_version_match() {
        let negotiated = check_protocol_version(PROTOCOL_VERSION).unwrap();
        assert_eq!(negotiated, PROTOCOL_VERSION);
    }

    #[test]
    fn an_older_or_newer_protocol_version_is_refused_with_both_versions() {
        for remote in [PROTOCOL_VERSION - 1, PROTOCOL_VERSION + 1] {
            let error = check_protocol_version(remote).unwrap_err();
            assert_eq!(
                error.downcast_ref::<ProtocolVersionMismatch>(),
                Some(&ProtocolVersionMismatch {
                    remote,
                    local: PROTOCOL_VERSION
                })
            );
        }
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

    // ---- Path inspection ----

    #[test]
    fn every_kind_of_path_inspection_survives_the_wire_format() {
        let inspections = [
            AgentPathInspection::Missing {
                real_parent: "/srv/app/missing".into(),
                parent_exists: false,
            },
            AgentPathInspection::Exists {
                real_path: "/srv/app/file.txt".into(),
            },
            AgentPathInspection::Symlink {
                link_target: "../shared/file.txt".into(),
                real_path: "/srv/shared/file.txt".into(),
            },
            AgentPathInspection::AncestorNotDirectory {
                real_path: "/srv/app/dir".into(),
                regular_file: true,
            },
            AgentPathInspection::Unresolved {
                cause: "too many levels of symbolic links".into(),
            },
            AgentPathInspection::Error {
                message: "path traversal detected: ../x".into(),
            },
        ];
        for result in inspections {
            let response = AgentResponse::PathInspection { result };
            let bytes = serialize_response(&response).unwrap();
            assert_eq!(deserialize_response(&bytes).unwrap(), response);
        }
    }
}

pub mod framing;
