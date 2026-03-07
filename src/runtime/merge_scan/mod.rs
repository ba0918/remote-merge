//! ディレクトリ再帰マージ用の非ブロッキング走査。
//!
//! scanner.rs のパターン（スレッド + mpsc + poll）を踏襲し、
//! サブツリー展開 + コンテンツ読み込みを非ブロッキングで行う。
//!
//! 責務分離:
//! - task: スレッド内処理（SSH接続・ツリー展開・コンテンツ読み込み）
//! - poll: イベントループからのポーリング
//! - apply: 走査結果の AppState 反映

pub mod apply;
pub mod poll;
pub mod task;

use std::sync::mpsc;

use crate::app::{AppState, MergeScanMsg, MergeScanState};
use crate::merge::executor::MergeDirection;
use crate::ui::dialog::{DialogState, ProgressDialog, ProgressPhase};

use super::TuiRuntime;

// re-export（呼び出し側の変更を最小限に）
pub use poll::poll_merge_scan_result;

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
    };
    state.dialog =
        DialogState::Progress(ProgressDialog::new(ProgressPhase::Scanning, dir_path, true));

    let (tx, rx) = mpsc::channel();
    runtime.merge_scan_receiver = Some(rx);

    let local_root = state.left_tree.root.clone();
    let exclude = state.active_exclude_patterns();
    let config = runtime.config.clone();
    let server_name = state.server_name.clone();
    let dir_path = dir_path.to_string();

    std::thread::spawn(move || {
        let result =
            task::run_merge_scan(&tx, &local_root, &exclude, &config, &server_name, &dir_path);
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
