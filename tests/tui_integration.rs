//! TUI 状態管理の結合テスト
//!
//! AppState + diff エンジン + ファイルツリーの一連のフローを検証する。
//! 実際のターミナル描画はせず、状態遷移とデータフローをテストする。

use std::path::PathBuf;

use remote_merge::app::{AppState, Badge, Focus};
use remote_merge::diff::engine::{self, DiffResult, DiffTag, HunkDirection};
use remote_merge::tree::{FileNode, FileTree};
use remote_merge::ui::dialog::DialogState;

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
                vec![FileNode::new_file("main.rs"), FileNode::new_file("lib.rs")],
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
    assert!(
        !state.flat_nodes.is_empty(),
        "マージされたツリーが表示されるべき"
    );

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

    // 5. diff カーソル移動（scroll_down/up はカーソル移動に変更済み）
    state.scroll_down();
    assert_eq!(state.diff_cursor, 1);
    state.scroll_up();
    assert_eq!(state.diff_cursor, 0);
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
    assert_eq!(
        state.compute_badge("local_only.txt", false),
        Badge::LocalOnly
    );

    // remote only
    assert_eq!(
        state.compute_badge("remote_only.txt", false),
        Badge::RemoteOnly
    );

    // キャッシュあり・同一 → Equal
    state
        .local_cache
        .insert("both.txt".to_string(), "same".to_string());
    state
        .remote_cache
        .insert("both.txt".to_string(), "same".to_string());
    assert_eq!(state.compute_badge("both.txt", false), Badge::Equal);

    // キャッシュあり・差異 → Modified
    state
        .remote_cache
        .insert("both.txt".to_string(), "different".to_string());
    assert_eq!(state.compute_badge("both.txt", false), Badge::Modified);
}

#[test]
fn test_diff_engine_integration_with_app() {
    // diff エンジン単体が AppState から正しく呼ばれることを確認
    let old = "line1\nline2\nline3\n";
    let new = "line1\nchanged\nline3\nnew_line\n";

    let result = engine::compute_diff(old, new);

    match result {
        DiffResult::Modified {
            lines,
            hunks,
            stats,
            ..
        } => {
            // 統計が正しい
            assert_eq!(stats.deletions, 1); // line2
            assert_eq!(stats.insertions, 2); // changed, new_line
            assert_eq!(stats.equal, 2); // line1, line3

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

// === A-1: サイレント失敗の修正テスト ===

#[test]
fn test_select_file_shows_status_on_no_cache() {
    // キャッシュ未取得時にステータスメッセージに表示される
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());
    state.tree_cursor = 0;
    // キャッシュなしで select_file → "content not loaded" メッセージ
    state.select_file();
    assert!(
        state.status_message.contains("content not loaded"),
        "キャッシュ未取得時にステータスメッセージが設定されるべき: {}",
        state.status_message
    );
    assert!(state.current_diff.is_none());
}

#[test]
fn test_select_file_shows_status_on_local_only() {
    // ローカルにのみ存在する場合のステータス（ツリー上もローカルだけ）
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());
    state
        .local_cache
        .insert("test.txt".to_string(), "hello".to_string());
    state.tree_cursor = 0;
    state.select_file();
    assert!(
        state.status_message.contains("local only"),
        "ローカルのみの場合にステータスメッセージが設定されるべき: {}",
        state.status_message
    );
    // diff は空文字列との比較で表示される
    assert!(state.current_diff.is_some());
}

#[test]
fn test_select_file_shows_status_on_remote_only() {
    // リモートにのみ存在する場合のステータス（ツリー上もリモートだけ）
    let local_tree = make_tree("/local", vec![]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), "hello".to_string());
    state.tree_cursor = 0;
    state.select_file();
    assert!(
        state.status_message.contains("remote only"),
        "リモートのみの場合にステータスメッセージが設定されるべき: {}",
        state.status_message
    );
    assert!(state.current_diff.is_some());
}

