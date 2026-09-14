use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::app::Side;
use crate::backup::{local_target_identity, next_session_id, remote_target_identity};
use crate::config::AppConfig;

pub(crate) struct BackupStore {
    root: Option<PathBuf>,
    startup_directory: PathBuf,
    now: DateTime<Utc>,
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
        }
    }

    pub(crate) fn reserve_session(&self) -> anyhow::Result<String> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("backup store location could not be determined"))?;
        create_dir_owner_only(root)?;
        let reservations = root.join("reservations");
        create_dir_owner_only(&reservations)?;
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

    pub(crate) fn save(
        &self,
        config: &AppConfig,
        target: &Side,
        session_id: &str,
        rel_path: &str,
        content: &[u8],
    ) -> anyhow::Result<String> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("backup store location could not be determined"))?;
        let description =
            match target {
                Side::Local => local_target_identity(&config.local, &self.startup_directory)
                    .root_dir()
                    .display()
                    .to_string(),
                Side::Remote(name) => {
                    let identity =
                        remote_target_identity(config.servers.get(name).ok_or_else(|| {
                            anyhow::anyhow!("Server '{}' not found in config", name)
                        })?);
                    format!(
                        "{}:{}:{}",
                        identity.host(),
                        identity.port(),
                        identity.root_dir().display()
                    )
                }
            };
        let key = format!("{:x}", Sha256::digest(description.as_bytes()));
        let target_dir = root.join("targets").join(&key);
        create_dir_owner_only(&target_dir)?;
        write_file_owner_only(&target_dir.join("target.txt"), description.as_bytes())?;
        let destination = root
            .join("sessions")
            .join(session_id)
            .join(key)
            .join("files")
            .join(rel_path);
        if let Some(parent) = destination.parent() {
            create_dir_owner_only(parent)?;
        }
        write_file_owner_only(&destination, content)?;
        Ok(format!("{session_id}/{rel_path}"))
    }
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
