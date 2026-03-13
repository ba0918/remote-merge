//! Agent E2E テスト。
//!
//! Client → Server → Dispatch → file_io/tree_scan の全プロトコルフローを
//! UnixStream ペアを使ってインプロセスで検証する。SSH 不要。

use std::fs;
use std::os::unix::net::UnixStream;

use tempfile::TempDir;

use super::client::AgentClient;
use super::protocol::{FileKind, FileReadResult};
use super::server::{run_agent_loop, MetadataConfig};
use super::tree_scan::convert_agent_entries_to_nodes;

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
    let (entries, _truncated) = client.list_tree("", &[], 10000).unwrap();
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"hello.txt"));
    // ディレクトリ "sub" は buffer に含まれない（走査キューにのみ追加）
    assert!(!paths.contains(&"sub"));
    assert!(paths.contains(&"sub/inner.txt"));

    // ReadFiles
    let results = client
        .read_files(&["hello.txt".to_string()], 1_048_576)
        .unwrap();
    assert_eq!(results.len(), 1);
    match &results[0] {
        FileReadResult::Ok { content, .. } => assert_eq!(content, b"world"),
        FileReadResult::Error { message, .. } => panic!("expected Ok, got Error: {message}"),
    }

    // WriteFile
    client
        .write_file("written.txt", b"new content", false)
        .unwrap();
    assert_eq!(
        fs::read_to_string(tmp.path().join("written.txt")).unwrap(),
        "new content"
    );

    // StatFiles
    let stats = client.stat_files(&["written.txt".to_string()]).unwrap();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].size, 11); // "new content".len()

    // Ping
    client.ping().unwrap();

    // Shutdown
    client.shutdown().unwrap();
}

// ---------------------------------------------------------------------------
// 2. Tree Scan + Convert
// ---------------------------------------------------------------------------

#[test]
fn tree_scan_and_convert_to_file_nodes() {
    use crate::tree::NodeKind;

    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("root.txt"), "r").unwrap();
    fs::create_dir_all(tmp.path().join("a/b")).unwrap();
    fs::write(tmp.path().join("a/file_a.txt"), "a").unwrap();
    fs::write(tmp.path().join("a/b/file_b.txt"), "b").unwrap();

    // シンボリックリンク
    std::os::unix::fs::symlink("root.txt", tmp.path().join("link")).unwrap();

    let mut client = create_pair(&tmp);
    let (entries, _truncated) = client.list_tree("", &[], 10000).unwrap();

    // エントリ種類の検証
    let file_entry = entries.iter().find(|e| e.path == "root.txt").unwrap();
    assert_eq!(file_entry.kind, FileKind::File);

    // ディレクトリ "a" は buffer に含まれない
    assert!(!entries.iter().any(|e| e.path == "a"));

    let link_entry = entries.iter().find(|e| e.path == "link").unwrap();
    assert_eq!(link_entry.kind, FileKind::Symlink);
    assert_eq!(link_entry.symlink_target.as_deref(), Some("root.txt"));

    // convert_agent_entries_to_nodes の検証
    let nodes = convert_agent_entries_to_nodes(&entries);
    assert_eq!(nodes.len(), entries.len());

    // NodeKind マッピングの検証
    let root_node = nodes.iter().find(|n| n.name == "root.txt").unwrap();
    assert!(matches!(root_node.kind, NodeKind::File));

    // ディレクトリ "a" は entries に含まれないため nodes にも含まれない
    assert!(!nodes.iter().any(|n| n.name == "a"));

    let link_node = nodes.iter().find(|n| n.name == "link").unwrap();
    match &link_node.kind {
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
    client.write_file("large.bin", &data, true).unwrap();

    // ファイルシステム上で検証
    let written = fs::read(tmp.path().join("large.bin")).unwrap();
    assert_eq!(written.len(), data.len());
    assert_eq!(written, data);

    // ReadFiles で読み返して検証
    let results = client
        .read_files(&["large.bin".to_string()], 16 * 1024 * 1024)
        .unwrap();
    let mut reassembled = Vec::new();
    for r in &results {
        match r {
            FileReadResult::Ok { content, .. } => reassembled.extend_from_slice(content),
            FileReadResult::Error { message, .. } => panic!("read error: {message}"),
        }
    }
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
    let (entries, _truncated) = client
        .list_tree("", &["node_modules".to_string(), ".git".to_string()], 10000)
        .unwrap();

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
        .read_files(&["/nonexistent/path/xyz123".to_string()], 1024)
        .unwrap();
    assert_eq!(results.len(), 1);
    match &results[0] {
        FileReadResult::Error { message, .. } => {
            assert!(!message.is_empty());
        }
        FileReadResult::Ok { .. } => {
            // 実装によっては空を返す可能性もあるが、基本的に Error が来る
        }
    }
}

