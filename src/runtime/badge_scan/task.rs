//! バッジスキャンのワーカースレッド処理。
//!
//! 新規 SSH 接続を開き、対象ファイルのコンテンツを読み込んで
//! `BadgeScanMsg::FileResult` でメインスレッドに送信する。
//! AppState には一切触らない（純粋な I/O + 送信）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use crate::app::BadgeScanMsg;
use crate::config::AppConfig;
use crate::diff::binary::BinaryInfo;
use crate::diff::engine::is_binary;
use crate::merge::executor::read_local_file_bytes;
use crate::ssh::client::SshClient;
use crate::ssh::passphrase_provider::PassphraseProvider;
use crate::tree::FileTree;

/// ワーカースレッドに渡すスキャンパラメータ
pub struct BadgeScanParams {
    pub dir_path: String,
    pub file_paths: Vec<String>,
    pub left_source: ScanSource,
    pub right_source: ScanSource,
    pub config: AppConfig,
    pub cancel_flag: Arc<AtomicBool>,
    pub passphrase_provider: Option<Arc<dyn PassphraseProvider>>,
}

/// スキャン対象の読み込み元
#[derive(Debug, Clone)]
pub enum ScanSource {
    /// ローカルファイルシステム
    Local(PathBuf),
    /// リモートサーバ（サーバ名）
    Remote(String),
}

/// バッジスキャンのワーカースレッド処理を実行する。
///
/// 各ファイルについてコンテンツを読み込み、`BadgeScanMsg::FileResult` で送信する。
/// キャンセルフラグが立ったら中断する。
pub fn run_badge_scan(tx: &mpsc::Sender<BadgeScanMsg>, params: &BadgeScanParams) {
    tracing::info!(
        "Badge scan started: dir={}, files={}",
        params.dir_path,
        params.file_paths.len()
    );

    let result = run_badge_scan_inner(tx, params);

    match result {
        Ok(()) => {
            let _ = tx.send(BadgeScanMsg::Done {
                dir_path: params.dir_path.clone(),
            });
        }
        Err(e) => {
            tracing::warn!("Badge scan error: {}", e);
            let _ = tx.send(BadgeScanMsg::Error {
                path: params.dir_path.clone(),
                message: e,
            });
        }
    }
}

/// 内部実装: SSH 接続を確立してファイルを読み込む
fn run_badge_scan_inner(
    tx: &mpsc::Sender<BadgeScanMsg>,
    params: &BadgeScanParams,
) -> Result<(), String> {
    // 左右の読み込みクライアントを準備
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("tokio runtime creation failed: {}", e))?;

    let mut left_client = prepare_client(&rt, &params.left_source, &params.config, params)?;
    let mut right_client = prepare_client(&rt, &params.right_source, &params.config, params)?;

    let left_root = get_root(&params.left_source, &params.config);
    let right_root = get_root(&params.right_source, &params.config);

    // キャンセルチェック
    if params.cancel_flag.load(Ordering::Relaxed) {
        tracing::debug!("Badge scan cancelled: dir={}", params.dir_path);
        return Ok(());
    }

    // バッチ読み込み: 左右それぞれ全ファイルを一括取得 (N+1 SSH 問題の解消)
    let left_contents = batch_read_side(&rt, &mut left_client, &left_root, &params.file_paths);
    let right_contents = batch_read_side(&rt, &mut right_client, &right_root, &params.file_paths);

    // 結果を分配
    for path in &params.file_paths {
        if params.cancel_flag.load(Ordering::Relaxed) {
            tracing::debug!("Badge scan cancelled: dir={}", params.dir_path);
            return Ok(());
        }

        let (left_content, left_binary) = left_contents
            .get(path.as_str())
            .cloned()
            .unwrap_or((None, None));
        let (right_content, right_binary) = right_contents
            .get(path.as_str())
            .cloned()
            .unwrap_or((None, None));

        let _ = tx.send(BadgeScanMsg::FileResult {
            path: path.clone(),
            left_content,
            right_content,
            left_binary,
            right_binary,
        });
    }

    // クリーンアップ
    disconnect_client(&rt, &mut left_client);
    disconnect_client(&rt, &mut right_client);

    Ok(())
}