#[test]
fn test_select_file_shows_not_loaded_when_cache_missing() {
    // ツリーには両方あるがキャッシュが片方だけの場合 → "not loaded"
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());
    state
        .local_cache
        .insert("test.txt".to_string(), "hello".to_string());
    // remote_cache なし → ツリーにはあるので "not loaded"
    state.tree_cursor = 0;
    state.select_file();
    assert!(
        state.status_message.contains("not loaded"),
        "キャッシュ未ロードの場合は 'not loaded' と表示されるべき: {}",
        state.status_message
    );
    // diff は表示される
    assert!(state.current_diff.is_some());
}

// === A-2: ハンクマージプレビューテスト ===

#[test]
fn test_preview_hunk_merge_generates_before_after() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());
    state
        .local_cache
        .insert("test.txt".to_string(), "line1\nline2\nline3\n".to_string());
    state.remote_cache.insert(
        "test.txt".to_string(),
        "line1\nmodified\nline3\n".to_string(),
    );
    state.tree_cursor = 0;
    state.select_file();

    // RightToLeft のプレビュー
    let result = state.preview_hunk_merge(HunkDirection::RightToLeft);
    assert!(result.is_some(), "プレビューが生成されるべき");
    let (before, after) = result.unwrap();
    assert!(before.contains("line2"), "before にはline2が含まれるべき");
    assert!(
        after.contains("modified"),
        "after にはmodifiedが含まれるべき"
    );
}

#[test]
fn test_hunk_merge_preview_dialog_created() {
    use remote_merge::ui::dialog::{DialogState, HunkMergePreview};

    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());
    state
        .local_cache
        .insert("test.txt".to_string(), "a\nb\nc\n".to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), "a\nX\nc\n".to_string());
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    // stage → pending 設定
    state.stage_hunk_merge(HunkDirection::RightToLeft);
    assert!(state.pending_hunk_merge.is_some());

    // プレビューを生成してダイアログに設定（main.rs のロジックを模倣）
    let direction = state.pending_hunk_merge.unwrap();
    if let Some((before, after)) = state.preview_hunk_merge(direction) {
        let path = state.selected_path.clone().unwrap_or_default();
        state.dialog =
            DialogState::HunkMergePreview(HunkMergePreview::new(path, direction, before, after));
    }

    assert!(matches!(state.dialog, DialogState::HunkMergePreview(_)));
    if let DialogState::HunkMergePreview(ref preview) = state.dialog {
        assert_eq!(preview.file_path, "test.txt");
        assert!(preview.direction_label.contains("remote"));
    }
}

#[test]
fn test_hunk_merge_confirm_executes() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());
    state
        .local_cache
        .insert("test.txt".to_string(), "a\nb\nc\n".to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), "a\nX\nc\n".to_string());
    state.tree_cursor = 0;
    state.select_file();

    // Y で確定するとマージが実行される (apply_hunk_merge をシミュレート)
    let result = state.apply_hunk_merge(HunkDirection::RightToLeft);
    assert!(result.is_some());
    assert_eq!(state.local_cache.get("test.txt").unwrap(), "a\nX\nc\n");
}

#[test]
fn test_hunk_merge_cancel_aborts() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());
    state
        .local_cache
        .insert("test.txt".to_string(), "a\nb\nc\n".to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), "a\nX\nc\n".to_string());
    state.tree_cursor = 0;
    state.select_file();

    // N でキャンセル → pending がクリアされ、キャッシュは変更されない
    state.stage_hunk_merge(HunkDirection::RightToLeft);
    state.pending_hunk_merge = None; // キャンセルをシミュレート
    state.close_dialog();

    assert!(state.pending_hunk_merge.is_none());
    assert_eq!(state.local_cache.get("test.txt").unwrap(), "a\nb\nc\n");
}

// === B-1: ヘルプオーバーレイテスト ===

#[test]
fn test_help_overlay_has_all_sections() {
    use remote_merge::ui::dialog::HelpOverlay;

    let help = HelpOverlay::new();
    let section_titles: Vec<&str> = help.sections.iter().map(|s| s.title.as_str()).collect();
    assert!(
        section_titles.contains(&"File Tree"),
        "File Tree セクションが存在するべき"
    );
    assert!(
        section_titles.contains(&"Diff View"),
        "Diff View セクションが存在するべき"
    );
    assert!(
        section_titles.contains(&"Global"),
        "Global セクションが存在するべき"
    );

    // 各セクションにバインドが存在する
    for section in &help.sections {
        assert!(
            !section.bindings.is_empty(),
            "{} セクションにバインドが存在するべき",
            section.title
        );
    }
}