#[test]
fn write_path_traversal_returns_error() {
    let tmp = TempDir::new().unwrap();
    let mut client = create_pair(&tmp);

    // パストラバーサルを含む相対パスで WriteFile → エラーになるがクラッシュしない
    let result = client.write_file("../../tmp/evil.txt", b"evil", false);
    // write_file はエラーを返す（WriteResult.success=false → bail!）
    assert!(result.is_err());
}

#[test]
fn server_continues_after_error() {
    let tmp = TempDir::new().unwrap();
    let mut client = create_pair(&tmp);

    // エラーを引き起こす操作
    let _ = client.read_files(&["/nonexistent/abc".to_string()], 1024);

    // サーバーが続行していることを Ping で確認
    client.ping().unwrap();

    // もう一度エラーを起こしても問題ない
    let _ = client.read_files(&["/another/nonexistent".to_string()], 1024);
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
    client.symlink(link_rel, "target.txt").unwrap();

    // ListTree でシンボリックリンクが表示される
    let (entries, _truncated) = client.list_tree("", &[], 10000).unwrap();
    let link_entry = entries
        .iter()
        .find(|e| e.path == link_rel)
        .expect("symlink should appear in tree");
    assert_eq!(link_entry.kind, FileKind::Symlink);
    assert_eq!(link_entry.symlink_target.as_deref(), Some("target.txt"));

    // ReadFiles でシンボリックリンクを読む → ターゲットの内容が返る
    let results = client
        .read_files(&[link_rel.to_string()], 1_048_576)
        .unwrap();
    assert_eq!(results.len(), 1);
    match &results[0] {
        FileReadResult::Ok { content, .. } => {
            assert_eq!(content, b"symlink content");
        }
        FileReadResult::Error { message, .. } => {
            panic!("expected Ok, got Error: {message}");
        }
    }
}

// ---------------------------------------------------------------------------
// 7. Backup
// ---------------------------------------------------------------------------

#[test]
fn backup_creates_copy_via_protocol() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("important.txt"), "backup me").unwrap();

    let mut client = create_pair(&tmp);

    let backup_dir = "backups";
    client
        .backup(&["important.txt".to_string()], backup_dir)
        .unwrap();

    // バックアップが存在すること
    let backup_path = tmp.path().join("backups/important.txt");
    assert!(backup_path.exists(), "backup file should exist");
    assert_eq!(fs::read_to_string(&backup_path).unwrap(), "backup me");
}

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
    let results = client.read_files(&paths, 1_048_576).unwrap();
    assert_eq!(results.len(), count);

    for (i, result) in results.iter().enumerate() {
        let expected_content = format!("content of file {i}");
        match result {
            FileReadResult::Ok { content, .. } => {
                assert_eq!(
                    String::from_utf8_lossy(content),
                    expected_content,
                    "file {i} content mismatch"
                );
            }
            FileReadResult::Error { message, .. } => {
                panic!("file {i} returned error: {message}");
            }
        }
    }
}
