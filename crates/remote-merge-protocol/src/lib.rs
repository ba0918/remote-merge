#![warn(clippy::unwrap_used)]
#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
use anyhow::Result;

pub use remote_merge_core::relative_path::RelPathError;
pub use remote_merge_core::scan::outcome::{ListedEntry, ScanFailure, ScanRequest};

mod failure;
pub use failure::WireFailure;

macro_rules! define_protocol_version {
    ($version:literal) => {
        /// 通信の版。要求と応答の形（wire_shape_tests などが固定するバイト列）を変えたら上げる。
        ///
        /// 手元と同じ版のエージェントだけを使う（docs/ir/ssh/fallback.md の REQ-ssh-021）。古い版の
        /// ための既定値は持たないので、版ごとの違いの経緯はここに書かず、git の履歴と docs/decision に
        /// 残す。
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

define_protocol_version!(15);

/// ハンドシェイク行のプレフィックス
pub const HANDSHAKE_PREFIX: &str = "remote-merge agent";

// ---------------------------------------------------------------------------
// Request / Response
// ---------------------------------------------------------------------------

mod messages;
pub use messages::{AgentRequest, AgentResponse};

mod data;
pub use data::{
    AgentEntryLink, AgentPathCheck, AgentPathInspection, FileHashResult, FileReadResult,
};

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
