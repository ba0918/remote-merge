use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::app::Side;
use crate::backup::{local_target_identity, next_session_id, remote_target_identity};
use crate::config::AppConfig;

static TEMP_ENTRY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct BackupStore {
    root: Option<PathBuf>,
    startup_directory: PathBuf,
    now: DateTime<Utc>,
    initialization_error: Option<String>,
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
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StoredBackupRecord {
    File { path: String, real_path: PathBuf },
    Symlink { path: String, link_target: PathBuf },
}

pub(crate) enum BackupRecord {
    File {
        real_path: PathBuf,
        content: Vec<u8>,
    },
    Symlink,
}

impl BackupStore {
    pub(crate) fn new(
        root: Option<PathBuf>,
        startup_directory: PathBuf,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            root,
            startup_directory,
            now,
            initialization_error: None,
        }
    }

    pub(crate) fn reserve_session(&mut self) -> anyhow::Result<String> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("backup store location could not be determined"))?;
        if let Err(error) = create_dir_owner_only(root) {
            self.initialization_error = Some(error.to_string());
            return Ok(next_session_id(self.now, &[]));
        }
        let reservations = root.join("reservations");
        if let Err(error) = create_dir_owner_only(&reservations) {
            self.initialization_error = Some(error.to_string());
            return Ok(next_session_id(self.now, &[]));
        }
        let mut existing = fs::read_dir(&reservations)?
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect::<Vec<_>>();
        loop {
            let refs = existing.iter().map(String::as_str).collect::<Vec<_>>();
            let id = next_session_id(self.now, &refs);
            let reservation = reservations.join(&id);
            match fs::create_dir(&reservation) {
                Ok(()) => {
                    set_dir_permissions(&reservation)?;
                    return Ok(id);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    existing.push(id)
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub(crate) fn save_file(
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

    pub(crate) fn save_symlink(
        &self,
        config: &AppConfig,
        target: &Side,
        session_id: &str,
        rel_path: &str,
        link_target: &Path,
    ) -> anyhow::Result<String> {
        let record = StoredBackup::Symlink {
            path: rel_path,
            link_target,
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
        if let Some(error) = &self.initialization_error {
            anyhow::bail!(error.clone());
        }
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("backup store location could not be determined"))?;
        let description = target_description(config, target, &self.startup_directory)?;
        let key = format!("{:x}", Sha256::digest(description.as_bytes()));
        let target_dir = root.join("targets").join(&key);
        create_dir_owner_only(&target_dir)?;
        write_file_owner_only(&target_dir.join("target.txt"), description.as_bytes())?;
        let entry_key = format!("{:x}", Sha256::digest(rel_path.as_bytes()));
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

    pub(crate) fn finish_session(&self, session_id: &str) {
        let Some(root) = &self.root else { return };
        if root.join("sessions").join(session_id).exists() {
            return;
        }
        let reservations = root.join("reservations");
        let _ = fs::remove_dir(reservations.join(session_id));
        let _ = fs::remove_dir(&reservations);
        let _ = fs::remove_dir(root);
    }

    pub(crate) fn cleanup_expired(&self, config: &AppConfig) -> anyhow::Result<usize> {
        let Some(root) = &self.root else {
            return Ok(0);
        };
        let sessions_dir = root.join("sessions");
        let Ok(sessions) = fs::read_dir(&sessions_dir) else {
            return Ok(0);
        };
        let target_keys = configured_target_keys(config, &self.startup_directory)?;
        let mut removed = 0;
        for session in sessions.filter_map(Result::ok) {
            let Some(session_id) = session.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if crate::backup::is_session_expired(
                &session_id,
                config.backup.retention_days,
                self.now,
            ) != Some(true)
            {
                continue;
            }
            for target_key in &target_keys {
                let target_dir = session.path().join(target_key);
                if target_dir.exists() {
                    fs::remove_dir_all(target_dir)?;
                    removed += 1;
                }
            }
            if fs::read_dir(session.path())?.next().is_none() {
                fs::remove_dir(session.path())?;
            }
        }
        Ok(removed)
    }

    pub(crate) fn list_sessions(
        &self,
        config: &AppConfig,
        target: &Side,
    ) -> anyhow::Result<Vec<crate::service::types::BackupSession>> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("backup store location could not be determined"))?;
        let description = target_description(config, target, &self.startup_directory)?;
        let target_key = format!("{:x}", Sha256::digest(description.as_bytes()));
        let Ok(entries) = fs::read_dir(root.join("sessions")) else {
            return Ok(Vec::new());
        };
        let mut sessions = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let Some(session_id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if crate::backup::parse_session_id(&session_id).is_none() {
                continue;
            }
            let Ok(records) = fs::read_dir(entry.path().join(&target_key).join("records")) else {
                continue;
            };
            let mut files = Vec::new();
            let mut complete = true;
            for record_dir in records.filter_map(Result::ok) {
                let record = fs::read(record_dir.path().join("record.json"))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<StoredBackupRecord>(&bytes).ok());
                match record {
                    Some(StoredBackupRecord::File { path, .. }) => {
                        let Ok(metadata) = fs::metadata(record_dir.path().join("content")) else {
                            complete = false;
                            break;
                        };
                        files.push(crate::service::types::BackupEntry {
                            path,
                            size: Some(metadata.len()),
                            link_target: None,
                        });
                    }
                    Some(StoredBackupRecord::Symlink { path, link_target }) => {
                        files.push(crate::service::types::BackupEntry {
                            path,
                            size: None,
                            link_target: Some(link_target.to_string_lossy().into_owned()),
                        });
                    }
                    None => {
                        complete = false;
                        break;
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

    pub(crate) fn read_record(
        &self,
        config: &AppConfig,
        target: &Side,
        session_id: &str,
        rel_path: &str,
    ) -> anyhow::Result<BackupRecord> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("backup store location could not be determined"))?;
        let description = target_description(config, target, &self.startup_directory)?;
        let target_key = format!("{:x}", Sha256::digest(description.as_bytes()));
        let entry_key = format!("{:x}", Sha256::digest(rel_path.as_bytes()));
        let record_dir = root
            .join("sessions")
            .join(session_id)
            .join(target_key)
            .join("records")
            .join(entry_key);
        let record: StoredBackupRecord =
            serde_json::from_slice(&fs::read(record_dir.join("record.json"))?)?;
        match record {
            StoredBackupRecord::File { path, real_path } if path == rel_path => {
                Ok(BackupRecord::File {
                    real_path,
                    content: fs::read(record_dir.join("content"))?,
                })
            }
            StoredBackupRecord::File { .. } => anyhow::bail!("backup record path does not match"),
            StoredBackupRecord::Symlink { .. } => Ok(BackupRecord::Symlink),
        }
    }
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

fn configured_target_keys(
    config: &AppConfig,
    startup_directory: &Path,
) -> anyhow::Result<Vec<String>> {
    std::iter::once(Side::Local)
        .chain(config.servers.keys().cloned().map(Side::Remote))
        .map(|target| {
            let description = target_description(config, &target, startup_directory)?;
            Ok(format!("{:x}", Sha256::digest(description.as_bytes())))
        })
        .collect()
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

fn create_dir_owner_only(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        set_dir_permissions(path)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_dir_permissions(path: &Path) -> std::io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}
#[cfg(not(unix))]
fn set_dir_permissions(_: &Path) -> std::io::Result<()> {
    Ok(())
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