/// Side に応じて SSH クライアントを準備する（Local なら None）
fn prepare_client(
    rt: &tokio::runtime::Runtime,
    source: &ScanSource,
    config: &AppConfig,
    params: &BadgeScanParams,
) -> Result<Option<SshClient>, String> {
    match source {
        ScanSource::Local(_) => Ok(None),
        ScanSource::Remote(name) => {
            let server_config = config
                .servers
                .get(name)
                .ok_or_else(|| format!("Server '{}' not found in config", name))?;
            let pp = params.passphrase_provider.as_deref();
            let client = rt
                .block_on(SshClient::connect_with_passphrase(
                    name,
                    server_config,
                    &config.ssh,
                    pp,
                ))
                .map_err(|e| format!("SSH connection failed for badge scan: {}", e))?;
            Ok(Some(client))
        }
    }
}

/// Side のルートディレクトリを取得する
fn get_root(source: &ScanSource, config: &AppConfig) -> ScanRoot {
    match source {
        ScanSource::Local(path) => ScanRoot::Local(path.clone()),
        ScanSource::Remote(name) => {
            let root = config
                .servers
                .get(name)
                .map(|c| c.root_dir.to_string_lossy().to_string())
                .unwrap_or_default();
            ScanRoot::Remote(root)
        }
    }
}

/// ルートの種別
enum ScanRoot {
    Local(PathBuf),
    Remote(String),
}

/// 全ファイルのコンテンツを一括読み込みする。
///
/// ローカルの場合は個別読み込み、リモートの場合は `read_files_batch_bytes` でバッチ取得し、
/// N+1 SSH コマンド問題を回避する。
fn batch_read_side<'a>(
    rt: &tokio::runtime::Runtime,
    client: &mut Option<SshClient>,
    root: &ScanRoot,
    rel_paths: &'a [String],
) -> HashMap<&'a str, (Option<String>, Option<BinaryInfo>)> {
    let mut results = HashMap::with_capacity(rel_paths.len());
    match root {
        ScanRoot::Local(local_root) => {
            for path in rel_paths {
                results.insert(path.as_str(), read_local_content(local_root, path));
            }
        }
        ScanRoot::Remote(remote_root) => {
            if let Some(client) = client.as_mut() {
                batch_read_remote(rt, client, remote_root, rel_paths, &mut results);
            }
        }
    }
    results
}

/// リモートファイルを一括バッチ読み込みし、結果を HashMap に格納する。
fn batch_read_remote<'a>(
    rt: &tokio::runtime::Runtime,
    client: &mut SshClient,
    remote_root: &str,
    rel_paths: &'a [String],
    results: &mut HashMap<&'a str, (Option<String>, Option<BinaryInfo>)>,
) {
    let root = remote_root.trim_end_matches('/');
    let abs_paths: Vec<String> = rel_paths
        .iter()
        .map(|p| format!("{}/{}", root, p))
        .collect();

    match rt.block_on(client.read_files_batch_bytes(&abs_paths)) {
        Ok(map) => {
            for (i, rel) in rel_paths.iter().enumerate() {
                let abs = &abs_paths[i];
                let classified = match map.get(abs.as_str()) {
                    Some(bytes) if !bytes.is_empty() => classify_content_bytes(bytes),
                    _ => (None, None),
                };
                results.insert(rel.as_str(), classified);
            }
        }
        Err(e) => {
            tracing::debug!("Badge scan batch remote read failed: {}", e);
        }
    }
}

/// バイト列からコンテンツ/バイナリを判定する純粋関数。
/// ローカル・リモート共通で使用。
fn classify_content_bytes(bytes: &[u8]) -> (Option<String>, Option<BinaryInfo>) {
    if is_binary(bytes) {
        (None, Some(BinaryInfo::from_bytes(bytes)))
    } else {
        (Some(String::from_utf8_lossy(bytes).into_owned()), None)
    }
}

/// ローカルファイルを読み込む
fn read_local_content(local_root: &Path, rel_path: &str) -> (Option<String>, Option<BinaryInfo>) {
    match read_local_file_bytes(local_root, rel_path, false) {
        Ok(bytes) => classify_content_bytes(&bytes),
        Err(_) => (None, None),
    }
}

/// SSH クライアントをクリーンアップする
fn disconnect_client(rt: &tokio::runtime::Runtime, client: &mut Option<SshClient>) {
    if let Some(c) = client.take() {
        let _ = rt.block_on(c.disconnect());
    }
}

