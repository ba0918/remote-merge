//! 読みに行ったディレクトリが、もうない・ディレクトリでなくなっていたときの答え
use super::*;

// @kotowari[REQ-scan-017]
#[test]
fn a_directory_that_became_a_file_or_went_away_is_answered_as_changed() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("was_dir");
    std::fs::write(&file, "now a file").unwrap();
    for path in [file, temp.path().join("gone")] {
        let tree = collect_below(&path, b"x", &Exclude::default());
        assert_eq!(tree.changed, [Vec::<u8>::new()], "{path:?}");
        assert!(tree.unreadable.is_empty(), "{path:?}");
        assert!(tree.entries.is_empty(), "{path:?}");
    }
}

#[test]
fn only_a_missing_or_non_directory_path_counts_as_changed() {
    use std::io::{Error, ErrorKind};
    assert!(went_away(&Error::from(ErrorKind::NotFound)));
    assert!(went_away(&Error::from(ErrorKind::NotADirectory)));
    assert!(!went_away(&Error::from(ErrorKind::PermissionDenied)));
    assert!(!went_away(&Error::other("io")));
}