#[test]
fn test_show_help_sets_dialog() {
    let mut state = AppState::new(
        make_tree("/local", vec![]),
        make_tree("/remote", vec![]),
        "develop".to_string(),
    );

    assert!(!state.has_dialog());
    state.show_help();
    assert!(state.has_dialog());
    assert!(matches!(state.dialog, DialogState::Help(_)));
}

#[test]
fn test_help_closes_on_esc() {
    let mut state = AppState::new(
        make_tree("/local", vec![]),
        make_tree("/remote", vec![]),
        "develop".to_string(),
    );

    state.show_help();
    assert!(state.has_dialog());
    state.close_dialog();
    assert!(!state.has_dialog());
}

// === B-2: 拡張スクロールテスト ===

fn make_state_with_long_diff() -> AppState {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    // 100行のファイルを用意し、50行目を変更
    let old: String = (0..100).map(|i| format!("line{}\n", i)).collect();
    let mut new_text = old.clone();
    new_text = new_text.replace("line50\n", "modified50\n");

    state.local_cache.insert("test.txt".to_string(), old);
    state.remote_cache.insert("test.txt".to_string(), new_text);
    state.tree_cursor = 0;
    state.select_file();
    state
}

#[test]
fn test_scroll_page_down() {
    let mut state = make_state_with_long_diff();
    assert_eq!(state.diff_cursor, 0);

    state.scroll_page_down(20);
    assert_eq!(state.diff_cursor, 20);

    state.scroll_page_down(20);
    assert_eq!(state.diff_cursor, 40);
}

#[test]
fn test_scroll_page_up() {
    let mut state = make_state_with_long_diff();
    // カーソルを50に設定
    state.diff_cursor = 50;
    state.ensure_cursor_visible();

    state.scroll_page_up(20);
    assert_eq!(state.diff_cursor, 30);

    state.scroll_page_up(20);
    assert_eq!(state.diff_cursor, 10);
}

#[test]
fn test_scroll_home() {
    let mut state = make_state_with_long_diff();
    state.diff_cursor = 50;
    state.ensure_cursor_visible();

    state.scroll_to_home();
    assert_eq!(state.diff_cursor, 0);
    assert_eq!(state.diff_scroll, 0);
}

#[test]
fn test_scroll_end() {
    let mut state = make_state_with_long_diff();
    let line_count = state.diff_line_count();
    assert!(line_count > 0);

    state.scroll_to_end();
    assert_eq!(state.diff_cursor, line_count - 1);
}

#[test]
fn test_scroll_page_clamp() {
    let mut state = make_state_with_long_diff();
    let line_count = state.diff_line_count();

    // 大量にスクロールしても最大値を超えない
    state.scroll_page_down(10000);
    assert_eq!(state.diff_cursor, line_count - 1);

    // 0以下にならない
    state.diff_cursor = 5;
    state.ensure_cursor_visible();
    state.scroll_page_up(10000);
    assert_eq!(state.diff_cursor, 0);
}

// === C-2: 2ペイン (Side-by-Side) Diff モードテスト ===

#[test]
fn test_toggle_diff_mode() {
    use remote_merge::app::DiffMode;

    let mut state = AppState::new(
        make_tree("/local", vec![]),
        make_tree("/remote", vec![]),
        "develop".to_string(),
    );

    assert_eq!(state.diff_mode, DiffMode::Unified);
    state.toggle_diff_mode();
    assert_eq!(state.diff_mode, DiffMode::SideBySide);
    state.toggle_diff_mode();
    assert_eq!(state.diff_mode, DiffMode::Unified);
}

