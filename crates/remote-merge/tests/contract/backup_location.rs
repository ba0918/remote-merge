use std::path::{Path, PathBuf};

use remote_merge::backup::backup_store_path;

// @kotowari[EX-backup-007]
#[test]
fn an_absolute_xdg_data_home_selects_its_backup_directory() {
    assert_eq!(
        backup_store_path(Some(Path::new("/var/data")), Some(Path::new("/home/user"))),
        Some(PathBuf::from("/var/data/remote-merge/backups"))
    );
}

// @kotowari[EX-backup-008]
#[test]
fn a_relative_xdg_data_home_falls_back_to_the_users_data_directory() {
    assert_eq!(
        backup_store_path(
            Some(Path::new("relative/data")),
            Some(Path::new("/home/user"))
        ),
        Some(PathBuf::from(
            "/home/user/.local/share/remote-merge/backups"
        ))
    );
}
