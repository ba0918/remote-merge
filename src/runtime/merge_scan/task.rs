//! マージ走査スレッド内の処理。
//!
//! SSH接続・サブツリー展開・コンテンツ読み込みを実行する。
//! AppState には一切触らず、結果を MergeScanResult で返す。

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc;

use crate::app::{MergeScanMsg, MergeScanResult};
use crate::config::AppConfig;
use crate::ssh::client::SshClient;
use crate::tree::FileNode;

/// ファイル数上限（DoS 防止）
const MAX_FILES: usize = 10_000;

/// 走査スレッドのメイン処理
pub fn run_merge_scan(
    tx: &mpsc::Sender<MergeScanMsg>,
    local_root: &Path,
    exclude: &[String],
    config: &AppConfig,
    server_name: &str,
    dir_path: &str,
) -> Result<MergeScanResult, String> {
    let scan_start = std::time::Instant::now();
    tracing::info!(
        "Merge scan started: server={}, dir={}",
        server_name,
        dir_path
    );

    let server_config = config
        .servers
        .get(server_name)
        .ok_or_else(|| format!("Server '{}' not found in config", server_name))?;
    let remote_root = server_config.root_dir.to_string_lossy().to_string();

    // 新しい SSH 接続を確立
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("tokio runtime creation failed: {}", e))?;

    let mut client = rt
        .block_on(SshClient::connect(server_name, server_config, &config.ssh))
        .map_err(|e| format!("SSH connection failed: {}", e))?;

    // サブツリーを再帰的に展開
    let mut local_tree_updates = Vec::new();
    let mut remote_tree_updates = Vec::new();
    let mut file_paths = Vec::new();

    expand_subtree_recursive(
        tx,
        local_root,
        exclude,
        &remote_root,
        &rt,
        &mut client,
        dir_path,
        &mut local_tree_updates,
        &mut remote_tree_updates,
        &mut file_paths,
    )?;

    // コンテンツ読み込みフェーズ
    let mut result = MergeScanResult {
        local_cache: HashMap::new(),
        remote_cache: HashMap::new(),
        local_binary_cache: HashMap::new(),
        remote_binary_cache: HashMap::new(),
        local_tree_updates,
        remote_tree_updates,
        error_paths: HashSet::new(),
    };
    read_all_contents(
        tx,
        local_root,
        &remote_root,
        &rt,
        &mut client,
        &file_paths,
        &mut result,
    );

    let _ = rt.block_on(client.disconnect());

    let duration = scan_start.elapsed();
    let total_files = result.local_cache.len() + result.local_binary_cache.len();
    let errors = result.error_paths.len();
    tracing::info!(
        "Merge scan completed: files={}, errors={}, duration={:.2}s",
        total_files,
        errors,
        duration.as_secs_f64()
    );

    Ok(result)
}

/// 全ファイルのコンテンツを読み込み、結果を MergeScanResult に蓄積する
fn read_all_contents(
    tx: &mpsc::Sender<MergeScanMsg>,
    local_root: &Path,
    remote_root: &str,
    rt: &tokio::runtime::Runtime,
    client: &mut SshClient,
    file_paths: &[String],
    result: &mut MergeScanResult,
) {
    use crate::diff::binary::BinaryInfo;
    use crate::merge::executor;

    let total = file_paths.len();
    let _ = tx.send(MergeScanMsg::ContentPhase { total });

    for (i, path) in file_paths.iter().enumerate() {
        if i % 5 == 0 {
            let _ = tx.send(MergeScanMsg::Progress {
                files_found: i,
                current_path: Some(path.clone()),
            });
        }

        // ローカルコンテンツ
        let local_ok = match executor::read_local_file(local_root, path) {
            Ok(content) => {
                if crate::diff::engine::is_binary(content.as_bytes()) {
                    result
                        .local_binary_cache
                        .insert(path.clone(), BinaryInfo::from_bytes(content.as_bytes()));
                } else {
                    result.local_cache.insert(path.clone(), content);
                }
                true
            }
            Err(e) => {
                tracing::debug!("Local file read skipped: {} - {}", path, e);
                false
            }
        };

        // リモートコンテンツ
        let full_remote = format!("{}/{}", remote_root.trim_end_matches('/'), path);
        let remote_ok = match rt.block_on(client.read_file(&full_remote)) {
            Ok(content) => {
                if crate::diff::engine::is_binary(content.as_bytes()) {
                    result
                        .remote_binary_cache
                        .insert(path.clone(), BinaryInfo::from_bytes(content.as_bytes()));
                } else {
                    result.remote_cache.insert(path.clone(), content);
                }
                true
            }
            Err(e) => {
                tracing::debug!("Remote file read skipped: {} - {}", path, e);
                false
            }
        };

        // 両方とも読み込めなかった場合のみエラー扱い
        if !local_ok && !remote_ok {
            result.error_paths.insert(path.clone());
        }
    }
}

