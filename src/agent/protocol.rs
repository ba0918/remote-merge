use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// プロトコルバージョン（破壊的変更時にインクリメント）
pub const PROTOCOL_VERSION: u32 = 2;

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
        max_entries: usize,
    },
    ReadFiles {
        paths: Vec<String>,
        chunk_size_limit: usize,
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
    Backup {
        paths: Vec<String>,
        backup_dir: String,
    },
    Symlink {
        path: String,
        target: String,
    },
    ListBackups {
        backup_dir: String,
    },
    RestoreBackup {
        backup_dir: String,
        session_id: String,
        files: Vec<String>,
        /// NOTE: クライアント指定の root_dir は安全のため dispatch 側で無視される。
        /// Agent 起動時の --root (self.root_dir) が常に復元先として使用される。
        /// プロトコル互換性のためフィールドは残す。
        root_dir: String,
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
    },
    FileContents {
        results: Vec<FileReadResult>,
    },
    WriteResult {
        success: bool,
        error: Option<String>,
    },
    Stats {
        entries: Vec<AgentFileStat>,
    },
    BackupResult {
        success: bool,
        error: Option<String>,
    },
    SymlinkResult {
        success: bool,
        error: Option<String>,
    },
    BackupList {
        sessions: Vec<AgentBackupSession>,
    },
    RestoreResult {
        results: Vec<AgentRestoreFileResult>,
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

/// バックアップセッション情報（ListBackups レスポンス用）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBackupSession {
    pub session_id: String,
    pub files: Vec<AgentBackupFile>,
}

/// バックアップセッション内の個別ファイル情報。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBackupFile {
    pub path: String,
    pub size: u64,
}

/// 個別ファイルの復元結果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRestoreFileResult {
    pub path: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

