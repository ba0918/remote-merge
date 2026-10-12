//! 子の一覧とハッシュの要求

use super::support::*;

#[test]
fn a_listing_larger_than_one_frame_is_returned_whole_and_the_agent_stays_usable() {
    // 4000 バイトのリンク先の symlink 5000 個は、リンク先だけでフレームの上限（16 MiB）を超える
    let tmp = TempDir::new().unwrap();
    let target = "t".repeat(4000);
    for index in 0..5000 {
        std::os::unix::fs::symlink(&target, tmp.path().join(format!("l{index}"))).unwrap();
    }

    let mut client = create_pair(&tmp);
    let listing = client.list_dir(&rel(""), &[], 10_000).unwrap();

    let AgentDirListing::Children { entries } = listing else {
        panic!("{listing:?}");
    };
    assert_eq!(entries.len(), 5000);
    assert!(entries
        .iter()
        .all(|entry| entry.symlink_target.as_deref() == Some(target.as_str())));
    client.ping().unwrap();
}

// ---- HashFiles ----

#[test]
fn hash_files_basic() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.txt"), "hello world").unwrap();

    let mut client = create_pair(&tmp);
    let results = client.hash_files(&rels(&["test.txt"])).unwrap();
    assert_eq!(results.len(), 1);
    match &results[0] {
        protocol::FileHashResult::Ok { path, hash } => {
            assert_eq!(path, "test.txt");
            assert!(!hash.is_empty());
        }
        other => panic!("expected Ok, got {other:?}"),
    }
}

#[test]
fn hash_files_symlink() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("target.txt"), "data").unwrap();
    std::os::unix::fs::symlink("target.txt", tmp.path().join("link.txt")).unwrap();

    let mut client = create_pair(&tmp);
    let results = client.hash_files(&rels(&["link.txt"])).unwrap();
    assert_eq!(results.len(), 1);
    match &results[0] {
        protocol::FileHashResult::Symlink { path, target } => {
            assert_eq!(path, "link.txt");
            assert_eq!(target, "target.txt");
        }
        other => panic!("expected Symlink, got {other:?}"),
    }
}

#[test]
fn hash_files_nonexistent() {
    let tmp = TempDir::new().unwrap();

    let mut client = create_pair(&tmp);
    let results = client.hash_files(&rels(&["nonexistent.txt"])).unwrap();
    assert_eq!(results.len(), 1);
    assert!(matches!(
        &results[0],
        protocol::FileHashResult::Failed {
            failure: protocol::WireFailure::NotFound,
            ..
        }
    ));
}
