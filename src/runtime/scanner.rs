//! 非同期走査スレッド管理（変更ファイルフィルター用）。
//!
//! CLI の status コマンドと同じ `service::status` の純粋関数群を使い、
//! TUI でも同一の検出ロジックで差分を判定する。
//!
//! 左右の Side（Local / Remote）に応じてスキャン方法とコンテンツ取得方法を切り替える。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

use crate::app::side::Side;
use crate::app::AppState;
use crate::app::ScanState;
use crate::config::AppConfig;
use crate::service::status::{
    compute_status_from_trees, needs_content_compare, refine_status_with_content,
};
use crate::service::types::FileStatusKind;
use crate::ssh::client::SshClient;
use crate::ssh::passphrase_provider::PassphraseProvider;
use crate::tree::{FileNode, FileTree};

use super::TuiRuntime;

/// 走査結果（ツリー + 差分ステータス）
pub struct ScanOutput {
    left_nodes: Vec<FileNode>,
    right_nodes: Vec<FileNode>,
    left_trunc: bool,
    right_trunc: bool,
    /// CLI と共通の差分ステータス（path → FileStatusKind）
    statuses: HashMap<String, FileStatusKind>,
    /// Undetermined ファイルのコンテンツ比較結果（path → (left, right)）
    /// バイト列で保持し、バイナリファイルも正しく比較する。
    /// キャッシュ反映時に String に変換する。
    resolved_contents: HashMap<String, (Vec<u8>, Vec<u8>)>,
}

/// 変更ファイルフィルターの切替処理（Shift+F）
pub fn handle_diff_filter_toggle(state: &mut AppState, runtime: &mut TuiRuntime) {
    // 走査中ならブロック
    if matches!(state.scan_state, ScanState::Scanning) {
        state.status_message = "Scanning in progress. Please wait.".to_string();
        return;
    }

    // 既にフィルターモード ON -> OFF に切替
    if state.diff_filter_mode {
        state.toggle_diff_filter();
        return;
    }

    // リモート接続チェック（片方以上がリモートの場合は接続必須）
    if !state.is_connected {
        state.status_message = "Not connected: please reconnect to the remote side".to_string();
        return;
    }

    // 走査済み（キャッシュあり）-> 即時切替
    if state.scan_left_tree.is_some() && state.scan_right_tree.is_some() {
        state.toggle_diff_filter();
        return;
    }

    // 未走査: 非ブロッキング走査を開始
    state.scan_state = ScanState::Scanning;
    state.status_message = "Scanning... [Esc: cancel]".to_string();

    let (tx, rx) = mpsc::channel();
    runtime.scan_receiver = Some(rx);

    let left_source = state.left_source.clone();
    let right_source = state.right_source.clone();
    let exclude = state.active_exclude_patterns();
    let sensitive_patterns = state.sensitive_patterns.clone();
    let config = runtime.core.config.clone();
    let pp = runtime.core.passphrase_provider.clone();

    std::thread::spawn(move || {
        let result = run_scan(
            &left_source,
            &right_source,
            &exclude,
            &sensitive_patterns,
            &config,
            pp.as_deref(),
        );
        let _ = tx.send(result);
    });
}

