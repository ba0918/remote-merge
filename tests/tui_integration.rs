//! TUI 状態管理の結合テスト
//!
//! AppState + diff エンジン + ファイルツリーの一連のフローを検証する。
//! 実際のターミナル描画はせず、状態遷移とデータフローをテストする。

use std::path::PathBuf;

use remote_merge::app::{AppState, Badge, Focus};
use remote_merge::diff::engine::{self, DiffResult, DiffTag};
use remote_merge::tree::{FileNode, FileTree};

/// テスト用ツリーを作成するヘルパー
fn make_tree(root: &str, nodes: Vec<FileNode>) -> FileTree {
    let mut tree = FileTree {
        root: PathBuf::from(root),
        nodes,
    };
    tree.sort();
    tree
}

#[test]
fn test_tree_to_file_select_to_diff_flow() {
    // ローカルとリモートにそれぞれファイルを持つツリーを構築
    let local_tree = make_tree(
        "/local",
        vec![
            FileNode::new_dir_with_children(
                "src",
                vec![
                    FileNode::new_file("main.rs"),
                    FileNode::new_file("lib.rs"),
                ],
            ),
            FileNode::new_file("README.md"),
        ],
    );

    let remote_tree = make_tree(
        "/remote",
        vec![
            FileNode::new_dir_with_children(
                "src",
                vec![
                    FileNode::new_file("main.rs"),
                    // lib.rs はリモートにない
                ],
            ),
            FileNode::new_file("README.md"),
            FileNode::new_file("deploy.sh"), // ローカルにない
        ],
    );

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    // 1. 初期状態確認
    assert_eq!(state.focus, Focus::FileTree);
    assert!(!state.flat_nodes.is_empty(), "マージされたツリーが表示されるべき");

    // 2. ディレクトリ展開
    // src ディレクトリを見つける
    let src_idx = state
        .flat_nodes
        .iter()
        .position(|n| n.name == "src")
        .expect("src が存在するべき");
    state.tree_cursor = src_idx;
    state.toggle_expand();

    // 展開後、子ファイルが見える
    let expanded_count = state.flat_nodes.len();
    assert!(expanded_count > 2, "展開後にファイルが増えるべき");

    // 3. ファイル選択 + diff 計算
    // README.md にキャッシュを設定
    state
        .local_cache
        .insert("README.md".to_string(), "# Hello\n".to_string());
    state
        .remote_cache
        .insert("README.md".to_string(), "# Hello World\n".to_string());

    // README.md を見つけて選択
    let readme_idx = state
        .flat_nodes
        .iter()
        .position(|n| n.name == "README.md")
        .expect("README.md が存在するべき");
    state.tree_cursor = readme_idx;
    state.select_file();

    // diff が計算されること
    assert!(state.current_diff.is_some());
    assert_eq!(state.selected_path, Some("README.md".to_string()));

    match &state.current_diff {
        Some(DiffResult::Modified { stats, .. }) => {
            assert!(stats.deletions > 0 || stats.insertions > 0);
        }
        other => panic!("Modified を期待したが {:?}", other),
    }

    // 4. Tab でフォーカス切替
    state.toggle_focus();
    assert_eq!(state.focus, Focus::DiffView);

    // 5. diff スクロール
    state.scroll_down();
    assert_eq!(state.diff_scroll, 1);
    state.scroll_up();
    assert_eq!(state.diff_scroll, 0);
}

#[test]
fn test_badge_computation() {
    let local_tree = make_tree(
        "/local",
        vec![
            FileNode::new_file("both.txt"),
            FileNode::new_file("local_only.txt"),
        ],
    );

    let remote_tree = make_tree(
        "/remote",
        vec![
            FileNode::new_file("both.txt"),
            FileNode::new_file("remote_only.txt"),
        ],
    );

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    // キャッシュなし → Unchecked
    assert_eq!(state.compute_badge("both.txt", false), Badge::Unchecked);

    // local only
    assert_eq!(state.compute_badge("local_only.txt", false), Badge::LocalOnly);

    // remote only
    assert_eq!(state.compute_badge("remote_only.txt", false), Badge::RemoteOnly);

    // キャッシュあり・同一 → Equal
    state.local_cache.insert("both.txt".to_string(), "same".to_string());
    state.remote_cache.insert("both.txt".to_string(), "same".to_string());
    assert_eq!(state.compute_badge("both.txt", false), Badge::Equal);

    // キャッシュあり・差異 → Modified
    state.remote_cache.insert("both.txt".to_string(), "different".to_string());
    assert_eq!(state.compute_badge("both.txt", false), Badge::Modified);
}

#[test]
fn test_diff_engine_integration_with_app() {
    // diff エンジン単体が AppState から正しく呼ばれることを確認
    let old = "line1\nline2\nline3\n";
    let new = "line1\nchanged\nline3\nnew_line\n";

    let result = engine::compute_diff(old, new);

    match result {
        DiffResult::Modified { lines, hunks, stats } => {
            // 統計が正しい
            assert_eq!(stats.deletions, 1);   // line2
            assert_eq!(stats.insertions, 2);   // changed, new_line
            assert_eq!(stats.equal, 2);        // line1, line3

            // ハンクが存在
            assert!(!hunks.is_empty());

            // 行インデックスが正しい
            let delete_line = lines.iter().find(|l| l.tag == DiffTag::Delete).unwrap();
            assert_eq!(delete_line.old_index, Some(1)); // line2 は index 1
        }
        other => panic!("Modified を期待したが {:?}", other),
    }
}

#[test]
fn test_cursor_navigation_bounds() {
    let local_tree = make_tree(
        "/local",
        vec![
            FileNode::new_file("a.txt"),
            FileNode::new_file("b.txt"),
            FileNode::new_file("c.txt"),
        ],
    );

    let mut state = AppState::new(local_tree, make_tree("/remote", vec![]), "dev".to_string());

    assert_eq!(state.tree_cursor, 0);

    // 上に行こうとしても 0 のまま
    state.cursor_up();
    assert_eq!(state.tree_cursor, 0);

    // 下に移動
    state.cursor_down();
    assert_eq!(state.tree_cursor, 1);
    state.cursor_down();
    assert_eq!(state.tree_cursor, 2);

    // 最下部を超えない
    state.cursor_down();
    assert_eq!(state.tree_cursor, 2);
}
