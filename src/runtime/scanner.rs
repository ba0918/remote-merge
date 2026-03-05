//! 非同期走査スレッド管理（変更ファイルフィルター用）。

use std::sync::mpsc;

use crate::app::AppState;
use crate::app::ScanState;
use crate::ssh::client::SshClient;

use super::TuiRuntime;

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

    // SSH未接続チェック
    if !state.is_connected {
        state.status_message = "SSH not connected: please connect to remote server".to_string();
        return;
    }

    // 走査済み（キャッシュあり）-> 即時切替
    if state.scan_local_tree.is_some() && state.scan_remote_tree.is_some() {
        state.toggle_diff_filter();
        return;
    }

    // 未走査: 非ブロッキング走査を開始
    state.scan_state = ScanState::Scanning;
    state.status_message = "Scanning... [Esc: cancel]".to_string();

    let (tx, rx) = mpsc::channel();
    runtime.scan_receiver = Some(rx);

    let local_root = state.local_tree.root.clone();
    let exclude = state.active_exclude_patterns();

    let config = runtime.config.clone();
    let server_name = state.server_name.clone();

    std::thread::spawn(move || {
        let result = (|| -> Result<
            (
                Vec<crate::tree::FileNode>,
                Vec<crate::tree::FileNode>,
                bool,
                bool,
            ),
            String,
        > {
            // ローカル走査
            let (local_nodes, local_trunc) =
                crate::local::scan_local_tree_recursive(&local_root, &exclude, 50_000)
                    .map_err(|e| format!("Local scan error: {}", e))?;

            // リモート走査（新しい SSH 接続が必要）
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| format!("tokio runtime creation failed: {}", e))?;

            let server_config = config
                .servers
                .get(&server_name)
                .ok_or_else(|| format!("Server '{}' not found in config", server_name))?;

            let mut client = rt
                .block_on(SshClient::connect(&server_name, server_config, &config.ssh))
                .map_err(|e| format!("SSH connection failed: {}", e))?;

            let remote_root = server_config.root_dir.to_string_lossy().to_string();
            let (remote_nodes, remote_trunc) = rt
                .block_on(client.list_tree_recursive(&remote_root, &exclude, 50_000, 60))
                .map_err(|e| format!("Remote scan error: {}", e))?;

            let _ = rt.block_on(client.disconnect());

            Ok((local_nodes, remote_nodes, local_trunc, remote_trunc))
        })();

        let _ = tx.send(result);
    });
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
        Ok(Ok((local_nodes, remote_nodes, local_trunc, remote_trunc))) => {
            if local_trunc || remote_trunc {
                let msg = if local_trunc && remote_trunc {
                    "Both local and remote reached entry limit"
                } else if local_trunc {
                    "Local reached entry limit"
                } else {
                    "Remote reached entry limit"
                };
                state.scan_state = ScanState::PartialComplete(
                    local_nodes.clone(),
                    remote_nodes.clone(),
                    msg.to_string(),
                );
                state.set_scan_result(local_nodes, remote_nodes);
                state.toggle_diff_filter();
                state.status_message = format!("[DIFF ONLY] Showing partial results ({})", msg);
            } else {
                state.set_scan_result(local_nodes, remote_nodes);
                state.toggle_diff_filter();
            }
            runtime.scan_receiver = None;
        }
        Ok(Err(e)) => {
            state.scan_state = ScanState::Error(e.clone());
            state.status_message = format!("Scan error: {}", e);
            runtime.scan_receiver = None;
        }
        Err(mpsc::TryRecvError::Empty) => {
            // まだ走査中 - 何もしない
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            state.scan_state = ScanState::Error("Scan thread terminated unexpectedly".to_string());
            state.status_message = "Scan thread terminated unexpectedly".to_string();
            runtime.scan_receiver = None;
        }
    }
}
