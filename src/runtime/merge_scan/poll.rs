//! イベントループからのポーリング処理。
//!
//! mpsc チャネルからメッセージを drain し、
//! ダイアログの進捗更新と完了/エラー処理を行う。

use std::sync::mpsc;

use crate::app::{AppState, MergeScanMsg, MergeScanState};
use crate::ui::dialog::{DialogState, ProgressPhase};

use super::apply::apply_merge_scan_result;
use crate::runtime::TuiRuntime;

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
    let mut last_progress: Option<(usize, Option<String>)> = None;
    let mut content_phase_total: Option<usize> = None;
    let mut final_msg = None;

    loop {
        match rx.try_recv() {
            Ok(MergeScanMsg::Progress {
                files_found,
                current_path,
            }) => {
                last_progress = Some((files_found, current_path));
            }
            Ok(MergeScanMsg::ContentPhase { total }) => {
                content_phase_total = Some(total);
            }
            Ok(MergeScanMsg::AgentFailed { server_name }) => {
                tracing::warn!(
                    "Agent failed during merge scan for {}, invalidating",
                    server_name
                );
                runtime.core.invalidate_agent(&server_name);
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

    // ContentPhase: total が確定したらダイアログに反映
    if let Some(total) = content_phase_total {
        if let DialogState::Progress(ref mut progress) = state.dialog {
            progress.total = Some(total);
            progress.phase = ProgressPhase::LoadingFiles;
        }
    }

    // Progress 更新（ダイアログの進捗値が唯一の真実源）
    if let Some((n, path)) = last_progress {
        if let DialogState::Progress(ref mut progress) = state.dialog {
            progress.current = n;
            progress.current_path = path;
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
            MergeScanMsg::Progress { .. }
            | MergeScanMsg::ContentPhase { .. }
            | MergeScanMsg::AgentFailed { .. } => unreachable!(),
        }
        runtime.merge_scan_receiver = None;
    }
}
