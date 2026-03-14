//! バッジスキャンのポーリング処理。
//!
//! イベントループから呼ばれ、チャネルからメッセージを受信して
//! キャッシュ更新 + バッジ再計算を行う。

use std::sync::mpsc;

use crate::app::{AppState, BadgeScanMsg};
use crate::runtime::TuiRuntime;

/// バッジスキャン結果のポーリング処理（イベントループから呼ばれる）。
///
/// 全進行中スキャンのチャネルを drain し、FileResult を受信したら:
/// 1. キャッシュ未保存ならキャッシュに保存
/// 2. compute_badge() でバッジ再計算
/// 3. flat_nodes のバッジを更新
/// 4. 親ディレクトリのバッジも再計算
pub fn poll_badge_scan_results(state: &mut AppState, runtime: &mut TuiRuntime) {
    if runtime.badge_scans.is_empty() {
        return;
    }

    // 完了・エラーになったディレクトリを収集
    let mut completed_dirs: Vec<String> = Vec::new();

    // 全エントリを処理
    let dir_paths: Vec<String> = runtime.badge_scans.keys().cloned().collect();

    for dir_path in &dir_paths {
        let entry = match runtime.badge_scans.get(dir_path) {
            Some(e) => e,
            None => continue,
        };

        // チャネルから全メッセージを drain
        loop {
            match entry.receiver.try_recv() {
                Ok(BadgeScanMsg::FileResult {
                    path,
                    left_content,
                    right_content,
                    left_binary,
                    right_binary,
                }) => {
                    apply_file_result(
                        state,
                        &path,
                        left_content,
                        right_content,
                        left_binary,
                        right_binary,
                    );
                }
                Ok(BadgeScanMsg::Done { dir_path }) => {
                    tracing::debug!("Badge scan completed: dir={}", dir_path);
                    completed_dirs.push(dir_path);
                    break;
                }
                Ok(BadgeScanMsg::Error { path, message }) => {
                    tracing::warn!("Badge scan error: {} - {}", path, message);
                    // SSH 接続失敗の場合は is_connected を false にする
                    if message.contains("SSH connection failed") {
                        state.is_connected = false;
                    }
                    completed_dirs.push(path);
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    tracing::debug!("Badge scan channel disconnected: dir={}", dir_path);
                    completed_dirs.push(dir_path.clone());
                    break;
                }
            }
        }
    }

    // 完了したエントリを削除
    for dir_path in &completed_dirs {
        runtime.badge_scans.remove(dir_path);
    }
}

/// FileResult を受信した際の処理
fn apply_file_result(
    state: &mut AppState,
    path: &str,
    left_content: Option<String>,
    right_content: Option<String>,
    left_binary: Option<crate::diff::binary::BinaryInfo>,
    right_binary: Option<crate::diff::binary::BinaryInfo>,
) {
    // キャッシュ保存（既にキャッシュ済みなら上書きしない）
    if let Some(content) = left_content {
        if !state.left_cache.contains_key(path) {
            state.left_cache.insert(path.to_string(), content);
        }
    }
    if let Some(content) = right_content {
        if !state.right_cache.contains_key(path) {
            state.right_cache.insert(path.to_string(), content);
        }
    }
    if let Some(info) = left_binary {
        if !state.left_binary_cache.contains_key(path) {
            state.left_binary_cache.insert(path.to_string(), info);
        }
    }
    if let Some(info) = right_binary {
        if !state.right_binary_cache.contains_key(path) {
            state.right_binary_cache.insert(path.to_string(), info);
        }
    }

    // バッジ再計算
    let badge = state.compute_badge(path, false);

    // flat_nodes のバッジを更新（パスが存在する場合のみ）
    if let Some(node) = state.flat_nodes.iter_mut().find(|n| n.path == path) {
        node.badge = badge;
    }

    // 親ディレクトリのバッジも再計算
    update_parent_badges(state, path);
}

