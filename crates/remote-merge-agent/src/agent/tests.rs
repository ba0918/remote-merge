//! Agent E2E テスト。
//!
//! Client → Server → Dispatch → file_io・走査の全プロトコルフローを
//! UnixStream ペアを使ってインプロセスで検証する。SSH 不要。

use std::fs;
use std::os::unix::net::UnixStream;

use tempfile::TempDir;

use super::client::AgentClient;
use super::protocol::StatResult;
use super::server::{run_agent_loop, MetadataConfig};
use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::relative_path::EntryPath;
use remote_merge_core::scan::outcome::{ListedEntry, ListedKind, ScanRequest, ScanScope};

/// root_dir 全体を走査した一覧の項目
fn whole_listing(
    client: &mut AgentClient<UnixStream, UnixStream>,
    exclude: &[String],
) -> Vec<ListedEntry> {
    let request = ScanRequest {
        scope: ScanScope::Whole,
        max_entries: 10000,
    };
    client
        .list_tree(&request, exclude)
        .unwrap()
        .unwrap()
        .entries
}

fn entry(path: &str) -> EntryPath {
    EntryPath::parse(path).unwrap()
}

fn rels(paths: &[&str]) -> Vec<EntryPath> {
    paths.iter().map(|path| entry(path)).collect()
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// UnixStream ペアで client ↔ server を接続する。
/// server は別スレッドで起動し、AgentClient を返す。
fn create_pair(tmp: &TempDir) -> AgentClient<UnixStream, UnixStream> {
    let (client_stream, server_stream) = UnixStream::pair().unwrap();
    let root = tmp.path().to_path_buf();
    let server_reader = server_stream.try_clone().unwrap();
    let server_writer = server_stream;
    std::thread::spawn(move || {
        run_agent_loop(
            server_reader,
            server_writer,
            root,
            MetadataConfig::default(),
        )
        .ok();
    });
    AgentClient::connect(client_stream.try_clone().unwrap(), client_stream).unwrap()
}

// ---------------------------------------------------------------------------
// 1. Full Protocol Roundtrip
// ---------------------------------------------------------------------------

#[test]
fn full_protocol_roundtrip() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("hello.txt"), "world").unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub/inner.txt"), "data").unwrap();

    let mut client = create_pair(&tmp);

    // ListTree
    let entries = whole_listing(&mut client, &[]);
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["hello.txt", "sub", "sub/inner.txt"]);

    // ReadFiles
    let results = client
        .read_files(&rels(&["hello.txt"]), 1_048_576, None)
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, Ok(b"world".to_vec()));

    // WriteFile
    client
        .write_file(&entry("written.txt"), b"new content", false)
        .unwrap();
    assert_eq!(
        fs::read_to_string(tmp.path().join("written.txt")).unwrap(),
        "new content"
    );

    // StatFiles
    let stats = client.stat_files(&rels(&["written.txt"])).unwrap();
    assert_eq!(stats.len(), 1);
    assert!(matches!(&stats[0], StatResult::Ok(stat) if stat.size == 11));

    // Ping
    client.ping().unwrap();

    // Shutdown
    client.shutdown().unwrap();
}

// ---------------------------------------------------------------------------
// 2. Tree Scan + Convert
// ---------------------------------------------------------------------------

#[test]
fn a_scan_over_the_protocol_becomes_a_file_tree() {
    use remote_merge_core::tree::NodeKind;

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
            is_dir: false
        }
    );

    // ツリーにした形の検証
    let tree = remote_merge_core::scan::tree_build::to_tree(
        tmp.path(),
        remote_merge_core::scan::outcome::Listing {
            entries,
            ..Default::default()
        },
    );
    assert!(matches!(
        tree.find_node("root.txt").unwrap().kind,
        NodeKind::File
    ));
    assert!(tree.find_node("a/b/file_b.txt").is_some());
    match &tree.find_node("link").unwrap().kind {
        NodeKind::Symlink { target } => assert_eq!(target, "root.txt"),
        other => panic!("expected Symlink, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 3. Chunked Write + Read Roundtrip
// ---------------------------------------------------------------------------

#[test]
fn chunked_write_and_read_roundtrip() {
    let tmp = TempDir::new().unwrap();

    // 4MB + 1000 バイト → 自動チャンク分割される
    let data = vec![0xCDu8; 4 * 1024 * 1024 + 1000];

    let mut client = create_pair(&tmp);
    client.write_file(&entry("large.bin"), &data, true).unwrap();

    // ファイルシステム上で検証
    let written = fs::read(tmp.path().join("large.bin")).unwrap();
    assert_eq!(written.len(), data.len());
    assert_eq!(written, data);

    // ReadFiles で読み返して検証
    let results = client
        .read_files(&rels(&["large.bin"]), 16 * 1024 * 1024, None)
        .unwrap();
    assert_eq!(results.len(), 1);
    let reassembled = results[0].1.clone().unwrap();
    assert_eq!(reassembled.len(), data.len());
    assert_eq!(reassembled, data);
}

// ---------------------------------------------------------------------------
// 4. Exclude Patterns
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// 5. Error Recovery
// ---------------------------------------------------------------------------

#[test]
fn read_nonexistent_returns_error_not_crash() {
    let tmp = TempDir::new().unwrap();
    let mut client = create_pair(&tmp);

    let results = client
        .read_files(&rels(&["nonexistent/path/xyz123"]), 1024, None)
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, Err(OpFailure::NotFound));
}

#[test]
fn server_continues_after_error() {
    let tmp = TempDir::new().unwrap();
    let mut client = create_pair(&tmp);

    // エラーを引き起こす操作
    let _ = client.read_files(&rels(&["nonexistent/abc"]), 1024, None);

    // サーバーが続行していることを Ping で確認
    client.ping().unwrap();

    // もう一度エラーを起こしても問題ない
    let _ = client.read_files(&rels(&["another/nonexistent"]), 1024, None);
    client.ping().unwrap();
}

// ---------------------------------------------------------------------------
// 6. Symlink Operations
// ---------------------------------------------------------------------------

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
            is_dir: false
        }
    );

    // ReadFiles でシンボリックリンクを読む → ターゲットの内容が返る
    let results = client
        .read_files(&rels(&[link_rel]), 1_048_576, None)
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, Ok(b"symlink content".to_vec()));
}

// ---------------------------------------------------------------------------
// 7. Backup
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 8. Multiple Files Batch
// ---------------------------------------------------------------------------

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
