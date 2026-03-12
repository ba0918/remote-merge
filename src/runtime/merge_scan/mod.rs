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

use crate::app::{AppState, MergeScanMsg, MergeScanState, Side};
use crate::merge::executor::MergeDirection;
use crate::ui::dialog::{DialogState, ProgressDialog, ProgressPhase};

use super::TuiRuntime;

// re-export（呼び出し側の変更を最小限に）
pub use poll::poll_merge_scan_result;

/// AppState の ref_source から RefSource を構築する。
///
/// Local の場合は config の local.root_dir を使う
/// （left_tree.root はリモートサーバの場合があるため）。
fn build_ref_source(
    state: &AppState,
    config: &crate::config::AppConfig,
) -> Option<task::RefSource> {
    match state.ref_source.as_ref()? {
        Side::Local => Some(task::RefSource::Local(config.local.root_dir.clone())),
        Side::Remote(name) => Some(task::RefSource::Remote(name.clone())),
    }
}

/// 非ブロッキング走査を開始する
///
/// 走査対象ディレクトリ配下のサブツリーを再帰的に展開し、
/// 全ファイルのコンテンツをキャッシュに読み込む。
/// reference サーバが設定されている場合、ref コンテンツも同時に取得する。
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
    let config = runtime.core.config.clone();
    let server_name = state
        .right_source
        .server_name()
        .expect("merge scan requires remote right_source")
        .to_string();
    let dir_path = dir_path.to_string();
    let ref_source = build_ref_source(state, &config);

    // Agent が利用可能なら Arc::clone を渡す
    let agent = runtime.core.get_agent(&server_name);
    let ref_agent = match &ref_source {
        Some(task::RefSource::Remote(ref_name)) => runtime.core.get_agent(ref_name),
        _ => None,
    };

    std::thread::spawn(move || {
        let result = task::run_merge_scan(
            &tx,
            agent,
            ref_agent,
            &local_root,
            &exclude,
            &config,
            &server_name,
            &dir_path,
            ref_source,
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AgentConfig, AppConfig, BackupConfig, DefaultsConfig, FilterConfig, LocalConfig, SshConfig,
    };
    use crate::tree::{FileNode, FileTree};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn make_tree(root: &str) -> FileTree {
        FileTree {
            root: PathBuf::from(root),
            nodes: vec![FileNode::new_file("a.txt")],
        }
    }

    fn make_config(local_root: &str) -> AppConfig {
        AppConfig {
            servers: BTreeMap::new(),
            local: LocalConfig {
                root_dir: PathBuf::from(local_root),
            },
            filter: FilterConfig::default(),
            ssh: SshConfig::default(),
            backup: BackupConfig::default(),
            agent: AgentConfig::default(),
            defaults: DefaultsConfig::default(),
        }
    }

    fn make_state_with_ref(ref_source: Option<Side>) -> AppState {
        let mut state = AppState::new(
            make_tree("/remote-left"),
            make_tree("/remote-right"),
            Side::Remote("develop".to_string()),
            Side::Remote("staging".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.ref_source = ref_source;
        state
    }

    #[test]
    fn build_ref_source_none_when_no_ref() {
        let state = make_state_with_ref(None);
        let config = make_config("/local");
        assert!(build_ref_source(&state, &config).is_none());
    }

    #[test]
    fn build_ref_source_local_uses_config_root() {
        let state = make_state_with_ref(Some(Side::Local));
        let config = make_config("/config-local-root");
        let src = build_ref_source(&state, &config).unwrap();
        match src {
            task::RefSource::Local(path) => {
                // config の local.root_dir を使い、left_tree.root ("/remote-left") は使わない
                assert_eq!(path, PathBuf::from("/config-local-root"));
            }
            task::RefSource::Remote(_) => panic!("Expected Local"),
        }
    }

    #[test]
    fn build_ref_source_remote() {
        let state = make_state_with_ref(Some(Side::Remote("release".to_string())));
        let config = make_config("/local");
        let src = build_ref_source(&state, &config).unwrap();
        match src {
            task::RefSource::Remote(name) => {
                assert_eq!(name, "release");
            }
            task::RefSource::Local(_) => panic!("Expected Remote"),
        }
    }
}