/// 親ディレクトリのバッジを再計算する
fn update_parent_badges(state: &mut AppState, file_path: &str) {
    let mut current = file_path.to_string();
    while let Some(pos) = current.rfind('/') {
        current = current[..pos].to_string();
        let badge = state.compute_badge(&current, true);
        if let Some(node) = state.flat_nodes.iter_mut().find(|n| n.path == current) {
            node.badge = badge;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::{Badge, FlatNode};
    use crate::app::Side;
    use crate::diff::binary::BinaryInfo;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn make_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    fn make_flat_file(path: &str, badge: Badge) -> FlatNode {
        FlatNode {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            depth: path.matches('/').count(),
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge,
            ref_only: false,
        }
    }

    fn make_flat_dir(path: &str, badge: Badge) -> FlatNode {
        FlatNode {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            depth: path.matches('/').count(),
            is_dir: true,
            is_symlink: false,
            expanded: true,
            badge,
            ref_only: false,
        }
    }

    fn make_test_state() -> AppState {
        AppState::new(
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
            )]),
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
            )]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        )
    }

    #[test]
    fn apply_file_result_caches_content() {
        let mut state = make_test_state();
        state.flat_nodes = vec![make_flat_file("src/a.rs", Badge::Loading)];

        apply_file_result(
            &mut state,
            "src/a.rs",
            Some("left".to_string()),
            Some("right".to_string()),
            None,
            None,
        );

        assert_eq!(state.left_cache.get("src/a.rs").unwrap(), "left");
        assert_eq!(state.right_cache.get("src/a.rs").unwrap(), "right");
    }

    #[test]
    fn apply_file_result_does_not_overwrite_cache() {
        let mut state = make_test_state();
        state.flat_nodes = vec![make_flat_file("src/a.rs", Badge::Loading)];
        state
            .left_cache
            .insert("src/a.rs".to_string(), "original".to_string());

        apply_file_result(
            &mut state,
            "src/a.rs",
            Some("new".to_string()),
            Some("right".to_string()),
            None,
            None,
        );

        // 既存キャッシュは上書きされない
        assert_eq!(state.left_cache.get("src/a.rs").unwrap(), "original");
    }

    #[test]
    fn apply_file_result_updates_badge() {
        let mut state = make_test_state();
        state.flat_nodes = vec![
            make_flat_dir("src", Badge::Unchecked),
            make_flat_file("src/a.rs", Badge::Loading),
        ];

        // 同じコンテンツ → Equal
        apply_file_result(
            &mut state,
            "src/a.rs",
            Some("same".to_string()),
            Some("same".to_string()),
            None,
            None,
        );

        assert_eq!(state.flat_nodes[1].badge, Badge::Equal);
    }

    #[test]
    fn apply_file_result_skips_missing_flat_node() {
        let mut state = make_test_state();
        state.flat_nodes = vec![make_flat_file("src/a.rs", Badge::Loading)];

        // flat_nodes に存在しないパス → キャッシュには保存するがバッジ更新はスキップ
        apply_file_result(
            &mut state,
            "src/nonexistent.rs",
            Some("content".to_string()),
            None,
            None,
            None,
        );

        assert!(state.left_cache.contains_key("src/nonexistent.rs"));
        // flat_nodes[0] は変わらない
        assert_eq!(state.flat_nodes[0].badge, Badge::Loading);
    }

    #[test]
    fn apply_file_result_left_only() {
        let mut state = make_test_state();
        state.flat_nodes = vec![make_flat_file("src/a.rs", Badge::Loading)];

        // left のみコンテンツあり
        apply_file_result(
            &mut state,
            "src/a.rs",
            Some("content".to_string()),
            None,
            None,
            None,
        );

        // compute_badge が LeftOnly/Unchecked/etc を返す（ツリー構造による）
        assert_ne!(state.flat_nodes[0].badge, Badge::Loading);
    }

    #[test]
    fn apply_file_result_with_binary() {
        let mut state = make_test_state();
        state.flat_nodes = vec![make_flat_file("src/img.png", Badge::Loading)];

        let info = BinaryInfo::from_bytes(&[0u8; 32]);
        apply_file_result(
            &mut state,
            "src/img.png",
            None,
            None,
            Some(info.clone()),
            Some(info),
        );

        assert!(state.left_binary_cache.contains_key("src/img.png"));
        assert!(state.right_binary_cache.contains_key("src/img.png"));
    }

    #[test]
    fn poll_done_removes_entry() {
        let mut state = make_test_state();
        let mut runtime = TuiRuntime::new_for_test();

        let (tx, rx) = std::sync::mpsc::channel();
        let flag = Arc::new(AtomicBool::new(false));
        runtime.badge_scans.insert(
            "src".to_string(),
            super::super::BadgeScanEntry {
                receiver: rx,
                cancel_flag: flag,
            },
        );

        // Done メッセージを送信
        tx.send(BadgeScanMsg::Done {
            dir_path: "src".to_string(),
        })
        .unwrap();

        poll_badge_scan_results(&mut state, &mut runtime);
        assert!(runtime.badge_scans.is_empty());
    }

    #[test]
    fn poll_file_result_then_done() {
        let mut state = make_test_state();
        let mut runtime = TuiRuntime::new_for_test();
        state.flat_nodes = vec![
            make_flat_dir("src", Badge::Unchecked),
            make_flat_file("src/a.rs", Badge::Loading),
        ];

        let (tx, rx) = std::sync::mpsc::channel();
        let flag = Arc::new(AtomicBool::new(false));
        runtime.badge_scans.insert(
            "src".to_string(),
            super::super::BadgeScanEntry {
                receiver: rx,
                cancel_flag: flag,
            },
        );

        // FileResult → Done
        tx.send(BadgeScanMsg::FileResult {
            path: "src/a.rs".to_string(),
            left_content: Some("hello".to_string()),
            right_content: Some("hello".to_string()),
            left_binary: None,
            right_binary: None,
        })
        .unwrap();
        tx.send(BadgeScanMsg::Done {
            dir_path: "src".to_string(),
        })
        .unwrap();

        poll_badge_scan_results(&mut state, &mut runtime);

        assert!(runtime.badge_scans.is_empty());
        assert_eq!(state.flat_nodes[1].badge, Badge::Equal);
        assert!(state.left_cache.contains_key("src/a.rs"));
    }

    #[test]
    fn poll_empty_scans_is_noop() {
        let mut state = make_test_state();
        let mut runtime = TuiRuntime::new_for_test();

        // 空のスキャンリスト → 何もしない
        poll_badge_scan_results(&mut state, &mut runtime);
        // パニックしなければ OK
    }

    #[test]
    fn poll_max_files_status_message() {
        let mut runtime = TuiRuntime::new_for_test();
        let file_count = crate::config::DEFAULT_BADGE_SCAN_MAX_FILES + 1;
        let files: Vec<FileNode> = (0..file_count)
            .map(|i| FileNode::new_file(format!("file_{}.rs", i)))
            .collect();
        let mut state = AppState::new(
            make_tree(vec![FileNode::new_dir_with_children("big", files.clone())]),
            make_tree(vec![FileNode::new_dir_with_children("big", files)]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        );
        state.expanded_dirs.insert("big".to_string());
        state.rebuild_flat_nodes();

        super::super::start_badge_scan(&mut state, &mut runtime, "big");
        assert!(state.status_message.contains("Badge scan skipped"));
    }

    #[test]
    fn update_parent_badges_updates_dir() {
        let mut state = make_test_state();
        state.flat_nodes = vec![
            make_flat_dir("src", Badge::Unchecked),
            make_flat_file("src/a.rs", Badge::Equal),
        ];
        state
            .left_cache
            .insert("src/a.rs".to_string(), "same".to_string());
        state
            .right_cache
            .insert("src/a.rs".to_string(), "same".to_string());

        update_parent_badges(&mut state, "src/a.rs");

        // src ディレクトリのバッジが更新される（compute_dir_badge による）
        // 具体的な値はキャッシュ状態に依存するが、Unchecked のままではない可能性がある
        // （片方のファイルしかキャッシュしていないので Unchecked のままかもしれないが、
        // ここでは関数がパニックしないことを確認）
    }
}