/// スキャンのメイン処理（スレッド内で実行）。
///
/// 左右の Side に応じてローカルスキャンまたは SSH スキャンを行い、
/// `service::status` の純粋関数群で差分を計算する。
fn run_scan(
    left_source: &Side,
    right_source: &Side,
    exclude: &[String],
    sensitive_patterns: &[String],
    config: &AppConfig,
    passphrase_provider: Option<&dyn PassphraseProvider>,
) -> Result<ScanOutput, String> {
    let scan_start = std::time::Instant::now();
    tracing::info!(
        "Scan started: left={}, right={}, exclude_patterns={}",
        left_source.display_name(),
        right_source.display_name(),
        exclude.len()
    );

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("tokio runtime creation failed: {}", e))?;

    // 左側スキャン
    let (left_nodes, left_trunc, left_root, mut left_client) =
        scan_side(left_source, exclude, config, &rt, passphrase_provider)?;

    // 右側スキャン
    let (right_nodes, right_trunc, right_root, mut right_client) =
        scan_side(right_source, exclude, config, &rt, passphrase_provider)?;

    // === CLI と同じ service/status.rs のフローで差分計算 ===

    // 1. ツリー構造に変換してメタデータ比較
    let left_tree = build_temp_tree(&left_root, &left_nodes);
    let right_tree = build_temp_tree(&right_root, &right_nodes);
    let mut files = compute_status_from_trees(&left_tree, &right_tree, sensitive_patterns);

    // 2. コンテンツ比較が必要なファイルを抽出
    let paths_to_compare = needs_content_compare(&files, &left_tree, &right_tree);

    // 3. コンテンツ取得・比較
    let resolved_contents = if paths_to_compare.is_empty() {
        HashMap::new()
    } else {
        fetch_contents_both_sides(
            &paths_to_compare,
            left_source,
            &left_root,
            &mut left_client,
            right_source,
            &right_root,
            &mut right_client,
            &rt,
        )
    };

    if !resolved_contents.is_empty() {
        refine_status_with_content(&mut files, &resolved_contents);
    }

    // 4. FileStatus リストを HashMap に変換
    let statuses: HashMap<String, FileStatusKind> =
        files.iter().map(|f| (f.path.clone(), f.status)).collect();

    // SSH 切断
    if let Some(c) = left_client.take() {
        let _ = rt.block_on(c.disconnect());
    }
    if let Some(c) = right_client.take() {
        let _ = rt.block_on(c.disconnect());
    }

    let duration = scan_start.elapsed();
    let modified_count = statuses
        .values()
        .filter(|s| !matches!(s, FileStatusKind::Equal))
        .count();
    tracing::info!(
        "Scan completed: left_files={}, right_files={}, changed={}, content_verified={}, duration={:.2}s",
        left_nodes.len(),
        right_nodes.len(),
        modified_count,
        resolved_contents.len(),
        duration.as_secs_f64()
    );

    Ok(ScanOutput {
        left_nodes,
        right_nodes,
        left_trunc,
        right_trunc,
        statuses,
        resolved_contents,
    })
}

/// 片側のツリースキャンを実行する。
///
/// Local → ローカルファイルシステム走査、Remote → SSH 経由で走査。
/// SSH クライアントはコンテンツ取得で再利用するため、所有権を返す。
fn scan_side(
    side: &Side,
    exclude: &[String],
    config: &AppConfig,
    rt: &tokio::runtime::Runtime,
    passphrase_provider: Option<&dyn PassphraseProvider>,
) -> Result<(Vec<FileNode>, bool, PathBuf, Option<SshClient>), String> {
    match side {
        Side::Local => {
            let root = config.local.root_dir.clone();
            let include = &config.filter.include;
            let (nodes, trunc) = crate::local::scan_local_tree_recursive_with_include(
                &root,
                exclude,
                include,
                config.max_scan_entries,
            )
            .map_err(|e| format!("Local scan error: {}", e))?;
            Ok((nodes, trunc, root, None))
        }
        Side::Remote(server_name) => {
            let server_config = config
                .servers
                .get(server_name)
                .ok_or_else(|| format!("Server '{}' not found in config", server_name))?;

            let mut client = rt
                .block_on(SshClient::connect_with_passphrase(
                    server_name,
                    server_config,
                    &config.ssh,
                    passphrase_provider,
                ))
                .map_err(|e| format!("SSH connection failed ({}): {}", server_name, e))?;

            let root = server_config.root_dir.clone();
            let root_str = root.to_string_lossy().to_string();
            let include = &config.filter.include;
            let (nodes, trunc) = rt
                .block_on(client.list_tree_recursive(
                    &root_str,
                    exclude,
                    include,
                    config.max_scan_entries,
                    60,
                ))
                .map_err(|e| format!("Remote scan error ({}): {}", server_name, e))?;

            Ok((nodes, trunc, root, Some(client)))
        }
    }
}

/// ローカルルートと走査済みノードから一時的な FileTree を構築する。
fn build_temp_tree(root: &std::path::Path, nodes: &[FileNode]) -> FileTree {
    let mut tree = FileTree::new(root);
    tree.nodes = nodes.to_vec();
    tree.sort();
    tree
}

/// 左右のコンテンツを Side に応じた方法で取得してペアにまとめる。
///
/// バイト列で返すため、バイナリファイルも正しく比較できる。
#[allow(clippy::too_many_arguments)]
fn fetch_contents_both_sides(
    paths: &[String],
    left_source: &Side,
    left_root: &std::path::Path,
    left_client: &mut Option<SshClient>,
    right_source: &Side,
    right_root: &std::path::Path,
    right_client: &mut Option<SshClient>,
    rt: &tokio::runtime::Runtime,
) -> HashMap<String, (Vec<u8>, Vec<u8>)> {
    let left_contents =
        fetch_side_contents(left_source, paths, left_root, left_client.as_mut(), rt);
    let right_contents =
        fetch_side_contents(right_source, paths, right_root, right_client.as_mut(), rt);

    let mut result = HashMap::new();
    for path in paths {
        if let (Some(l), Some(r)) = (left_contents.get(path), right_contents.get(path)) {
            result.insert(path.clone(), (l.clone(), r.clone()));
        }
    }
    result
}

