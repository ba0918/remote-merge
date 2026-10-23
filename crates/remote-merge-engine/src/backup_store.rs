use std::fs::{self, OpenOptions};

use anyhow::Context as _;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::backup::{local_target_identity, next_session_id, remote_target_identity};
use crate::side::Side;
use remote_merge_config::AppConfig;
use remote_merge_core::sha256;

static TEMP_ENTRY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct BackupStore {
    root: Option<PathBuf>,
    startup_directory: PathBuf,
    now: DateTime<Utc>,
    reservation_time: ReservationTime,
}

/// セッション ID の日時にする、セッションを予約する時点の時刻の決め方
#[derive(Clone, Copy, Debug)]
pub enum ReservationTime {
    /// 予約するときのシステムの時刻
    System,
    /// 決めた時刻
    Fixed(DateTime<Utc>),
}

impl ReservationTime {
    fn now(self) -> DateTime<Utc> {
        match self {
            Self::System => Utc::now(),
            Self::Fixed(time) => time,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StoredBackup<'a> {
    File {
        path: &'a str,
        real_path: &'a Path,
    },
    Symlink {
        path: &'a str,
        link_target: &'a Path,
        #[serde(skip_serializing_if = "Option::is_none")]
        expected_target: Option<&'a Path>,
        #[serde(skip_serializing_if = "Option::is_none")]
        real_parent: Option<&'a Path>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StoredBackupRecord {
    File {
        path: String,
        real_path: PathBuf,
    },
    Symlink {
        path: String,
        link_target: PathBuf,
        #[serde(default)]
        expected_target: Option<PathBuf>,
        #[serde(default)]
        real_parent: Option<PathBuf>,
    },
}

pub enum BackupRecord {
    File {
        real_path: PathBuf,
        content: Vec<u8>,
    },
    Symlink {
        link_target: PathBuf,
        expected_target: Option<PathBuf>,
        real_parent: Option<PathBuf>,
    },
}

pub struct SymlinkBackup<'a> {
    pub link_target: &'a Path,
    pub expected: Option<(&'a Path, &'a Path)>,
}

impl BackupStore {
    /// `now` は期限切れの判定に使う時刻、`reservation_time` はセッション ID の日時にする時刻
    pub fn new(
        root: Option<PathBuf>,
        startup_directory: PathBuf,
        now: DateTime<Utc>,
        reservation_time: ReservationTime,
    ) -> Self {
        Self {
            root,
            startup_directory,
            now,
            reservation_time,
        }
    }

    /// 集約先の場所（決められなかったときは誤り）
    fn root(&self) -> anyhow::Result<&Path> {
        self.root
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("backup store location could not be determined"))
    }

    pub fn reserve_session(&self) -> anyhow::Result<String> {
        let root = self.root()?;
        // 集約先と予約の場所を作れなければ、どのファイルも書き込む前にコマンド全体を止める
        // （docs/ir/backup/failure.md の REQ-backup-045）
        create_dir_owner_only(root).map_err(|error| store_failure(root, &error))?;
        let reservations = root.join("reservations");
        create_dir_owner_only(&reservations)
            .map_err(|error| store_failure(&reservations, &error))?;
        let mut existing = fs::read_dir(&reservations)
            .map_err(|error| store_failure(&reservations, &error))?
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect::<Vec<_>>();
        // ID の日時は、コマンドを始めた時刻ではなく予約する時刻にする。始めた時刻にすると、確認の問いで
        // 何日も待った実行が、期限切れの整理で予約ごと消えた同じ秒の ID を受け取る（REQ-backup-023）
        let reserved_at = self.reservation_time.now();
        loop {
            let refs = existing.iter().map(String::as_str).collect::<Vec<_>>();
            let id = next_session_id(reserved_at, &refs).map_err(|_| {
                anyhow::anyhow!(
                    "backup session id could not be allocated: the sequence for this time is at its maximum"
                )
            })?;
            let reservation = reservations.join(&id);
            match fs::create_dir(&reservation) {
                Ok(()) => {
                    set_dir_permissions(&reservation)?;
                    return Ok(id);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    existing.push(id)
                }
                Err(error) => return Err(store_failure(&reservation, &error)),
            }
        }
    }

    /// 書き込み先 `target` のセッション `session_id` の領域を、ファイルを書き込む前に作る。作れなければ
    /// ファイルごとの失敗にせず、コマンド全体を止める（REQ-backup-045）
    pub fn prepare_target(
        &self,
        config: &AppConfig,
        target: &Side,
        session_id: &str,
    ) -> anyhow::Result<()> {
        let root = self.root()?;
        let description = target_description(config, target, &self.startup_directory)?;
        let key = sha256::hex_of_bytes(description.as_bytes());
        let target_dir = root.join("targets").join(&key);
        create_dir_owner_only(&target_dir).map_err(|error| store_failure(&target_dir, &error))?;
        let description_file = target_dir.join("target.txt");
        write_file_owner_only(&description_file, description.as_bytes())
            .map_err(|error| store_failure(&description_file, &error))?;
        let records = root
            .join("sessions")
            .join(session_id)
            .join(key)
            .join("records");
        create_dir_owner_only(&records).map_err(|error| store_failure(&records, &error))
    }

    pub fn save_file(
        &self,
        config: &AppConfig,
        target: &Side,
        session_id: &str,
        rel_path: &str,
        real_path: &Path,
        content: &[u8],
    ) -> anyhow::Result<String> {
        let record = StoredBackup::File {
            path: rel_path,
            real_path,
        };
        self.save(config, target, session_id, rel_path, &record, Some(content))
    }

    pub fn save_symlink(
        &self,
        config: &AppConfig,
        target: &Side,
        session_id: &str,
        rel_path: &str,
        link: &SymlinkBackup<'_>,
    ) -> anyhow::Result<String> {
        let record = StoredBackup::Symlink {
            path: rel_path,
            link_target: link.link_target,
            expected_target: link.expected.map(|(target, _)| target),
            real_parent: link.expected.map(|(_, parent)| parent),
        };
        self.save(config, target, session_id, rel_path, &record, None)
    }

    fn save(
        &self,
        config: &AppConfig,
        target: &Side,
        session_id: &str,
        rel_path: &str,
        record: &StoredBackup<'_>,
        content: Option<&[u8]>,
    ) -> anyhow::Result<String> {
        let root = self.root()?;
        let description = target_description(config, target, &self.startup_directory)?;
        let key = sha256::hex_of_bytes(description.as_bytes());
        let target_dir = root.join("targets").join(&key);
        create_dir_owner_only(&target_dir)?;
        write_file_owner_only(&target_dir.join("target.txt"), description.as_bytes())?;
        let entry_key = sha256::hex_of_bytes(rel_path.as_bytes());
        let target_session_dir = root.join("sessions").join(session_id).join(key);
        let destination = target_session_dir.join("records").join(entry_key);
        let records_dir = destination
            .parent()
            .ok_or_else(|| anyhow::anyhow!("backup record path has no parent"))?;
        create_dir_owner_only(records_dir)?;
        let pending_dir = target_session_dir.join("pending");
        create_dir_owner_only(&pending_dir)?;
        let temporary = create_temporary_entry(&pending_dir)?;
        let write_result = (|| -> anyhow::Result<()> {
            if let Some(content) = content {
                write_file_owner_only(&temporary.join("content"), content)?;
            }
            write_file_owner_only(&temporary.join("record.json"), &serde_json::to_vec(record)?)?;
            fs::rename(&temporary, &destination)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_dir_all(&temporary);
        }
        let _ = fs::remove_dir(&pending_dir);
        write_result?;
        Ok(format!("{session_id}/{rel_path}"))
    }

    /// セッションを終える。前もって作った領域のうち何も保存しなかったもの（空のディレクトリ）と、
    /// 何も保存しなかったセッションそのものとその予約を除く。集約先の共有の場所（sessions・targets・
    /// reservations と集約先そのもの）と書き込み先の記録は、同時に動く別の実行が使うので消さない
    pub fn finish_session(&self, session_id: &str) {
        let Some(root) = &self.root else { return };
        let session_dir = root.join("sessions").join(session_id);
        match fs::read_dir(&session_dir) {
            Ok(areas) => {
                for area in areas {
                    match area {
                        Ok(area) => {
                            tidy(&area.path().join("records"));
                            tidy(&area.path());
                        }
                        Err(error) => warn_untidy(&session_dir, &error),
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => warn_untidy(&session_dir, &error),
        }
        tidy(&session_dir);
        if session_dir.exists() {
            return;
        }
        tidy(&root.join("reservations").join(session_id));
    }

    pub fn list_sessions(
        &self,
        config: &AppConfig,
        target: &Side,
    ) -> anyhow::Result<Vec<crate::service::types::BackupSession>> {
        let root = self.root()?;
        let target_key = target_key(config, target, &self.startup_directory)?;
        // 中身が見つからないものだけをセッションなしとして扱い、ほかの読み込みや解析の失敗は
        // そのパスと理由を示して止める（docs/ir/backup/rollback-cli.md の REQ-backup-041・044）
        let sessions_dir = root.join("sessions");
        let Some(entries) = unless_not_found(fs::read_dir(&sessions_dir), &sessions_dir)? else {
            return Ok(Vec::new());
        };
        let mut sessions = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| unreadable_store(&sessions_dir, &error))?;
            let Some(session_id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if crate::backup::parse_session_id(&session_id).is_none() {
                continue;
            }
            let records_dir = entry.path().join(&target_key).join("records");
            let Some(records) = unless_not_found(fs::read_dir(&records_dir), &records_dir)? else {
                continue;
            };
            let mut files = Vec::new();
            let mut complete = true;
            for record_dir in records {
                let record_dir = record_dir
                    .map_err(|error| unreadable_store(&records_dir, &error))?
                    .path();
                let Some(record) = read_stored_record(&record_dir)? else {
                    complete = false;
                    break;
                };
                match record {
                    StoredBackupRecord::File { path, .. } => {
                        let content = record_dir.join("content");
                        let Some(metadata) = unless_not_found(fs::metadata(&content), &content)?
                        else {
                            complete = false;
                            break;
                        };
                        files.push(crate::service::types::BackupEntry {
                            path,
                            size: Some(metadata.len()),
                            link_target: None,
                        });
                    }
                    StoredBackupRecord::Symlink {
                        path, link_target, ..
                    } => {
                        files.push(crate::service::types::BackupEntry {
                            path,
                            size: None,
                            link_target: Some(link_target.to_string_lossy().into_owned()),
                        });
                    }
                }
            }
            if complete && !files.is_empty() {
                files.sort_by(|left, right| left.path.cmp(&right.path));
                sessions.push(crate::service::types::BackupSession::new(
                    session_id, files, false,
                ));
            }
        }
        sessions.sort_by(|left, right| {
            crate::backup::compare_session_ids(&right.session_id, &left.session_id)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(sessions)
    }

    pub fn read_record(
        &self,
        config: &AppConfig,
        target: &Side,
        session_id: &str,
        rel_path: &str,
    ) -> anyhow::Result<BackupRecord> {
        let root = self.root()?;
        let target_key = target_key(config, target, &self.startup_directory)?;
        let records_dir = root
            .join("sessions")
            .join(session_id)
            .join(target_key)
            .join("records");
        let entry_key = sha256::hex_of_bytes(rel_path.as_bytes());
        let record_dir = records_dir.join(entry_key);
        let record_file = record_dir.join("record.json");
        let stored = fs::read(&record_file)
            .with_context(|| format!("cannot read the backup record {}", record_file.display()))?;
        let record = parse_stored_record(&stored, &record_file)?;
        match record {
            StoredBackupRecord::File { path, real_path } if path == rel_path => {
                Ok(BackupRecord::File {
                    real_path,
                    content: {
                        let content = record_dir.join("content");
                        fs::read(&content).with_context(|| {
                            format!("cannot read the backed-up content {}", content.display())
                        })?
                    },
                })
            }
            StoredBackupRecord::File { .. } => anyhow::bail!("backup record path does not match"),
            StoredBackupRecord::Symlink {
                path,
                link_target,
                expected_target,
                real_parent,
            } if path == rel_path => Ok(BackupRecord::Symlink {
                link_target,
                expected_target,
                real_parent,
            }),
            StoredBackupRecord::Symlink { .. } => {
                anyhow::bail!("backup record path does not match")
            }
        }
    }
}

/// 一つの記録のディレクトリの記録を読んで解析する。記録がなければ None
fn read_stored_record(record_dir: &Path) -> anyhow::Result<Option<StoredBackupRecord>> {
    let path = record_dir.join("record.json");
    let Some(bytes) = unless_not_found(fs::read(&path), &path)? else {
        return Ok(None);
    };
    parse_stored_record(&bytes, &path).map(Some)
}

/// 記録を解析する。記録の path は外から書き換えられうるので、root_dir の下の一つの項目を指す
/// 形（root_dir 全体を指す値を含まない）でなければ解析できない記録として扱う
fn parse_stored_record(bytes: &[u8], path: &Path) -> anyhow::Result<StoredBackupRecord> {
    let record: StoredBackupRecord = serde_json::from_slice(bytes).map_err(|error| {
        anyhow::anyhow!(
            "backup record could not be parsed: {}: {error}",
            path.display()
        )
    })?;
    let (StoredBackupRecord::File { path: rel_path, .. }
    | StoredBackupRecord::Symlink { path: rel_path, .. }) = &record;
    remote_merge_core::relative_path::EntryPath::parse(rel_path).map_err(|error| {
        anyhow::anyhow!(
            "backup record could not be parsed: {}: {error}",
            path.display()
        )
    })?;
    Ok(record)
}

/// 読み込みの結果。見つからなければ None、ほかの失敗はパスと理由を示すエラー
fn unless_not_found<T>(result: std::io::Result<T>, path: &Path) -> anyhow::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(unreadable_store(path, &error)),
    }
}

/// 集約先の中身の読み込みの失敗。どのパスかを示す
fn unreadable_store(path: &Path, error: &std::io::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "backup store could not be read: {}: {error}",
        path.display()
    )
}

/// 集約先の準備の失敗。どの場所で失敗したかを示す
fn store_failure(path: &Path, error: &std::io::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "backup store could not be prepared: {}: {error}",
        path.display()
    )
}

fn create_temporary_entry(parent: &Path) -> std::io::Result<PathBuf> {
    loop {
        let sequence = TEMP_ENTRY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".pending-{}-{sequence}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => {
                if let Err(error) = set_dir_permissions(&path) {
                    let _ = fs::remove_dir(&path);
                    return Err(error);
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

/// 設定にあるすべての書き込み先のキー。期限切れの整理の対象になる
fn configured_target_keys(
    config: &AppConfig,
    startup_directory: &Path,
) -> anyhow::Result<Vec<String>> {
    std::iter::once(Side::Local)
        .chain(config.servers.keys().cloned().map(Side::Remote))
        .map(|target| target_key(config, &target, startup_directory))
        .collect()
}

/// 書き込み先の識別から求めた、集約先の中のキー
fn target_key(
    config: &AppConfig,
    target: &Side,
    startup_directory: &Path,
) -> anyhow::Result<String> {
    let description = target_description(config, target, startup_directory)?;
    Ok(sha256::hex_of_bytes(description.as_bytes()))
}

fn target_description(
    config: &AppConfig,
    target: &Side,
    startup_directory: &Path,
) -> anyhow::Result<String> {
    Ok(match target {
        Side::Local => local_target_identity(&config.local, startup_directory)
            .root_dir()
            .display()
            .to_string(),
        Side::Remote(name) => {
            let identity = remote_target_identity(
                config
                    .servers
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", name))?,
            );
            format!(
                "{}:{}:{}",
                identity.host(),
                identity.port(),
                identity.root_dir().display()
            )
        }
    })
}

/// 空のディレクトリを消す。ないか空でないのは、消すものがないか中に保存したものがあるので、そのまま
/// にする。ほかの失敗は、消せなかったことを警告に残す（集約先を片付けられなくても結果は変わらない）
fn tidy(path: &Path) {
    match fs::remove_dir(path) {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) => {}
        Err(error) => warn_untidy(path, &error),
    }
}

fn warn_untidy(path: &Path, error: &std::io::Error) {
    tracing::warn!(
        "could not tidy the backup store at {}: {error}",
        path.display()
    );
}

fn create_dir_owner_only(path: &Path) -> std::io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    set_dir_permissions(path)
}

#[cfg(unix)]
fn set_dir_permissions(path: &Path) -> std::io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

fn write_file_owner_only(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(content)?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[path = "backup_store_cleanup.rs"]
mod cleanup;

#[cfg(test)]
#[path = "backup_store_tests.rs"]
mod tests;
