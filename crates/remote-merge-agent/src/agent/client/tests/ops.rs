//! 操作の要求

use std::fs;

use super::support::*;
use remote_merge_core::scan::outcome::{ListedEntry, ListedKind};

// ---- ListTree ----

fn whole() -> ScanRequest {
    ScanRequest {
        scope: remote_merge_core::scan::outcome::ScanScope::Whole,
        max_entries: 10000,
    }
}

/// root_dir 全体を走査した一覧の項目
fn whole_listing(
    client: &mut AgentClient<UnixStream, UnixStream>,
    exclude: &[String],
) -> Vec<ListedEntry> {
    client
        .list_tree(&whole(), exclude)
        .expect("the agent answers the scan")
        .expect("the scan of root_dir succeeds")
        .entries
}

#[test]
fn list_tree_empty_dir() {
    let tmp = TempDir::new().unwrap();
    let mut client = create_pair(&tmp);
    let listing = client.list_tree(&whole(), &[]).unwrap().unwrap();
    assert!(listing.entries.is_empty());
    assert!(!listing.truncated);
}

#[test]
fn list_tree_with_files() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("hello.txt"), "world").unwrap();
    std::fs::create_dir(tmp.path().join("sub")).unwrap();
    std::fs::write(tmp.path().join("sub/inner.txt"), "data").unwrap();

    let mut client = create_pair(&tmp);
    let listing = client.list_tree(&whole(), &[]).unwrap().unwrap();

    let paths: Vec<&str> = listing.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["hello.txt", "sub", "sub/inner.txt"]);
    assert!(!listing.truncated);
}

#[test]
fn list_tree_reports_why_the_scan_stopped() {
    let tmp = TempDir::new().unwrap();
    let mut client = create_pair(&tmp);
    std::fs::remove_dir(tmp.path()).unwrap();
    let failure = client.list_tree(&whole(), &[]).unwrap().unwrap_err();
    assert_eq!(failure, ScanFailure::RootMissing);
}

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

#[test]
fn a_scan_over_the_protocol_lists_every_entry_with_its_kind() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("root.txt"), "r").unwrap();
    fs::create_dir_all(tmp.path().join("a/b")).unwrap();
    fs::write(tmp.path().join("a/file_a.txt"), "a").unwrap();
    fs::write(tmp.path().join("a/b/file_b.txt"), "b").unwrap();

    // シンボリックリンク
    std::os::unix::fs::symlink("root.txt", tmp.path().join("link")).unwrap();

    let mut client = create_pair(&tmp);
    let entries = whole_listing(&mut client, &[]);

    // エントリ種類の検証
    let file_entry = entries.iter().find(|e| e.path == "root.txt").unwrap();
    assert_eq!(file_entry.kind, ListedKind::File);
    let dir_entry = entries.iter().find(|e| e.path == "a").unwrap();
    assert_eq!(dir_entry.kind, ListedKind::Directory);
    let link_entry = entries.iter().find(|e| e.path == "link").unwrap();
    assert_eq!(
        link_entry.kind,
        ListedKind::Symlink {
            target: "root.txt".into(),
            points_to: remote_merge_core::scan::outcome::LinkPoint::NotDirectory
        }
    );

    assert!(entries.iter().any(|e| e.path == "a/b/file_b.txt"));
}

#[test]
fn exclude_patterns_filter_entries() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("keep.txt"), "keep").unwrap();
    fs::create_dir(tmp.path().join("node_modules")).unwrap();
    fs::write(tmp.path().join("node_modules/pkg.js"), "js").unwrap();
    fs::create_dir(tmp.path().join(".git")).unwrap();
    fs::write(tmp.path().join(".git/HEAD"), "ref").unwrap();

    let mut client = create_pair(&tmp);
    let entries = whole_listing(
        &mut client,
        &["node_modules".to_string(), ".git".to_string()],
    );

    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"keep.txt"));
    assert!(
        !paths.iter().any(|p| p.contains("node_modules")),
        "node_modules should be excluded, got: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.contains(".git")),
        ".git should be excluded, got: {paths:?}"
    );
}

// ---- ReadFiles ----

