//! 操作の要求

use super::support::*;

// ---- ListTree ----

fn whole() -> ScanRequest {
    ScanRequest {
        scope: remote_merge_core::scan::outcome::ScanScope::Whole,
        max_entries: 10000,
    }
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
