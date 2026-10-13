use anyhow::Result;

pub use remote_merge_core::relative_path::RelPathError;
pub use remote_merge_core::scan::outcome::{ListedEntry, ScanFailure, ScanRequest};

mod failure;
pub use failure::WireFailure;

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
        /// v8 → v9: TreeChunk に UTF-8 として読めない名前のファイルの unreadable_names を追加
        /// v9 → v10: 受け取ったパスを入力の形の検査で拒んだことを型で返す。一つのパスの要求は
        ///           PathRefused、複数のパスの要求は FileContents・FileHashes・Stats の refused。
        ///           項目の操作の要求で root_dir 全体を項目として拒んだ理由も同じ型で運ぶ
        /// v10 → v11: ListTree を走査の規則の求め（起点の範囲と件数の上限）にし、TreeChunk で
        ///           規則が載せた一覧の項目を、TreeFailed で走査を止めた誤りの種類を運ぶ。
        ///           TreeIncomplete をなくした
        /// v11 → v12: 操作の失敗を種類（WireFailure）で運ぶ。読み込みの要求は大きさの上限を運ぶ。
        ///           使われなくなった StatFiles をなくした。書き込み・symlink の作成・削除・権限の変更の
        ///           結果を Changed にまとめた。ListDir をなくし、子の一覧は ListTree の
        ///           ScanScope::Children で求める
        /// v12 → v13: 走査の一覧の項目の種類に、FIFO・ソケット・デバイスのその他の種類（Other）を足した
        /// v13 → v14: 一覧の symlink に、リンク先がディレクトリか・ディレクトリでないか・ないか
        ///           （points_to）を持たせた
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

define_protocol_version!(14);

/// ハンドシェイク行のプレフィックス
pub const HANDSHAKE_PREFIX: &str = "remote-merge agent";

// ---------------------------------------------------------------------------
// Request / Response
// ---------------------------------------------------------------------------

mod messages;
pub use messages::{AgentRequest, AgentResponse};

mod data;
pub use data::{AgentEntryLink, AgentPathInspection, FileHashResult, FileReadResult};

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
#[path = "lib_tests.rs"]
mod tests;

pub mod framing;

#[cfg(test)]
#[path = "wire_shape_tests.rs"]
mod wire_shape_tests;

#[cfg(test)]
#[path = "failure_wire_tests.rs"]
mod failure_wire_tests;