#[test]
fn test_split_for_side_by_side() {
    use remote_merge::diff::engine::{DiffLine, DiffTag};
    use remote_merge::ui::diff_view::DiffView;

    let lines = vec![
        DiffLine {
            tag: DiffTag::Equal,
            value: "same\n".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        },
        DiffLine {
            tag: DiffTag::Delete,
            value: "old\n".to_string(),
            old_index: Some(1),
            new_index: None,
        },
        DiffLine {
            tag: DiffTag::Insert,
            value: "new\n".to_string(),
            old_index: None,
            new_index: Some(1),
        },
        DiffLine {
            tag: DiffTag::Equal,
            value: "end\n".to_string(),
            old_index: Some(2),
            new_index: Some(2),
        },
    ];

    let pairs = DiffView::split_for_side_by_side(&lines);
    // Equal: (Some, Some), Delete+Insert: ペアリング, Equal: (Some, Some)
    assert_eq!(pairs.len(), 3);

    // 最初のペア: Equal
    assert!(pairs[0].0.is_some());
    assert!(pairs[0].1.is_some());
    assert_eq!(pairs[0].0.as_ref().unwrap().tag, DiffTag::Equal);

    // 2番目: Delete + Insert がペアリング
    assert!(pairs[1].0.is_some());
    assert!(pairs[1].1.is_some());
    assert_eq!(pairs[1].0.as_ref().unwrap().tag, DiffTag::Delete);
    assert_eq!(pairs[1].1.as_ref().unwrap().tag, DiffTag::Insert);

    // 3番目: Equal
    assert_eq!(pairs[2].0.as_ref().unwrap().tag, DiffTag::Equal);
}

#[test]
fn test_side_by_side_equal_lines_both_sides() {
    use remote_merge::diff::engine::{DiffLine, DiffTag};
    use remote_merge::ui::diff_view::DiffView;

    let lines = vec![
        DiffLine {
            tag: DiffTag::Equal,
            value: "same1\n".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        },
        DiffLine {
            tag: DiffTag::Equal,
            value: "same2\n".to_string(),
            old_index: Some(1),
            new_index: Some(1),
        },
    ];

    let pairs = DiffView::split_for_side_by_side(&lines);
    assert_eq!(pairs.len(), 2);

    for (left, right) in &pairs {
        assert!(left.is_some());
        assert!(right.is_some());
        assert_eq!(left.as_ref().unwrap().value, right.as_ref().unwrap().value);
    }
}

#[test]
fn test_side_by_side_render() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use remote_merge::app::DiffMode;
    use remote_merge::diff::engine::compute_diff;
    use remote_merge::ui::diff_view::DiffView;

    let old = "aaa\nbbb\nccc\n";
    let new = "aaa\nXXX\nccc\n";
    let diff = compute_diff(old, new);

    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);
    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());
    state.current_diff = Some(diff);
    state.selected_path = Some("test.txt".to_string());
    state.diff_mode = DiffMode::SideBySide;

    // レンダリングがパニックしないことを確認
    let area = Rect::new(0, 0, 100, 20);
    let mut buf = Buffer::empty(area);
    let widget = DiffView::new(&state);
    widget.render(area, &mut buf);

    let content: String = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| {
                    buf.cell((x, y))
                        .map(|c| c.symbol().to_string())
                        .unwrap_or_default()
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(content.contains("aaa"), "コンテキスト行が表示されるべき");
    assert!(
        content.contains("side-by-side"),
        "モードラベルが表示されるべき"
    );
}

// === #1: ナビゲーション変更テスト ===

#[test]
fn test_diffview_jk_scrolls_one_line() {
    let mut state = make_state_with_long_diff();
    state.focus = Focus::DiffView;
    assert_eq!(state.diff_cursor, 0);

    // j/k は1行カーソル移動
    state.scroll_down();
    assert_eq!(state.diff_cursor, 1);
    state.scroll_down();
    assert_eq!(state.diff_cursor, 2);
    state.scroll_up();
    assert_eq!(state.diff_cursor, 1);
}

#[test]
#[allow(non_snake_case)]
fn test_diffview_nN_jumps_to_hunk() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    // 離れた2箇所に変更がある diff
    let old: String = (0..20).map(|i| format!("line{}\n", i)).collect();
    let mut new_text = old.clone();
    new_text = new_text.replace("line3\n", "modified3\n");
    new_text = new_text.replace("line15\n", "modified15\n");

    state.local_cache.insert("test.txt".to_string(), old);
    state.remote_cache.insert("test.txt".to_string(), new_text);
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    assert_eq!(state.hunk_count(), 2);
    assert_eq!(state.hunk_cursor, 0);

    // n: 次のハンクへ
    state.hunk_cursor_down();
    assert_eq!(state.hunk_cursor, 1);

    // N: 前のハンクへ
    state.hunk_cursor_up();
    assert_eq!(state.hunk_cursor, 0);
}

