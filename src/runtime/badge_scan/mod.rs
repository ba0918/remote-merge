//! ディレクトリ展開時の非同期バッジスキャン。
//!
//! 展開されたディレクトリの直下ファイルをバックグラウンドで diff スキャンし、
//! バッジを `[?]` → `[M]`/`[=]` 等に更新する。
//!
//! 責務分離:
//! - helpers: 純粋関数（ファイル収集、バッジ操作）
//! - task: ワーカースレッド処理（ファイル読み込み + 結果送信）
//! - poll: イベントループからのポーリング + バッジ更新

pub mod helpers;
pub mod poll;
pub mod task;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use crate::app::{AppState, BadgeScanMsg, Side};

use super::TuiRuntime;
use helpers::{
    collect_direct_children_files, filter_uncached_paths, revert_loading_badges, set_loading_badges,
};
use task::{BadgeScanParams, ScanSource};

/// バッジスキャンの進行中エントリ
pub struct BadgeScanEntry {
    pub receiver: mpsc::Receiver<BadgeScanMsg>,
    pub cancel_flag: Arc<AtomicBool>,
}

/// バッジスキャンを開始する。
///
/// 1. 直下ファイル一覧を取得（キャッシュ済みを除外）
/// 2. 重複チェック（同じ dir_path のスキャンが進行中なら起動しない）
/// 3. 対象ファイルのバッジを `[?]` → `[..]` に変更
/// 4. ワーカースレッドを起動
pub fn start_badge_scan(state: &mut AppState, runtime: &mut TuiRuntime, dir_path: &str) {
    // 重複チェック
    if runtime.badge_scans.contains_key(dir_path) {
        return;
    }

    // 直下ファイル一覧を取得
    let all_files = collect_direct_children_files(&state.left_tree, &state.right_tree, dir_path);
    if all_files.is_empty() {
        return;
    }

    // ファイル数上限チェック（設定値を参照）
    let max_files = runtime.core.config.badge_scan_max_files;
    if all_files.len() > max_files {
        state.scan_skipped_dirs.insert(dir_path.to_string());
        state.status_message = format!(
            "Badge scan skipped: {} ({} files, limit: {})",
            dir_path,
            all_files.len(),
            max_files
        );
        return;
    }

    // キャッシュ済みを除外
    let uncached = filter_uncached_paths(
        &all_files,
        &state.left_cache,
        &state.right_cache,
        &state.left_binary_cache,
        &state.right_binary_cache,
    );
    if uncached.is_empty() {
        return;
    }

    // バッジを [?] → [..] に変更
    set_loading_badges(&mut state.flat_nodes, &uncached);

    // チャネルとキャンセルフラグを作成
    let (tx, rx) = mpsc::channel();
    let cancel_flag = Arc::new(AtomicBool::new(false));

    // スキャンパラメータを構築
    let left_source = side_to_scan_source(&state.left_source, &runtime.core.config);
    let right_source = side_to_scan_source(&state.right_source, &runtime.core.config);
    let config = runtime.core.config.clone();
    let pp = runtime.core.passphrase_provider.clone();

    let params = BadgeScanParams {
        dir_path: dir_path.to_string(),
        file_paths: uncached,
        left_source,
        right_source,
        config,
        cancel_flag: cancel_flag.clone(),
        passphrase_provider: pp,
    };

    // エントリを登録
    runtime.badge_scans.insert(
        dir_path.to_string(),
        BadgeScanEntry {
            receiver: rx,
            cancel_flag,
        },
    );

    // ワーカースレッドを起動
    std::thread::spawn(move || {
        task::run_badge_scan(&tx, &params);
    });
}

/// 指定ディレクトリのバッジスキャンをキャンセルする。
pub fn cancel_badge_scan(state: &mut AppState, runtime: &mut TuiRuntime, dir_path: &str) {
    if let Some(entry) = runtime.badge_scans.remove(dir_path) {
        // キャンセルフラグを立てる
        entry.cancel_flag.store(true, Ordering::Relaxed);
        // receiver を drop（ワーカースレッドの tx.send が失敗するようになる）
        drop(entry.receiver);
        // [..] バッジを [?] に戻す
        revert_loading_badges(&mut state.flat_nodes, dir_path);
    }
}

