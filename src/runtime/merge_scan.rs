//! ディレクトリ再帰マージ用の非ブロッキング走査。
//!
//! scanner.rs のパターン（スレッド + mpsc + poll）を踏襲し、
//! サブツリー展開 + コンテンツ読み込みを非ブロッキングで行う。

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc;

use crate::app::{AppState, MergeScanMsg, MergeScanResult, MergeScanState};
use crate::merge::executor::{self, MergeDirection};
use crate::ssh::client::SshClient;
use crate::tree::FileNode;
use crate::ui::dialog::{DialogState, ProgressDialog};

use super::TuiRuntime;

/// ファイル数上限（DoS 防止）
const MAX_FILES: usize = 10_000;
/// 走査タイムアウト（秒）
const _SCAN_TIMEOUT_SECS: u64 = 60;

/// 非ブロッキング走査を開始する
///
/// 走査対象ディレクトリ配下のサブツリーを再帰的に展開し、
/// 全ファイルのコンテンツをキャッシュに読み込む。
pub fn start_merge_scan(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    dir_path: &str,
    direction: MergeDirection,
) {
    // 走査中ならブロック
    if !matches!(state.merge_scan_state, MergeScanState::Idle) {
        state.status_message = "Merge scan already in progress".to_string();
        return;
    }

    // SSH 未接続チェック
    if !state.is_connected {
        state.status_message = "SSH not connected: cannot scan for merge".to_string();
        return;
    }

    state.merge_scan_state = MergeScanState::Scanning {
        dir_path: dir_path.to_string(),
        direction,
        files_found: 0,
    };
    state.dialog = DialogState::Progress(ProgressDialog {
        title: format!("Scanning {}", dir_path),
        current: 0,
        total: None,
        cancelable: true,
    });

    let (tx, rx) = mpsc::channel();
    runtime.merge_scan_receiver = Some(rx);

    let local_root = state.local_tree.root.clone();
    let exclude = state.active_exclude_patterns();
    let config = runtime.config.clone();
    let server_name = state.server_name.clone();
    let dir_path = dir_path.to_string();

    std::thread::spawn(move || {
        let result = run_merge_scan(&tx, &local_root, &exclude, &config, &server_name, &dir_path);
        match result {
            Ok(scan_result) => {
                let _ = tx.send(MergeScanMsg::Done(Box::new(scan_result)));
            }
            Err(e) => {
                let _ = tx.send(MergeScanMsg::Error(e));
            }
        }
    });
}