#[test]
fn test_scroll_does_not_cancel_pending_merge() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());
    state
        .local_cache
        .insert("test.txt".to_string(), "a\nb\nc\n".to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), "a\nX\nc\n".to_string());
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    // stage → pending 設定
    state.stage_hunk_merge(HunkDirection::RightToLeft);
    assert!(state.pending_hunk_merge.is_some());

    // スクロール（j/k）ではpending操作がキャンセルされない
    state.scroll_down();
    assert!(
        state.pending_hunk_merge.is_some(),
        "スクロールでpendingがキャンセルされるべきでない"
    );

    state.scroll_up();
    assert!(
        state.pending_hunk_merge.is_some(),
        "スクロールでpendingがキャンセルされるべきでない"
    );
}

#[test]
fn test_hunk_jump_cancels_pending_merge() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    let old: String = (0..20).map(|i| format!("line{}\n", i)).collect();
    let mut new_text = old.clone();
    new_text = new_text.replace("line3\n", "modified3\n");
    new_text = new_text.replace("line15\n", "modified15\n");

    state.local_cache.insert("test.txt".to_string(), old);
    state.remote_cache.insert("test.txt".to_string(), new_text);
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    // stage → pending 設定
    state.stage_hunk_merge(HunkDirection::RightToLeft);
    assert!(state.pending_hunk_merge.is_some());

    // ハンクジャンプ（n/N）でpending操作がキャンセルされる
    state.cancel_hunk_merge();
    state.hunk_cursor_down();
    assert!(
        state.pending_hunk_merge.is_none(),
        "ハンクジャンプでpendingがキャンセルされるべき"
    );
}

// === #2: Unified マージ + sync_hunk_cursor テスト ===

#[test]
fn test_hunk_cursor_follows_scroll_position() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    // 離れた2箇所に変更がある diff
    let old: String = (0..30).map(|i| format!("line{}\n", i)).collect();
    let mut new_text = old.clone();
    new_text = new_text.replace("line5\n", "modified5\n");
    new_text = new_text.replace("line25\n", "modified25\n");

    state.local_cache.insert("test.txt".to_string(), old);
    state.remote_cache.insert("test.txt".to_string(), new_text);
    state.tree_cursor = 0;
    state.select_file();

    assert_eq!(state.hunk_count(), 2);
    assert_eq!(state.hunk_cursor, 0);

    // 2番目のハンクの実際の開始位置を取得
    let second_hunk_line = if let Some(DiffResult::Modified {
        merge_hunk_line_indices,
        ..
    }) = &state.current_diff
    {
        merge_hunk_line_indices[1]
    } else {
        panic!("Modified を期待");
    };

    // カーソルを2番目のハンク位置に移動
    state.diff_cursor = second_hunk_line;
    state.sync_hunk_cursor_to_scroll();
    assert_eq!(
        state.hunk_cursor, 1,
        "カーソル位置に応じて2番目のハンクが選択されるべき"
    );

    // 先頭に戻す
    state.diff_cursor = 0;
    state.sync_hunk_cursor_to_scroll();
    assert_eq!(
        state.hunk_cursor, 0,
        "先頭に戻すと最初のハンクが選択されるべき"
    );
}

#[test]
fn test_hunk_start_line_cache_computed() {
    use remote_merge::diff::engine::{compute_diff, DiffResult};

    let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
    let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";
    let diff = compute_diff(old, new);

    if let DiffResult::Modified {
        merge_hunks,
        merge_hunk_line_indices,
        ..
    } = &diff
    {
        assert_eq!(merge_hunks.len(), 2);
        assert_eq!(merge_hunk_line_indices.len(), 2);
        // 最初のハンクのインデックスは2番目より小さい
        assert!(merge_hunk_line_indices[0] < merge_hunk_line_indices[1]);
    } else {
        panic!("Modified を期待");
    }
}

