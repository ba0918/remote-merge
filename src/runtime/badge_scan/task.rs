//! バッジスキャンのワーカースレッド処理。
//!
//! 新規 SSH 接続を開き、対象ファイルのコンテンツを読み込んで
//! `BadgeScanMsg::FileResult` でメインスレッドに送信する。
//! AppState には一切触らない（純粋な I/O + 送信）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use crate::app::BadgeScanMsg;
use crate::config::AppConfig;
use crate::diff::binary::BinaryInfo;
use crate::diff::engine::is_binary;
use crate::merge::executor::read_local_file;
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

    // 各ファイルを処理
    for path in &params.file_paths {
        // キャンセルチェック
        if params.cancel_flag.load(Ordering::Relaxed) {
            tracing::debug!("Badge scan cancelled: dir={}", params.dir_path);
            return Ok(());
        }

        let (left_content, left_binary) =
            read_side_content(&rt, &mut left_client, &left_root, path);
        let (right_content, right_binary) =
            read_side_content(&rt, &mut right_client, &right_root, path);

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

/// 1ファイルのコンテンツを読み込む
fn read_side_content(
    rt: &tokio::runtime::Runtime,
    client: &mut Option<SshClient>,
    root: &ScanRoot,
    rel_path: &str,
) -> (Option<String>, Option<BinaryInfo>) {
    match root {
        ScanRoot::Local(local_root) => read_local_content(local_root, rel_path),
        ScanRoot::Remote(remote_root) => {
            if let Some(client) = client.as_mut() {
                read_remote_content(rt, client, remote_root, rel_path)
            } else {
                (None, None)
            }
        }
    }
}

/// ローカルファイルを読み込む
fn read_local_content(local_root: &Path, rel_path: &str) -> (Option<String>, Option<BinaryInfo>) {
    match read_local_file(local_root, rel_path) {
        Ok(content) => {
            if is_binary(content.as_bytes()) {
                (None, Some(BinaryInfo::from_bytes(content.as_bytes())))
            } else {
                (Some(content), None)
            }
        }
        Err(_) => (None, None),
    }
}

/// リモートファイルを SSH 経由で読み込む
fn read_remote_content(
    rt: &tokio::runtime::Runtime,
    client: &mut SshClient,
    remote_root: &str,
    rel_path: &str,
) -> (Option<String>, Option<BinaryInfo>) {
    let abs_path = format!("{}/{}", remote_root.trim_end_matches('/'), rel_path);
    match rt.block_on(client.read_files_batch(std::slice::from_ref(&abs_path))) {
        Ok(map) => match map.get(&abs_path) {
            Some(content) if !content.is_empty() => {
                if is_binary(content.as_bytes()) {
                    (None, Some(BinaryInfo::from_bytes(content.as_bytes())))
                } else {
                    (Some(content.clone()), None)
                }
            }
            _ => (None, None),
        },
        Err(e) => {
            tracing::debug!("Badge scan remote read failed: {} - {}", rel_path, e);
            (None, None)
        }
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