/// パースしたバージョンが現在のプロトコルバージョンと一致するか検証する
pub fn check_protocol_version(remote_version: u32) -> Result<()> {
    if remote_version != PROTOCOL_VERSION {
        bail!("protocol version mismatch: expected {PROTOCOL_VERSION}, got {remote_version}");
    }
    Ok(())
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
        assert_eq!(hs, "remote-merge agent v2");
        let ver = parse_handshake(&hs).unwrap();
        assert_eq!(ver, PROTOCOL_VERSION);
    }

    #[test]
    fn handshake_parse_with_trailing_whitespace() {
        let ver = parse_handshake("  remote-merge agent v2  ").unwrap();
        assert_eq!(ver, PROTOCOL_VERSION);
    }

    #[test]
    fn handshake_parse_returns_version_without_validation() {
        // parse_handshake はバージョン番号を返すだけ（バリデーションしない）
        let ver = parse_handshake("remote-merge agent v999").unwrap();
        assert_eq!(ver, 999);
    }

    #[test]
    fn check_protocol_version_mismatch() {
        let err = check_protocol_version(999).unwrap_err();
        assert!(err.to_string().contains("version mismatch"));
    }

    #[test]
    fn check_protocol_version_match() {
        check_protocol_version(PROTOCOL_VERSION).unwrap();
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

    // ---- Request roundtrip ----

    fn roundtrip_request(req: &AgentRequest) {
        let data = serialize_request(req).unwrap();
        let decoded = deserialize_request(&data).unwrap();
        assert_eq!(*req, decoded);
    }

    #[test]
    fn request_list_tree_roundtrip() {
        roundtrip_request(&AgentRequest::ListTree {
            root: "/var/www".into(),
            exclude: vec!["node_modules".into(), ".git".into()],
            max_entries: 5000,
        });
    }

    #[test]
    fn request_read_files_roundtrip() {
        roundtrip_request(&AgentRequest::ReadFiles {
            paths: vec!["/etc/hosts".into(), "/etc/passwd".into()],
            chunk_size_limit: 1_048_576,
        });
    }

    #[test]
    fn request_write_file_roundtrip() {
        roundtrip_request(&AgentRequest::WriteFile {
            path: "/tmp/test.txt".into(),
            content: b"hello world".to_vec(),
            is_binary: false,
            more_to_follow: false,
        });
    }

    #[test]
    fn request_write_file_binary_roundtrip() {
        roundtrip_request(&AgentRequest::WriteFile {
            path: "/tmp/image.png".into(),
            content: vec![0x89, 0x50, 0x4E, 0x47, 0x00, 0xFF],
            is_binary: true,
            more_to_follow: true,
        });
    }

    #[test]
    fn request_stat_files_roundtrip() {
        roundtrip_request(&AgentRequest::StatFiles {
            paths: vec!["/tmp/a".into()],
        });
    }

    #[test]
    fn request_backup_roundtrip() {
        roundtrip_request(&AgentRequest::Backup {
            paths: vec!["/var/www/index.html".into()],
            backup_dir: "/var/backups".into(),
        });
    }

    #[test]
    fn request_symlink_roundtrip() {
        roundtrip_request(&AgentRequest::Symlink {
            path: "/tmp/link".into(),
            target: "/tmp/target".into(),
        });
    }

    #[test]
    fn request_shutdown_roundtrip() {
        roundtrip_request(&AgentRequest::Shutdown);
    }

    #[test]
    fn request_ping_roundtrip() {
        roundtrip_request(&AgentRequest::Ping);
    }

    // ---- Response roundtrip ----

    fn roundtrip_response(resp: &AgentResponse) {
        let data = serialize_response(resp).unwrap();
        let decoded = deserialize_response(&data).unwrap();
        assert_eq!(*resp, decoded);
    }

    #[test]
    fn response_tree_chunk_roundtrip() {
        roundtrip_response(&AgentResponse::TreeChunk {
            nodes: vec![AgentFileEntry {
                path: "/var/www/index.html".into(),
                kind: FileKind::File,
                size: 1024,
                mtime_secs: 1700000000,
                mtime_nanos: 500,
                permissions: 0o644,
                symlink_target: None,
            }],
            is_last: true,
            total_scanned: 1,
        });
    }

    #[test]
    fn response_tree_chunk_with_symlink() {
        roundtrip_response(&AgentResponse::TreeChunk {
            nodes: vec![AgentFileEntry {
                path: "/var/www/current".into(),
                kind: FileKind::Symlink,
                size: 0,
                mtime_secs: 1700000000,
                mtime_nanos: 0,
                permissions: 0o777,
                symlink_target: Some("/var/www/releases/v2".into()),
            }],
            is_last: true,
            total_scanned: 1,
        });
    }

    #[test]
    fn response_file_contents_roundtrip() {
        roundtrip_response(&AgentResponse::FileContents {
            results: vec![
                FileReadResult::Ok {
                    path: "/tmp/a.txt".into(),
                    content: b"content".to_vec(),
                    more_to_follow: false,
                },
                FileReadResult::Error {
                    path: "/tmp/b.txt".into(),
                    message: "permission denied".into(),
                },
            ],
        });
    }

    #[test]
    fn response_write_result_roundtrip() {
        roundtrip_response(&AgentResponse::WriteResult {
            success: true,
            error: None,
        });
        roundtrip_response(&AgentResponse::WriteResult {
            success: false,
            error: Some("disk full".into()),
        });
    }

    #[test]
    fn response_stats_roundtrip() {
        roundtrip_response(&AgentResponse::Stats {
            entries: vec![AgentFileStat {
                path: "/tmp/test".into(),
                size: 4096,
                mtime_secs: 1700000000,
                mtime_nanos: 123456789,
                permissions: 0o644,
            }],
        });
    }

    #[test]
    fn response_backup_result_roundtrip() {
        roundtrip_response(&AgentResponse::BackupResult {
            success: true,
            error: None,
        });
    }

    #[test]
    fn response_symlink_result_roundtrip() {
        roundtrip_response(&AgentResponse::SymlinkResult {
            success: true,
            error: None,
        });
    }

    #[test]
    fn response_pong_roundtrip() {
        roundtrip_response(&AgentResponse::Pong);
    }

    #[test]
    fn response_error_roundtrip() {
        roundtrip_response(&AgentResponse::Error {
            message: "something went wrong".into(),
        });
    }

    // ---- Edge cases ----

    #[test]
    fn binary_content_roundtrip() {
        // 全バイト値 0x00..0xFF を含むデータ
        let content: Vec<u8> = (0..=255).collect();
        let result = FileReadResult::Ok {
            path: "/bin/test".into(),
            content: content.clone(),
            more_to_follow: false,
        };
        let resp = AgentResponse::FileContents {
            results: vec![result],
        };
        let data = serialize_response(&resp).unwrap();
        let decoded = deserialize_response(&data).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn large_tree_chunk_roundtrip() {
        let nodes: Vec<AgentFileEntry> = (0..1000)
            .map(|i| AgentFileEntry {
                path: format!("/var/www/file_{i}.txt"),
                kind: FileKind::File,
                size: i as u64 * 100,
                mtime_secs: 1700000000 + i as i64,
                mtime_nanos: 0,
                permissions: 0o644,
                symlink_target: None,
            })
            .collect();
        let resp = AgentResponse::TreeChunk {
            nodes,
            is_last: true,
            total_scanned: 1000,
        };
        let data = serialize_response(&resp).unwrap();
        let decoded = deserialize_response(&data).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn empty_collections_roundtrip() {
        roundtrip_request(&AgentRequest::ListTree {
            root: "/".into(),
            exclude: vec![],
            max_entries: 0,
        });
        roundtrip_request(&AgentRequest::ReadFiles {
            paths: vec![],
            chunk_size_limit: 0,
        });
        roundtrip_request(&AgentRequest::StatFiles { paths: vec![] });
        roundtrip_response(&AgentResponse::TreeChunk {
            nodes: vec![],
            is_last: true,
            total_scanned: 0,
        });
        roundtrip_response(&AgentResponse::FileContents { results: vec![] });
        roundtrip_response(&AgentResponse::Stats { entries: vec![] });
    }

    // ---- Protocol version ----

    #[test]
    fn protocol_version_is_2() {
        assert_eq!(PROTOCOL_VERSION, 2);
    }

    // ---- ListBackups / BackupList roundtrip ----

    #[test]
    fn request_list_backups_roundtrip() {
        roundtrip_request(&AgentRequest::ListBackups {
            backup_dir: "/var/www/.remote-merge-backup".into(),
        });
    }

    #[test]
    fn response_backup_list_roundtrip() {
        roundtrip_response(&AgentResponse::BackupList {
            sessions: vec![AgentBackupSession {
                session_id: "20260311-120000".into(),
                files: vec![
                    AgentBackupFile {
                        path: "index.html".into(),
                        size: 1024,
                    },
                    AgentBackupFile {
                        path: "css/style.css".into(),
                        size: 512,
                    },
                ],
            }],
        });
    }

    #[test]
    fn response_backup_list_empty_roundtrip() {
        roundtrip_response(&AgentResponse::BackupList { sessions: vec![] });
    }

    // ---- RestoreBackup / RestoreResult roundtrip ----

    #[test]
    fn request_restore_backup_roundtrip() {
        roundtrip_request(&AgentRequest::RestoreBackup {
            backup_dir: "/var/www/.remote-merge-backup".into(),
            session_id: "20260311-120000".into(),
            files: vec!["index.html".into(), "css/style.css".into()],
            root_dir: "/var/www".into(),
        });
    }

    #[test]
    fn response_restore_result_roundtrip() {
        roundtrip_response(&AgentResponse::RestoreResult {
            results: vec![
                AgentRestoreFileResult {
                    path: "index.html".into(),
                    success: true,
                    error: None,
                },
                AgentRestoreFileResult {
                    path: "missing.txt".into(),
                    success: false,
                    error: Some("file not found".into()),
                },
            ],
        });
    }

    // ---- AgentBackupSession / AgentBackupFile roundtrip ----

    #[test]
    fn agent_backup_session_serde_roundtrip() {
        let session = AgentBackupSession {
            session_id: "20260311-153000".into(),
            files: vec![
                AgentBackupFile {
                    path: "app/main.rs".into(),
                    size: 2048,
                },
                AgentBackupFile {
                    path: "config.toml".into(),
                    size: 256,
                },
            ],
        };
        let json = serde_json::to_string(&session).unwrap();
        let decoded: AgentBackupSession = serde_json::from_str(&json).unwrap();
        assert_eq!(session, decoded);
    }

    // ---- AgentRestoreFileResult skip_serializing_if ----

    #[test]
    fn restore_file_result_skips_none_error() {
        let result = AgentRestoreFileResult {
            path: "test.txt".into(),
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("error"));
    }
}