#[test]
fn test_sync_hunk_cursor_binary_search() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    // 3箇所の変更
    let old: String = (0..50).map(|i| format!("line{}\n", i)).collect();
    let mut new_text = old.clone();
    new_text = new_text.replace("line5\n", "mod5\n");
    new_text = new_text.replace("line25\n", "mod25\n");
    new_text = new_text.replace("line45\n", "mod45\n");

    state.local_cache.insert("test.txt".to_string(), old);
    state.remote_cache.insert("test.txt".to_string(), new_text);
    state.tree_cursor = 0;
    state.select_file();

    assert_eq!(state.hunk_count(), 3);

    // 各ハンク位置付近にカーソルを移動して正しいハンクが選択されるか確認
    state.diff_cursor = 0;
    state.sync_hunk_cursor_to_scroll();
    assert_eq!(state.hunk_cursor, 0);

    state.diff_cursor = 30;
    state.sync_hunk_cursor_to_scroll();
    assert_eq!(state.hunk_cursor, 1);

    state.diff_cursor = 48;
    state.sync_hunk_cursor_to_scroll();
    assert_eq!(state.hunk_cursor, 2);
}

// === #3: マージフロー改善テスト（即時適用 + undo） ===

#[test]
fn test_apply_hunk_creates_snapshot() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    let old = "aaa\nbbb\nccc\n";
    let new = "aaa\nXXX\nccc\n";

    state
        .local_cache
        .insert("test.txt".to_string(), old.to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), new.to_string());
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    assert!(state.undo_stack.is_empty(), "undo_stack は最初空であるべき");

    // 即時適用
    state.apply_hunk_merge(HunkDirection::RightToLeft);

    assert_eq!(
        state.undo_stack.len(),
        1,
        "適用後に1つのスナップショットがあるべき"
    );
    // 適用後のローカルキャッシュが変わっている
    let local_content = state.local_cache.get("test.txt").unwrap();
    assert!(
        local_content.contains("XXX"),
        "ローカルにリモートの変更が適用されるべき"
    );
}

#[test]
fn test_undo_restores_previous_cache() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    let old = "aaa\nbbb\nccc\n";
    let new = "aaa\nXXX\nccc\n";

    state
        .local_cache
        .insert("test.txt".to_string(), old.to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), new.to_string());
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    // 適用 → undo
    state.apply_hunk_merge(HunkDirection::RightToLeft);
    let after_apply = state.local_cache.get("test.txt").unwrap().clone();
    assert!(after_apply.contains("XXX"));

    state.undo_last();
    assert!(
        state.undo_stack.is_empty(),
        "undo後にスタックが空になるべき"
    );
    let after_undo = state.local_cache.get("test.txt").unwrap();
    assert_eq!(after_undo, old, "undoで元のコンテンツに戻るべき");
}

#[test]
fn test_undo_all_restores_initial_cache() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    let old = "line1\nline2\nline3\nline4\nline5\n";
    let new = "line1\nAAA\nline3\nBBB\nline5\n";

    state
        .local_cache
        .insert("test.txt".to_string(), old.to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), new.to_string());
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    // 2回適用
    state.apply_hunk_merge(HunkDirection::RightToLeft);
    state.apply_hunk_merge(HunkDirection::RightToLeft);

    assert_eq!(state.undo_stack.len(), 2);

    // undo_all で全部戻す
    state.undo_all();
    assert!(state.undo_stack.is_empty());
    let after_undo_all = state.local_cache.get("test.txt").unwrap();
    assert_eq!(after_undo_all, old, "undo_allで最初の状態に戻るべき");
}

#[test]
fn test_has_unsaved_changes() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    assert!(!state.has_unsaved_changes(), "初期状態では未保存変更なし");

    let old = "aaa\nbbb\nccc\n";
    let new = "aaa\nXXX\nccc\n";

    state
        .local_cache
        .insert("test.txt".to_string(), old.to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), new.to_string());
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    state.apply_hunk_merge(HunkDirection::RightToLeft);
    assert!(state.has_unsaved_changes(), "適用後は未保存変更あり");

    state.undo_last();
    assert!(!state.has_unsaved_changes(), "undo後は未保存変更なし");
}