/// サブツリーを再帰的に展開し、ファイルパスを収集する
#[allow(clippy::too_many_arguments)]
fn expand_subtree_recursive(
    tx: &mpsc::Sender<MergeScanMsg>,
    local_root: &Path,
    exclude: &[String],
    remote_root: &str,
    rt: &tokio::runtime::Runtime,
    client: &mut SshClient,
    dir_path: &str,
    local_tree_updates: &mut Vec<(String, Vec<FileNode>)>,
    remote_tree_updates: &mut Vec<(String, Vec<FileNode>)>,
    file_paths: &mut Vec<String>,
) -> Result<(), String> {
    if file_paths.len() >= MAX_FILES {
        return Err(format!("File limit reached ({})", MAX_FILES));
    }

    let _ = tx.send(MergeScanMsg::Progress {
        files_found: file_paths.len(),
        current_path: Some(dir_path.to_string()),
    });

    // ローカルディレクトリの走査
    let local_full = local_root.join(dir_path);
    let local_children = if local_full.is_dir() {
        match crate::local::scan_dir(&local_full, exclude, dir_path) {
            Ok(children) => {
                local_tree_updates.push((dir_path.to_string(), children.clone()));
                children
            }
            Err(e) => {
                tracing::debug!("Local dir scan skipped: {} - {}", dir_path, e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // リモートディレクトリの走査
    let remote_full = format!("{}/{}", remote_root.trim_end_matches('/'), dir_path);
    let remote_children = match rt.block_on(client.list_dir(&remote_full, exclude, dir_path)) {
        Ok(children) => {
            remote_tree_updates.push((dir_path.to_string(), children.clone()));
            children
        }
        Err(e) => {
            tracing::debug!("Remote dir scan skipped: {} - {}", dir_path, e);
            Vec::new()
        }
    };

    // ファイルパスを収集し、サブディレクトリを再帰的に展開
    let mut sub_dirs = HashSet::new();
    let mut local_file_set = HashSet::new();

    for child in &local_children {
        let child_path = format!("{}/{}", dir_path, child.name);
        if child.is_dir() {
            sub_dirs.insert(child_path);
        } else if !child.is_symlink() {
            // シンボリックリンクはツリーノードから直接比較するためスキップ
            local_file_set.insert(child_path.clone());
            file_paths.push(child_path);
        }
    }

    for child in &remote_children {
        let child_path = format!("{}/{}", dir_path, child.name);
        if child.is_dir() {
            sub_dirs.insert(child_path);
        } else if !child.is_symlink() && !local_file_set.contains(&child_path) {
            file_paths.push(child_path);
        }
    }

    // サブディレクトリを再帰
    let mut sorted_dirs: Vec<String> = sub_dirs.into_iter().collect();
    sorted_dirs.sort();
    for sub_dir in sorted_dirs {
        expand_subtree_recursive(
            tx,
            local_root,
            exclude,
            remote_root,
            rt,
            client,
            &sub_dir,
            local_tree_updates,
            remote_tree_updates,
            file_paths,
        )?;
    }

    Ok(())
}