/// 走査スレッドのメイン処理
fn run_merge_scan(
    tx: &mpsc::Sender<MergeScanMsg>,
    local_root: &Path,
    exclude: &[String],
    config: &crate::config::AppConfig,
    server_name: &str,
    dir_path: &str,
) -> Result<MergeScanResult, String> {
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

    // コンテンツ読み込み
    let mut local_cache = HashMap::new();
    let mut remote_cache = HashMap::new();
    let mut error_paths = HashSet::new();
    let total = file_paths.len();

    for (i, path) in file_paths.iter().enumerate() {
        if i % 5 == 0 {
            let _ = tx.send(MergeScanMsg::Progress(i));
        }

        // ローカルコンテンツ
        let local_ok = match executor::read_local_file(local_root, path) {
            Ok(content) => {
                local_cache.insert(path.clone(), content);
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
                remote_cache.insert(path.clone(), content);
                true
            }
            Err(e) => {
                tracing::debug!("Remote file read skipped: {} - {}", path, e);
                false
            }
        };

        // 両方とも読み込めなかった場合のみエラー扱い
        if !local_ok && !remote_ok {
            error_paths.insert(path.clone());
        }

        if total > 0 && i % 5 == 0 {
            let _ = tx.send(MergeScanMsg::Progress(i + 1));
        }
    }

    let _ = rt.block_on(client.disconnect());

    Ok(MergeScanResult {
        local_cache,
        remote_cache,
        local_tree_updates,
        remote_tree_updates,
        error_paths,
    })
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

    let _ = tx.send(MergeScanMsg::Progress(file_paths.len()));

    // ローカルディレクトリの走査
    let local_full = local_root.join(dir_path);
    let local_children = if local_full.is_dir() {
        match crate::local::scan_dir(&local_full, exclude) {
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
    let remote_children = match rt.block_on(client.list_dir(&remote_full, exclude)) {
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

    for child in &local_children {
        let child_path = format!("{}/{}", dir_path, child.name);
        if child.is_dir() {
            sub_dirs.insert(child_path);
        } else {
            file_paths.push(child_path);
        }
    }

    for child in &remote_children {
        let child_path = format!("{}/{}", dir_path, child.name);
        if child.is_dir() {
            sub_dirs.insert(child_path);
        } else if !file_paths.contains(&child_path) {
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

/// 走査結果のポーリング処理（イベントループから呼ばれる）
pub fn poll_merge_scan_result(state: &mut AppState, runtime: &mut TuiRuntime) {
    let (_dir_path, direction) = match &state.merge_scan_state {
        MergeScanState::Scanning {
            dir_path,
            direction,
            ..
        } => (dir_path.clone(), *direction),
        MergeScanState::Idle => return,
    };

    let rx = match &runtime.merge_scan_receiver {
        Some(rx) => rx,
        None => return,
    };

    // 全メッセージを drain（最新の Progress だけ残す）
    let mut last_progress = None;
    let mut final_msg = None;

    loop {
        match rx.try_recv() {
            Ok(MergeScanMsg::Progress(n)) => {
                last_progress = Some(n);
            }
            Ok(msg @ MergeScanMsg::Done(_)) | Ok(msg @ MergeScanMsg::Error(_)) => {
                final_msg = Some(msg);
                break;
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                final_msg = Some(MergeScanMsg::Error(
                    "Merge scan thread terminated unexpectedly".to_string(),
                ));
                break;
            }
        }
    }

    // Progress 更新
    if let Some(n) = last_progress {
        if let MergeScanState::Scanning {
            ref mut files_found,
            ..
        } = state.merge_scan_state
        {
            *files_found = n;
        }
        // ダイアログの進捗を更新
        if let DialogState::Progress(ref mut progress) = state.dialog {
            progress.current = n;
        }
    }

    // 完了/エラー処理
    if let Some(msg) = final_msg {
        match msg {
            MergeScanMsg::Done(result) => {
                apply_merge_scan_result(state, *result);
                state.merge_scan_state = MergeScanState::Idle;
                state.dialog = DialogState::None;
                state.show_merge_dialog(direction);
            }
            MergeScanMsg::Error(e) => {
                state.merge_scan_state = MergeScanState::Idle;
                state.dialog = DialogState::None;
                state.status_message = format!("Merge scan error: {}", e);
            }
            MergeScanMsg::Progress(_) => unreachable!(),
        }
        runtime.merge_scan_receiver = None;
    }
}

/// 走査結果を AppState に反映する
fn apply_merge_scan_result(state: &mut AppState, result: MergeScanResult) {
    // ツリー更新
    for (path, children) in result.local_tree_updates {
        if let Some(node) = state.local_tree.find_node_mut(Path::new(&path)) {
            node.children = Some(children);
            node.sort_children();
        }
    }
    for (path, children) in result.remote_tree_updates {
        if let Some(node) = state.remote_tree.find_node_mut(Path::new(&path)) {
            node.children = Some(children);
            node.sort_children();
        }
    }

    // NOTE: expanded_dirs には追加しない（ツリー表示の展開状態を変えない）

    // キャッシュ反映（走査結果は新規SSH接続で取得した最新データなので上書き）
    for (path, content) in result.local_cache {
        state.local_cache.insert(path, content);
    }
    for (path, content) in result.remote_cache {
        state.remote_cache.insert(path, content);
    }

    // エラーパス
    state.error_paths.extend(result.error_paths);

    // flat_nodes を再構築
    state.rebuild_flat_nodes();
}