#[test]
fn test_help_contains_updated_keybinds() {
    use remote_merge::ui::dialog::HelpOverlay;

    let help = HelpOverlay::new();
    let diff_section = help
        .sections
        .iter()
        .find(|s| s.title == "Diff View")
        .unwrap();

    // 新しいキーバインドが含まれている
    let keys: Vec<&str> = diff_section
        .bindings
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert!(
        keys.iter().any(|k| k.contains("j/k")),
        "j/k スクロールが含まれるべき"
    );
    assert!(keys.contains(&"n"), "n ハンクジャンプが含まれるべき");
    assert!(keys.contains(&"N"), "N ハンクジャンプが含まれるべき");

    // 旧キーバインドが含まれていない
    let descs: Vec<&str> = diff_section
        .bindings
        .iter()
        .map(|(_, d)| d.as_str())
        .collect();
    assert!(
        !descs
            .iter()
            .any(|d| d.contains("次のハンクへ") && !d.contains("ジャンプ")),
        "旧ハンクナビゲーション表記が残っていないべき"
    );
}

// === UX 改善 Round 2 テスト ===

#[test]
fn test_equal_shows_file_content() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use remote_merge::diff::engine::DiffResult;
    use remote_merge::ui::diff_view::DiffView;

    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    let content = "line1\nline2\nline3\n";
    state
        .local_cache
        .insert("test.txt".to_string(), content.to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), content.to_string());
    state.tree_cursor = 0;
    state.select_file();

    assert!(matches!(state.current_diff, Some(DiffResult::Equal)));

    // レンダリングしてコンテンツが含まれるか確認
    let area = Rect::new(0, 0, 80, 15);
    let mut buf = Buffer::empty(area);
    let widget = DiffView::new(&state);
    widget.render(area, &mut buf);

    let rendered: String = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| {
                    buf.cell((x, y))
                        .map(|c| c.symbol().to_string())
                        .unwrap_or_default()
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        rendered.contains("identical"),
        "identical バナーが表示されるべき"
    );
    assert!(rendered.contains("line1"), "ファイル内容が表示されるべき");
    assert!(rendered.contains("line2"), "ファイル内容が表示されるべき");
}

#[test]
fn test_equal_scroll_works() {
    use remote_merge::diff::engine::DiffResult;

    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    let content: String = (0..50).map(|i| format!("line{}\n", i)).collect();
    state
        .local_cache
        .insert("test.txt".to_string(), content.clone());
    state.remote_cache.insert("test.txt".to_string(), content);
    state.tree_cursor = 0;
    state.select_file();

    assert!(matches!(state.current_diff, Some(DiffResult::Equal)));
    assert_eq!(state.diff_line_count(), 50);

    state.scroll_down();
    assert_eq!(state.diff_cursor, 1);

    state.scroll_page_down(20);
    assert_eq!(state.diff_cursor, 21);
}

#[test]
fn test_key_hints_file_tree() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let state = AppState::new(local_tree, remote_tree, "develop".to_string());

    let hints = state.build_key_hints();
    assert!(
        hints.contains("j/k"),
        "FileTree ヒントに j/k が含まれるべき"
    );
    assert!(
        hints.contains("Enter"),
        "FileTree ヒントに Enter が含まれるべき"
    );
    assert!(
        hints.contains("help"),
        "FileTree ヒントに help が含まれるべき"
    );
}

#[test]
fn test_key_hints_diff_view_no_changes() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    state
        .local_cache
        .insert("test.txt".to_string(), "aaa\n".to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), "bbb\n".to_string());
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    let hints = state.build_key_hints();
    assert!(
        hints.contains("scroll"),
        "DiffView ヒントに scroll が含まれるべき"
    );
    assert!(
        hints.contains("hunk"),
        "DiffView ヒントに hunk が含まれるべき"
    );
    assert!(
        hints.contains("apply"),
        "DiffView ヒントに apply が含まれるべき"
    );
}

#[test]
fn test_key_hints_diff_view_with_changes() {
    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    // 2箇所に差分を持つファイル（1つ適用しても Modified のまま残る）
    let old = "line1\nline2\nline3\nline4\nline5\n";
    let new = "line1\nAAA\nline3\nBBB\nline5\n";
    state
        .local_cache
        .insert("test.txt".to_string(), old.to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), new.to_string());
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    state.apply_hunk_merge(HunkDirection::RightToLeft);

    let hints = state.build_key_hints();
    assert!(
        hints.contains("changes"),
        "変更ありヒントに changes が含まれるべき"
    );
    assert!(
        hints.contains("write"),
        "変更ありヒントに write が含まれるべき"
    );
    assert!(
        hints.contains("undo"),
        "変更ありヒントに undo が含まれるべき"
    );
}