/// 全バッジスキャンをキャンセルする（再接続時用）。
pub fn cancel_all_badge_scans(state: &mut AppState, runtime: &mut TuiRuntime) {
    let dir_paths: Vec<String> = runtime.badge_scans.keys().cloned().collect();
    for dir_path in dir_paths {
        cancel_badge_scan(state, runtime, &dir_path);
    }
    state.scan_skipped_dirs.clear();
}

/// Side を ScanSource に変換する
fn side_to_scan_source(side: &Side, config: &crate::config::AppConfig) -> ScanSource {
    match side {
        Side::Local => ScanSource::Local(config.local.root_dir.clone()),
        Side::Remote(name) => ScanSource::Remote(name.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::{Badge, FlatNode};
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

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

    #[test]
    fn start_badge_scan_prevents_duplicate() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = AppState::new(
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs")],
            )]),
            make_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs")],
            )]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        );
        state.expanded_dirs.insert("src".to_string());
        state.rebuild_flat_nodes();

        // バッジスキャンのダミーエントリを事前に挿入
        let (_tx, rx) = mpsc::channel();
        let flag = Arc::new(AtomicBool::new(false));
        runtime.badge_scans.insert(
            "src".to_string(),
            BadgeScanEntry {
                receiver: rx,
                cancel_flag: flag,
            },
        );

        // 同じディレクトリの二重起動は防止される
        start_badge_scan(&mut state, &mut runtime, "src");
        // エントリは1つのまま
        assert_eq!(runtime.badge_scans.len(), 1);
    }

    #[test]
    fn start_badge_scan_different_dirs_allowed() {
        let mut runtime = TuiRuntime::new_for_test();

        // ダミーエントリ
        let (_tx, rx) = mpsc::channel();
        let flag = Arc::new(AtomicBool::new(false));
        runtime.badge_scans.insert(
            "src".to_string(),
            BadgeScanEntry {
                receiver: rx,
                cancel_flag: flag,
            },
        );

        // 異なるディレクトリには別のエントリを追加可能
        let (_tx2, rx2) = mpsc::channel();
        let flag2 = Arc::new(AtomicBool::new(false));
        runtime.badge_scans.insert(
            "lib".to_string(),
            BadgeScanEntry {
                receiver: rx2,
                cancel_flag: flag2,
            },
        );

        assert_eq!(runtime.badge_scans.len(), 2);
    }

    #[test]
    fn cancel_badge_scan_nonexistent_is_noop() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = AppState::new(
            make_tree(vec![]),
            make_tree(vec![]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        );

        // 存在しないパスのキャンセルは no-op
        cancel_badge_scan(&mut state, &mut runtime, "nonexistent");
        assert!(runtime.badge_scans.is_empty());
    }

    #[test]
    fn cancel_all_badge_scans_clears_all() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = AppState::new(
            make_tree(vec![]),
            make_tree(vec![]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        );

        // 複数エントリを追加
        for dir in &["src", "lib", "tests"] {
            let (_tx, rx) = mpsc::channel();
            let flag = Arc::new(AtomicBool::new(false));
            runtime.badge_scans.insert(
                dir.to_string(),
                BadgeScanEntry {
                    receiver: rx,
                    cancel_flag: flag,
                },
            );
        }
        assert_eq!(runtime.badge_scans.len(), 3);

        cancel_all_badge_scans(&mut state, &mut runtime);
        assert!(runtime.badge_scans.is_empty());
    }

    #[test]
    fn cancel_badge_scan_reverts_loading_badges() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = AppState::new(
            make_tree(vec![]),
            make_tree(vec![]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        );
        state.flat_nodes = vec![
            make_flat_file("src/a.rs", Badge::Loading),
            make_flat_file("src/b.rs", Badge::Loading),
        ];

        let (_tx, rx) = mpsc::channel();
        let flag = Arc::new(AtomicBool::new(false));
        runtime.badge_scans.insert(
            "src".to_string(),
            BadgeScanEntry {
                receiver: rx,
                cancel_flag: flag,
            },
        );

        cancel_badge_scan(&mut state, &mut runtime, "src");
        assert_eq!(state.flat_nodes[0].badge, Badge::Unchecked);
        assert_eq!(state.flat_nodes[1].badge, Badge::Unchecked);
    }

    #[test]
    fn cancel_badge_scan_sets_cancel_flag() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = AppState::new(
            make_tree(vec![]),
            make_tree(vec![]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        );

        let (_tx, rx) = mpsc::channel();
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();
        runtime.badge_scans.insert(
            "src".to_string(),
            BadgeScanEntry {
                receiver: rx,
                cancel_flag: flag,
            },
        );

        cancel_badge_scan(&mut state, &mut runtime, "src");
        assert!(flag_clone.load(Ordering::Relaxed));
    }

    #[test]
    fn start_badge_scan_skips_over_max_files() {
        let mut runtime = TuiRuntime::new_for_test();

        // badge_scan_max_files (デフォルト 500) を超えるファイル数のディレクトリ
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

        start_badge_scan(&mut state, &mut runtime, "big");
        assert!(runtime.badge_scans.is_empty());
        assert!(state.status_message.contains("Badge scan skipped"));
    }

    #[test]
    fn start_badge_scan_uses_config_max_files() {
        // runtime の config.badge_scan_max_files を参照することを確認
        let runtime = TuiRuntime::new_for_test();
        assert_eq!(
            runtime.core.config.badge_scan_max_files,
            crate::config::DEFAULT_BADGE_SCAN_MAX_FILES
        );
    }

    #[test]
    fn start_badge_scan_records_skipped_dir() {
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

        start_badge_scan(&mut state, &mut runtime, "big");
        assert!(state.scan_skipped_dirs.contains("big"));
    }

    #[test]
    fn cancel_all_badge_scans_clears_skipped_dirs() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = AppState::new(
            make_tree(vec![]),
            make_tree(vec![]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        );
        state.scan_skipped_dirs.insert("big".to_string());
        state.scan_skipped_dirs.insert("huge".to_string());

        cancel_all_badge_scans(&mut state, &mut runtime);
        assert!(state.scan_skipped_dirs.is_empty());
    }

    fn make_config(local_root: &str) -> crate::config::AppConfig {
        use std::collections::BTreeMap;
        crate::config::AppConfig {
            servers: BTreeMap::new(),
            local: crate::config::LocalConfig {
                root_dir: PathBuf::from(local_root),
            },
            filter: crate::config::FilterConfig::default(),
            ssh: crate::config::SshConfig::default(),
            backup: crate::config::BackupConfig::default(),
            agent: crate::config::AgentConfig::default(),
            defaults: crate::config::DefaultsConfig::default(),
            max_scan_entries: crate::config::DEFAULT_MAX_SCAN_ENTRIES,
            badge_scan_max_files: crate::config::DEFAULT_BADGE_SCAN_MAX_FILES,
        }
    }

    #[test]
    fn side_to_scan_source_local() {
        let config = make_config("/local");
        let source = side_to_scan_source(&Side::Local, &config);
        match source {
            ScanSource::Local(p) => assert_eq!(p, PathBuf::from("/local")),
            _ => panic!("Expected Local"),
        }
    }

    #[test]
    fn side_to_scan_source_remote() {
        let config = make_config("/local");
        let source = side_to_scan_source(&Side::new("develop"), &config);
        match source {
            ScanSource::Remote(name) => assert_eq!(name, "develop"),
            _ => panic!("Expected Remote"),
        }
    }

    /// 起動直後にルートディレクトリ("") のバッジスキャンが起動されることを確認
    #[test]
    fn start_badge_scan_root_dir_adds_entry() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = AppState::new(
            make_tree(vec![FileNode::new_file("README.md")]),
            make_tree(vec![FileNode::new_file("README.md")]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        );
        state.rebuild_flat_nodes();

        // ルートディレクトリのバッジスキャンを起動
        start_badge_scan(&mut state, &mut runtime, "");

        // "" キーが badge_scans に追加される
        assert!(
            runtime.badge_scans.contains_key(""),
            "root dir badge scan entry should be registered"
        );
    }
}
