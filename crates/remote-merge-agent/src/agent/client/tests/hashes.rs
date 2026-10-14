//! 子の一覧とハッシュの要求

use super::support::*;

#[test]
fn a_listing_larger_than_one_frame_is_returned_whole_and_the_agent_stays_usable() {
    // 4000 バイトのリンク先の symlink 5000 個は、リンク先だけでフレームの上限（16 MiB）を超える。
    // 子の一覧はリンク先がディレクトリかを辿って決めるので、名前の長さの上限に当たらない短い要素で
    // つないだ、ないものを指すリンク先にする
    let tmp = TempDir::new().unwrap();
    let target = "x/".repeat(2000);
    for index in 0..5000 {
        std::os::unix::fs::symlink(&target, tmp.path().join(format!("l{index}"))).unwrap();
    }

    let mut client = create_pair(&tmp);
    let request = ScanRequest {
        scope: remote_merge_core::scan::outcome::ScanScope::Children(String::new()),
        max_entries: 10_000,
    };
    let listing = client.list_tree(&request, &[]).unwrap().unwrap();

    assert_eq!(listing.entries.len(), 5000);
    assert!(listing.entries.iter().all(|entry| matches!(
        &entry.kind,
        remote_merge_core::scan::outcome::ListedKind::Symlink { target: t, .. } if *t == target.as_bytes()
    )));
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