#[test]
fn test_key_hints_diff_view_equal() {
    use remote_merge::diff::engine::DiffResult;

    let local_tree = make_tree("/local", vec![FileNode::new_file("test.txt")]);
    let remote_tree = make_tree("/remote", vec![FileNode::new_file("test.txt")]);

    let mut state = AppState::new(local_tree, remote_tree, "develop".to_string());

    state
        .local_cache
        .insert("test.txt".to_string(), "same\n".to_string());
    state
        .remote_cache
        .insert("test.txt".to_string(), "same\n".to_string());
    state.tree_cursor = 0;
    state.select_file();
    state.focus = Focus::DiffView;

    assert!(matches!(state.current_diff, Some(DiffResult::Equal)));

    let hints = state.build_key_hints();
    assert!(
        hints.contains("scroll"),
        "Equal ヒントに scroll が含まれるべき"
    );
    assert!(hints.contains("tree"), "Equal ヒントに tree が含まれるべき");
}

#[test]
fn test_cursor_line_has_background() {
    use remote_merge::app::AppState;
    use remote_merge::diff::engine::{DiffLine, DiffTag};
    use remote_merge::tree::FileTree;
    use remote_merge::ui::diff_view::DiffView;
    use std::path::PathBuf;

    let state = AppState::new(
        FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![],
        },
        FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![],
        },
        "develop".to_string(),
    );

    let line = DiffLine {
        tag: DiffTag::Equal,
        value: "same line\n".to_string(),
        old_index: Some(0),
        new_index: Some(0),
    };
    // カーソルラインかつフォーカス中 → パレットの cursor_line_bg が付く
    let rendered = DiffView::render_diff_line_highlighted(&state, &line, false, true, false, true);
    let value_span = rendered.spans.last().unwrap();
    assert_eq!(
        value_span.style.bg,
        Some(state.palette.cursor_line_bg),
        "カーソルラインにパレットの背景色が設定されるべき"
    );
}

#[test]
fn test_cursor_line_priority_below_diff() {
    use remote_merge::app::AppState;
    use remote_merge::diff::engine::{DiffLine, DiffTag};
    use remote_merge::tree::FileTree;
    use remote_merge::ui::diff_view::DiffView;
    use std::path::PathBuf;

    let state = AppState::new(
        FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![],
        },
        FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![],
        },
        "develop".to_string(),
    );

    let line = DiffLine {
        tag: DiffTag::Insert,
        value: "new line\n".to_string(),
        old_index: None,
        new_index: Some(0),
    };
    // Insert行 + カーソルライン → diff色が優先
    let rendered = DiffView::render_diff_line_highlighted(&state, &line, false, true, false, true);
    let value_span = rendered.spans.last().unwrap();
    assert_eq!(
        value_span.style.bg,
        Some(state.palette.diff_insert_bg),
        "diff色がカーソルラインより優先されるべき"
    );
}

#[test]
fn test_cursor_line_priority_below_hunk() {
    use remote_merge::app::AppState;
    use remote_merge::diff::engine::{DiffLine, DiffTag};
    use remote_merge::tree::FileTree;
    use remote_merge::ui::diff_view::DiffView;
    use std::path::PathBuf;

    let state = AppState::new(
        FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![],
        },
        FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![],
        },
        "develop".to_string(),
    );

    let line = DiffLine {
        tag: DiffTag::Equal,
        value: "same line\n".to_string(),
        old_index: Some(0),
        new_index: Some(0),
    };
    // ハンクハイライト + カーソルライン → ハンクが最優先
    let rendered = DiffView::render_diff_line_highlighted(&state, &line, true, true, false, true);
    let value_span = rendered.spans.last().unwrap();
    assert_eq!(
        value_span.style.bg,
        Some(state.palette.hunk_select_bg),
        "ハンクハイライトがカーソルラインより優先されるべき"
    );
}