/// ツリーの存在チェック（LeftOnly/RightOnly 判定用）
pub fn check_file_presence(tree: &FileTree, rel_path: &str) -> bool {
    tree.find_node(Path::new(rel_path)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    #[test]
    fn classify_content_bytes_binary_with_nul() {
        let mut data = vec![0u8; 100];
        data[0] = 0x00; // NUL バイト
        let (content, binary) = classify_content_bytes(&data);
        assert!(content.is_none());
        assert!(binary.is_some());
    }

    #[test]
    fn classify_content_bytes_normal_text() {
        let data = b"hello world";
        let (content, binary) = classify_content_bytes(data);
        assert_eq!(content, Some("hello world".to_string()));
        assert!(binary.is_none());
    }

    #[test]
    fn classify_content_bytes_empty() {
        let data = b"";
        let (content, binary) = classify_content_bytes(data);
        assert_eq!(content, Some("".to_string()));
        assert!(binary.is_none());
    }

    #[test]
    fn classify_content_bytes_non_utf8_shift_jis() {
        // Shift-JIS "テスト" = 0x83, 0x65, 0x83, 0x58, 0x83, 0x67
        // is_binary() は不正 UTF-8 シーケンスもバイナリと判定するため、
        // Shift-JIS バイト列はバイナリとして分類される
        let data: &[u8] = &[0x83, 0x65, 0x83, 0x58, 0x83, 0x67];
        let (content, binary) = classify_content_bytes(data);
        assert!(content.is_none());
        assert!(binary.is_some());
    }

    #[test]
    fn read_local_content_text_file() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hello world").unwrap();
        let (content, binary) = read_local_content(dir.path(), "hello.txt");
        assert_eq!(content, Some("hello world".to_string()));
        assert!(binary.is_none());
    }

    #[test]
    fn read_local_content_binary_file() {
        let dir = TempDir::new().unwrap();
        let mut data = vec![0u8; 100];
        data[0] = 0x00;
        std::fs::write(dir.path().join("bin.dat"), &data).unwrap();
        let (content, binary) = read_local_content(dir.path(), "bin.dat");
        assert!(content.is_none());
        assert!(binary.is_some());
    }

    #[test]
    fn read_local_content_missing_file() {
        let dir = TempDir::new().unwrap();
        let (content, binary) = read_local_content(dir.path(), "nonexistent.txt");
        assert!(content.is_none());
        assert!(binary.is_none());
    }

    #[test]
    fn check_file_presence_found() {
        let tree = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("main.rs")],
        )]);
        assert!(check_file_presence(&tree, "src/main.rs"));
    }

    #[test]
    fn check_file_presence_not_found() {
        let tree = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("main.rs")],
        )]);
        assert!(!check_file_presence(&tree, "src/lib.rs"));
    }

    #[test]
    fn batch_read_side_local() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("b.txt"), "world").unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let root = ScanRoot::Local(dir.path().to_path_buf());
        let paths = vec![
            "a.txt".to_string(),
            "b.txt".to_string(),
            "missing.txt".to_string(),
        ];
        let mut client = None;

        let results = batch_read_side(&rt, &mut client, &root, &paths);

        assert_eq!(results.len(), 3);
        assert_eq!(results["a.txt"].0, Some("hello".to_string()));
        assert_eq!(results["b.txt"].0, Some("world".to_string()));
        assert!(results["missing.txt"].0.is_none());
    }

    #[test]
    fn batch_read_side_remote_no_client() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let root = ScanRoot::Remote("/var/www".to_string());
        let paths = vec!["a.txt".to_string()];
        let mut client = None;

        let results = batch_read_side(&rt, &mut client, &root, &paths);
        // クライアントなしの場合は結果が空
        assert!(results.is_empty());
    }

    #[test]
    fn scan_source_local_variant() {
        let source = ScanSource::Local(PathBuf::from("/local"));
        match source {
            ScanSource::Local(p) => assert_eq!(p, PathBuf::from("/local")),
            _ => panic!("Expected Local"),
        }
    }

    #[test]
    fn scan_source_remote_variant() {
        let source = ScanSource::Remote("develop".to_string());
        match source {
            ScanSource::Remote(name) => assert_eq!(name, "develop"),
            _ => panic!("Expected Remote"),
        }
    }
}