/// 片側のコンテンツをバイト列で取得する。
///
/// Local → ファイルシステムから読み込み、Remote → SSH 経由でバッチ読み込み。
/// バイト列で返すため、バイナリファイルも lossy 変換なしで扱える。
fn fetch_side_contents(
    side: &Side,
    paths: &[String],
    root: &std::path::Path,
    client: Option<&mut SshClient>,
    rt: &tokio::runtime::Runtime,
) -> HashMap<String, Vec<u8>> {
    match side {
        Side::Local => {
            let mut contents = HashMap::new();
            for path in paths {
                let full = root.join(path);
                if let Ok(content) = std::fs::read(&full) {
                    contents.insert(path.clone(), content);
                }
            }
            contents
        }
        Side::Remote(_) => {
            let client = match client {
                Some(c) => c,
                None => return HashMap::new(),
            };
            let root_str = root.to_string_lossy();
            let full_paths: Vec<String> = paths
                .iter()
                .map(|p| format!("{}/{}", root_str.trim_end_matches('/'), p))
                .collect();

            let remote_contents = rt
                .block_on(client.read_files_batch(&full_paths))
                .unwrap_or_default();

            // リモートのフルパスキーを相対パスキーに変換
            // SSH バッチ読み込みは String を返すため、バイト列に変換する
            let mut contents = HashMap::new();
            for (i, path) in paths.iter().enumerate() {
                if let Some(content) = remote_contents.get(&full_paths[i]) {
                    contents.insert(path.clone(), content.as_bytes().to_vec());
                }
            }
            contents
        }
    }
}

/// 走査結果のポーリング処理（イベントループから呼ばれる）
pub fn poll_scan_result(state: &mut AppState, runtime: &mut TuiRuntime) {
    if !matches!(state.scan_state, ScanState::Scanning) {
        return;
    }

    let rx = match &runtime.scan_receiver {
        Some(rx) => rx,
        None => return,
    };

    match rx.try_recv() {
        Ok(Ok(scan_result)) => {
            // コンテンツ比較結果をキャッシュに反映（バイト列 → String 変換）
            for (path, (left_bytes, right_bytes)) in &scan_result.resolved_contents {
                state.left_cache.insert(
                    path.clone(),
                    String::from_utf8_lossy(left_bytes).into_owned(),
                );
                state.right_cache.insert(
                    path.clone(),
                    String::from_utf8_lossy(right_bytes).into_owned(),
                );
            }

            if scan_result.left_trunc || scan_result.right_trunc {
                let msg = if scan_result.left_trunc && scan_result.right_trunc {
                    "Both left and right reached entry limit"
                } else if scan_result.left_trunc {
                    "Left side reached entry limit"
                } else {
                    "Right side reached entry limit"
                };
                state.scan_state = ScanState::PartialComplete(
                    scan_result.left_nodes.clone(),
                    scan_result.right_nodes.clone(),
                    msg.to_string(),
                );
                state.set_scan_result(
                    scan_result.left_nodes,
                    scan_result.right_nodes,
                    scan_result.statuses,
                );
                state.toggle_diff_filter();
                state.status_message = format!("[DIFF ONLY] Showing partial results ({})", msg);
            } else {
                let resolved_count = scan_result.resolved_contents.len();
                state.set_scan_result(
                    scan_result.left_nodes,
                    scan_result.right_nodes,
                    scan_result.statuses,
                );
                state.toggle_diff_filter();
                if resolved_count > 0 {
                    let diff_count = state.flat_nodes.iter().filter(|n| !n.is_dir).count();
                    state.status_message = format!(
                        "[DIFF ONLY] changes: {} files ({} verified by content)",
                        diff_count, resolved_count
                    );
                }
            }
            runtime.scan_receiver = None;
        }
        Ok(Err(e)) => {
            tracing::error!("Scan failed: {}", e);
            state.scan_state = ScanState::Error(e.clone());
            state.status_message = format!("Scan error: {}", e);
            runtime.scan_receiver = None;
        }
        Err(mpsc::TryRecvError::Empty) => {
            // まだ走査中 - 何もしない
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            tracing::error!("Scan thread terminated unexpectedly");
            state.scan_state = ScanState::Error("Scan thread terminated unexpectedly".to_string());
            state.status_message = "Scan thread terminated unexpectedly".to_string();
            runtime.scan_receiver = None;
        }
    }
}