#[test]
fn read_files_single() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.txt"), "hello agent").unwrap();

    let mut client = create_pair(&tmp);
    let results = client
        .read_files(&rels(&["test.txt"]), 1_048_576, None)
        .unwrap();
    assert_eq!(
        results,
        vec![("test.txt".to_string(), Ok(b"hello agent".to_vec()))]
    );
}

#[test]
fn read_50_files_in_single_request() {
    let tmp = TempDir::new().unwrap();
    let count = 50;

    // 50 ファイルを作成（相対パスで管理）
    let mut paths = Vec::new();
    for i in 0..count {
        let name = format!("file_{i:03}.txt");
        let content = format!("content of file {i}");
        fs::write(tmp.path().join(&name), &content).unwrap();
        paths.push(name);
    }

    let mut client = create_pair(&tmp);

    // 全50ファイルを1リクエストで読み込む
    let results = client
        .read_files(
            &paths.iter().map(|p| entry(p)).collect::<Vec<_>>(),
            1_048_576,
            None,
        )
        .unwrap();
    assert_eq!(results.len(), count);

    for (i, (_, result)) in results.iter().enumerate() {
        let expected_content = format!("content of file {i}");
        assert_eq!(
            result.as_deref().map(String::from_utf8_lossy),
            Ok(expected_content.into()),
            "file {i} content mismatch"
        );
    }
}

// ---- WriteFile ----

#[test]
fn write_file_small() {
    let tmp = TempDir::new().unwrap();

    let mut client = create_pair(&tmp);
    client
        .write_file(&entry("out.txt"), b"written data", false)
        .unwrap();

    let content = std::fs::read_to_string(tmp.path().join("out.txt")).unwrap();
    assert_eq!(content, "written data");
}

#[test]
fn write_file_empty() {
    let tmp = TempDir::new().unwrap();

    let mut client = create_pair(&tmp);
    client.write_file(&entry("empty.txt"), b"", false).unwrap();

    let content = std::fs::read(tmp.path().join("empty.txt")).unwrap();
    assert!(content.is_empty());
}

#[test]
fn write_file_auto_chunking() {
    let tmp = TempDir::new().unwrap();
    // WRITE_CHUNK_SIZE を超えるデータ → 複数チャンクに分割される
    let data = vec![0xABu8; WRITE_CHUNK_SIZE + 1000];

    let mut client = create_pair(&tmp);
    client.write_file(&entry("large.bin"), &data, true).unwrap();

    let written = std::fs::read(tmp.path().join("large.bin")).unwrap();
    assert_eq!(written, data);
}

// ---- Symlink ----

#[test]
fn symlink_create_and_list_and_read() {
    let tmp = TempDir::new().unwrap();
    let target_path = tmp.path().join("target.txt");
    fs::write(&target_path, "symlink content").unwrap();

    let mut client = create_pair(&tmp);

    // シンボリックリンク作成（相対パスで指定）
    let link_rel = "mylink.txt";
    client.symlink(&entry(link_rel), "target.txt").unwrap();

    // ListTree でシンボリックリンクが表示される
    let entries = whole_listing(&mut client, &[]);
    let link_entry = entries
        .iter()
        .find(|e| e.path == link_rel)
        .expect("symlink should appear in tree");
    assert_eq!(
        link_entry.kind,
        ListedKind::Symlink {
            target: "target.txt".into(),
            points_to: remote_merge_core::scan::outcome::LinkPoint::NotDirectory
        }
    );

    // ReadFiles でシンボリックリンクを読む → ターゲットの内容が返る
    let results = client
        .read_files(&rels(&[link_rel]), 1_048_576, None)
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, Ok(b"symlink content".to_vec()));
}

// ---- Error response ----

#[test]
fn a_read_that_fails_answers_the_kind_of_the_failure() {
    let tmp = TempDir::new().unwrap();
    let mut client = create_pair(&tmp);
    let results = client
        .read_files(&rels(&["nonexistent/path/abc123"]), 1024, None)
        .unwrap();
    assert_eq!(
        results,
        vec![(
            "nonexistent/path/abc123".to_string(),
            Err(OpFailure::NotFound)
        )]
    );
}
