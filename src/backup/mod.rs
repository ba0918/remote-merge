//! 集約バックアップの場所、書き込み先の識別、セッション ID を扱う。

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::config::{LocalConfig, ServerConfig};

/// バックアップディレクトリ名
pub const BACKUP_DIR_NAME: &str = ".remote-merge-backup";

/// XDG のデータディレクトリとホームディレクトリから集約先を決める。
pub fn backup_store_path(xdg_data_home: Option<&Path>, home_dir: Option<&Path>) -> Option<PathBuf> {
    let data_home = match xdg_data_home {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        _ => home_dir?.join(".local/share"),
    };
    Some(data_home.join("remote-merge/backups"))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalTargetIdentity {
    root_dir: PathBuf,
}

impl LocalTargetIdentity {
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteTargetIdentity {
    host: String,
    port: u16,
    root_dir: PathBuf,
}

impl RemoteTargetIdentity {
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }
}

pub fn local_target_identity(
    config: &LocalConfig,
    startup_directory: &Path,
) -> LocalTargetIdentity {
    let root_dir = if config.root_dir.is_absolute() {
        config.root_dir.clone()
    } else {
        startup_directory.join(&config.root_dir)
    };
    LocalTargetIdentity::new(root_dir)
}

pub fn remote_target_identity(config: &ServerConfig) -> RemoteTargetIdentity {
    RemoteTargetIdentity {
        host: config.host.clone(),
        port: config.port,
        root_dir: config.root_dir.clone(),
    }
}

/// 現在時刻からバックアップ用タイムスタンプ（= セッションID）を生成する。
pub fn backup_timestamp() -> String {
    Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

/// 指定時刻と既存 ID から、次のバックアップセッション ID を決める。
pub fn next_session_id(now: DateTime<Utc>, existing_ids: &[&str]) -> String {
    let timestamp = now.format("%Y%m%d-%H%M%S").to_string();
    let next_sequence = existing_ids
        .iter()
        .filter_map(|id| parse_session_id(id))
        .filter(|id| id.timestamp.format("%Y%m%d-%H%M%S").to_string() == timestamp)
        .map(|id| id.sequence)
        .max()
        .map_or(1, |sequence| sequence + 1);
    match next_sequence {
        1 => timestamp,
        sequence => format!("{timestamp}-{sequence}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionId {
    timestamp: DateTime<Utc>,
    sequence: u64,
}

pub fn parse_session_id(session_id: &str) -> Option<SessionId> {
    let timestamp_text = session_id.get(..15)?;
    let timestamp = parse_backup_timestamp(timestamp_text)?;
    let sequence = match session_id.get(15..) {
        Some("") => 1,
        Some(suffix) => suffix.strip_prefix('-')?.parse::<u64>().ok()?,
        None => return None,
    };
    if sequence < 2 && session_id.len() > 15 {
        return None;
    }
    Some(SessionId {
        timestamp,
        sequence,
    })
}

pub fn compare_session_ids(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    Some(parse_session_id(left)?.cmp(&parse_session_id(right)?))
}

pub fn is_session_expired(
    session_id: &str,
    retention_days: u32,
    now: DateTime<Utc>,
) -> Option<bool> {
    let session = parse_session_id(session_id)?;
    Some(
        now.signed_duration_since(session.timestamp)
            >= chrono::Duration::days(i64::from(retention_days)),
    )
}

/// タイムスタンプ文字列をパースして DateTime<Utc> に変換する。
fn parse_backup_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(ts, "%Y%m%d-%H%M%S")
        .ok()
        .and_then(|naive| naive.and_local_timezone(Utc).single())
}

/// セッションディレクトリ名がタイムスタンプ形式か検証する。
///
/// `"20240115-140000"` → `Some("20240115-140000")`
/// `"not-a-timestamp"` → `None`
pub fn extract_timestamp(name: &str) -> Option<&str> {
    parse_session_id(name)?;
    name.get(..15)
}

/// ローカルファイルのバックアップをセッションディレクトリに作成する。
///
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthMethod, LocalConfig, ServerConfig};
    use chrono::{Datelike, TimeZone};

    fn remote_config(user: &str, host: &str, root_dir: &str) -> ServerConfig {
        ServerConfig {
            host: host.to_string(),
            port: 22,
            user: user.to_string(),
            auth: AuthMethod::Key,
            password: None,
            key: None,
            root_dir: PathBuf::from(root_dir),
            ssh_options: None,
            sudo: false,
            file_permissions: None,
            dir_permissions: None,
        }
    }

    #[test]
    fn backup_store_uses_home_when_xdg_data_home_is_unset() {
        assert_eq!(
            backup_store_path(None, Some(Path::new("/home/user"))),
            Some(PathBuf::from(
                "/home/user/.local/share/remote-merge/backups"
            ))
        );
    }

    #[test]
    fn backup_store_uses_home_when_xdg_data_home_is_empty() {
        assert_eq!(
            backup_store_path(Some(Path::new("")), Some(Path::new("/home/user"))),
            Some(PathBuf::from(
                "/home/user/.local/share/remote-merge/backups"
            ))
        );
    }

    #[test]
    fn backup_store_is_unavailable_without_xdg_data_home_or_home() {
        assert_eq!(backup_store_path(None, None), None);
    }

    #[test]
    fn remote_targets_with_different_users_have_the_same_identity() {
        let first = remote_config("alice", "example.com", "/srv/app");
        let second = remote_config("bob", "example.com", "/srv/app");

        assert_eq!(
            remote_target_identity(&first),
            remote_target_identity(&second)
        );
    }

    #[test]
    fn remote_targets_with_different_hosts_have_different_identities() {
        let first = remote_config("deploy", "old.example.com", "/srv/app");
        let second = remote_config("deploy", "new.example.com", "/srv/app");

        assert_ne!(
            remote_target_identity(&first),
            remote_target_identity(&second)
        );
    }

    #[test]
    fn remote_targets_with_different_ports_have_different_identities() {
        let first = remote_config("deploy", "example.com", "/srv/app");
        let mut second = first.clone();
        second.port = 2222;

        assert_ne!(
            remote_target_identity(&first),
            remote_target_identity(&second)
        );
    }

    #[test]
    fn trailing_slash_does_not_change_remote_target_identity() {
        let first = remote_config("deploy", "example.com", "/srv/app");
        let second = remote_config("deploy", "example.com", "/srv/app/");

        assert_eq!(
            remote_target_identity(&first),
            remote_target_identity(&second)
        );
    }

    #[test]
    fn relative_local_root_is_resolved_from_startup_directory() {
        let config = LocalConfig {
            root_dir: PathBuf::from("project"),
        };

        assert_eq!(
            local_target_identity(&config, Path::new("/work")),
            LocalTargetIdentity::new(PathBuf::from("/work/project"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_target_identity_does_not_follow_root_symlink() {
        let temp = tempfile::tempdir().unwrap();
        let release = temp.path().join("releases/a");
        std::fs::create_dir_all(&release).unwrap();
        std::os::unix::fs::symlink(&release, temp.path().join("current")).unwrap();
        let config = LocalConfig {
            root_dir: PathBuf::from("current"),
        };

        assert_eq!(
            local_target_identity(&config, temp.path()),
            LocalTargetIdentity::new(temp.path().join("current"))
        );
    }

    #[test]
    fn host_aliases_have_different_remote_target_identities() {
        let address = remote_config("deploy", "127.0.0.1", "/srv/app");
        let hostname = remote_config("deploy", "localhost", "/srv/app");

        assert_ne!(
            remote_target_identity(&address),
            remote_target_identity(&hostname)
        );
    }

    #[test]
    fn test_extract_timestamp_valid() {
        assert_eq!(
            extract_timestamp("20240115-140000"),
            Some("20240115-140000")
        );
    }

    #[test]
    fn test_extract_timestamp_invalid() {
        assert_eq!(extract_timestamp("not-a-timestamp"), None);
        assert_eq!(extract_timestamp("config.ts"), None);
        assert_eq!(extract_timestamp("2024011a-140000"), None);
        assert_eq!(extract_timestamp(""), None);
        assert_eq!(extract_timestamp("20240115_140000"), None); // アンダースコア
    }

    #[test]
    fn test_parse_backup_timestamp() {
        let dt = parse_backup_timestamp("20240115-140000");
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 15);
    }

    #[test]
    fn same_time_uses_second_session_suffix() {
        let now = Utc.with_ymd_and_hms(2026, 9, 14, 15, 52, 50).unwrap();
        let first = next_session_id(now, &[]);
        let second = next_session_id(now, &[first.as_str()]);

        assert_eq!(first, "20260914-155250");
        assert_eq!(second, "20260914-155250-2");
    }

    #[test]
    fn session_id_with_sequence_is_parsed() {
        let parsed = parse_session_id("20260914-155250-12").unwrap();

        assert_eq!(
            parsed.timestamp,
            Utc.with_ymd_and_hms(2026, 9, 14, 15, 52, 50).unwrap()
        );
        assert_eq!(parsed.sequence, 12);
    }

    #[test]
    fn tenth_session_sorts_after_ninth_session() {
        assert_eq!(
            compare_session_ids("20260914-155250-10", "20260914-155250-9"),
            Some(std::cmp::Ordering::Greater)
        );
    }

    #[test]
    fn session_expires_at_retention_boundary() {
        let now = Utc.with_ymd_and_hms(2026, 9, 21, 15, 52, 50).unwrap();

        assert_eq!(is_session_expired("20260914-155250-2", 7, now), Some(true));
    }
}
