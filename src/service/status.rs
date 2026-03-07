//! status サービス: ファイル差分一覧の計算。
//!
//! 2つのツリーを受け取り、各ファイルの差分ステータスを計算する純粋関数群。
//! CoreRuntime 経由のI/O操作は呼び出し側（CLI層）が行う。

use std::collections::HashSet;
use std::path::Path;

use crate::tree::{FileTree, NodePresence};

use super::types::*;

/// ツリーからファイルパスを再帰的に収集する。
///
/// ルートレベルの全ノードを起点に再帰走査する。
fn collect_all_files(tree: &FileTree) -> Vec<String> {
    let mut files = Vec::new();
    for node in &tree.nodes {
        if node.is_dir() {
            collect_files_recursive(node, &node.name, &mut files);
        } else {
            files.push(node.name.clone());
        }
    }
    files
}

/// ノードの子を再帰的に走査してファイルパスを収集する。
fn collect_files_recursive(
    node: &crate::tree::FileNode,
    current_path: &str,
    files: &mut Vec<String>,
) {
    let children = match &node.children {
        Some(c) => c,
        None => return,
    };

    for child in children {
        let child_path = format!("{}/{}", current_path, child.name);
        if child.is_dir() {
            collect_files_recursive(child, &child_path, files);
        } else {
            files.push(child_path);
        }
    }
}

/// 2つのツリーからファイルの union を計算する。
fn collect_all_file_paths(left: &FileTree, right: &FileTree) -> Vec<String> {
    let left_paths = collect_all_files(left);
    let right_paths = collect_all_files(right);

    let left_set: HashSet<String> = left_paths.iter().cloned().collect();
    let mut paths = left_paths;

    for path in right_paths {
        if !left_set.contains(&path) {
            paths.push(path);
        }
    }

    paths.sort();
    paths
}

/// 指定パスがセンシティブファイルかどうかを判定する。
pub fn is_sensitive(path: &str, patterns: &[String]) -> bool {
    let filename = path.rsplit('/').next().unwrap_or(path);
    patterns
        .iter()
        .any(|p| glob_match::glob_match(p, filename) || glob_match::glob_match(p, path))
}

/// ツリー構造からファイルの差分ステータスを計算する（純粋関数）。
///
/// 存在チェック + メタデータ比較（size, mtime）で判定。
/// - 片方のみ存在 → `LeftOnly` / `RightOnly`
/// - 両方存在 + size異なる → `Modified`
/// - 両方存在 + size同じ + mtime同じ → `Equal`
/// - 両方存在 + size同じ + mtime異なる/不明 → `Modified`（コンテンツ未比較）
///
/// コンテンツ比較で正確な判定が必要なファイルは `needs_content_compare` で抽出し、
/// `refine_status_with_content` で最終判定する。
pub fn compute_status_from_trees(
    left: &FileTree,
    right: &FileTree,
    sensitive_patterns: &[String],
) -> Vec<FileStatus> {
    let all_paths = collect_all_file_paths(left, right);
    let mut results = Vec::with_capacity(all_paths.len());

    for path in &all_paths {
        let left_presence = left.find_node_or_unloaded(Path::new(path));
        let right_presence = right.find_node_or_unloaded(Path::new(path));

        let status = match (left_presence, right_presence) {
            (NodePresence::Found, NodePresence::Found) => compare_by_metadata(left, right, path),
            (NodePresence::Found, NodePresence::NotFound) => FileStatusKind::LeftOnly,
            (NodePresence::NotFound, NodePresence::Found) => FileStatusKind::RightOnly,
            _ => FileStatusKind::Modified, // Unloaded → 不確定だが Modified として扱う
        };

        results.push(FileStatus {
            path: path.clone(),
            status,
            sensitive: is_sensitive(path, sensitive_patterns),
            hunks: None,
        });
    }

    results
}

/// メタデータ（size, mtime）でファイルの差分を判定する。
///
/// 共通ロジック `tree::compare_metadata` を使用し、結果を `FileStatusKind` に変換する。
/// `Undetermined`（コンテンツ比較が必要）は安全側に `Modified` として扱う。
fn compare_by_metadata(left: &FileTree, right: &FileTree, path: &str) -> FileStatusKind {
    let left_node = left.find_node(Path::new(path));
    let right_node = right.find_node(Path::new(path));

    match (left_node, right_node) {
        (Some(l), Some(r)) => match crate::tree::compare_metadata(l, r) {
            crate::tree::MetadataCmp::Equal => FileStatusKind::Equal,
            crate::tree::MetadataCmp::Modified => FileStatusKind::Modified,
            crate::tree::MetadataCmp::Undetermined => FileStatusKind::Modified,
        },
        _ => FileStatusKind::Modified, // ノード取得失敗 → 安全側に倒す
    }
}

/// コンテンツ比較が必要なファイルのパスを抽出する（純粋関数）。
///
/// `compute_status_from_trees` で `Modified` と判定されたファイルのうち、
/// size が一致しているもの（= メタデータだけでは判定できないもの）を返す。
/// size が異なるファイルは確実に Modified なので比較不要。
pub fn needs_content_compare(
    files: &[FileStatus],
    left: &FileTree,
    right: &FileTree,
) -> Vec<String> {
    files
        .iter()
        .filter(|f| f.status == FileStatusKind::Modified)
        .filter(|f| {
            let ln = left.find_node(Path::new(&f.path));
            let rn = right.find_node(Path::new(&f.path));
            match (ln, rn) {
                (Some(l), Some(r)) => {
                    // size が一致 → コンテンツ比較が必要
                    match (l.size, r.size) {
                        (Some(ls), Some(rs)) => ls == rs,
                        _ => true, // size 不明 → 比較必要
                    }
                }
                _ => true, // ノード不明 → 比較必要
            }
        })
        .map(|f| f.path.clone())
        .collect()
}

/// コンテンツ比較結果で FileStatus を更新する（純粋関数）。
///
/// `contents` には左右のコンテンツペアが格納されている。
/// 内容が一致すれば Equal、異なれば Modified のまま。
pub fn refine_status_with_content(
    files: &mut [FileStatus],
    contents: &std::collections::HashMap<String, (String, String)>,
) {
    for file in files.iter_mut() {
        if file.status != FileStatusKind::Modified {
            continue;
        }
        if let Some((left_content, right_content)) = contents.get(&file.path) {
            if left_content == right_content {
                file.status = FileStatusKind::Equal;
            }
        }
    }
}

/// FileStatus 一覧からサマリーを計算する（純粋関数）。
pub fn compute_summary(files: &[FileStatus]) -> StatusSummary {
    let mut summary = StatusSummary::default();
    for file in files {
        match file.status {
            FileStatusKind::Modified => summary.modified += 1,
            FileStatusKind::LeftOnly => summary.left_only += 1,
            FileStatusKind::RightOnly => summary.right_only += 1,
            FileStatusKind::Equal => summary.equal += 1,
        }
    }
    summary
}

/// StatusOutput を組み立てる（純粋関数）。
pub fn build_status_output(
    left_info: SourceInfo,
    right_info: SourceInfo,
    files: Vec<FileStatus>,
    summary_only: bool,
) -> StatusOutput {
    let summary = compute_summary(&files);
    StatusOutput {
        left: left_info,
        right: right_info,
        files: if summary_only { None } else { Some(files) },
        summary,
    }
}

/// exit code を判定する。差分があれば 1、なければ 0。
pub fn status_exit_code(summary: &StatusSummary) -> i32 {
    if summary.modified > 0 || summary.left_only > 0 || summary.right_only > 0 {
        exit_code::DIFF_FOUND
    } else {
        exit_code::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    // ── is_sensitive ──

    #[test]
    fn test_sensitive_env_file() {
        let patterns = vec![".env".into(), ".env.*".into()];
        assert!(is_sensitive(".env", &patterns));
        assert!(is_sensitive(".env.production", &patterns));
        assert!(!is_sensitive("README.md", &patterns));
    }

    #[test]
    fn test_sensitive_nested_path() {
        let patterns = vec!["*.pem".into()];
        assert!(is_sensitive("certs/server.pem", &patterns));
        assert!(!is_sensitive("certs/server.crt", &patterns));
    }

    #[test]
    fn test_sensitive_wildcard() {
        let patterns = vec!["*secret*".into()];
        assert!(is_sensitive("config/secret.yml", &patterns));
        assert!(is_sensitive("my-secret-key.txt", &patterns));
        assert!(!is_sensitive("public.yml", &patterns));
    }

    // ── compute_status_from_trees ──

    #[test]
    fn test_status_left_only() {
        let left = make_tree(vec![FileNode::new_file("only_local.rs")]);
        let right = make_tree(vec![]);
        let files = compute_status_from_trees(&left, &right, &[]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatusKind::LeftOnly);
    }

    #[test]
    fn test_status_right_only() {
        let left = make_tree(vec![]);
        let right = make_tree(vec![FileNode::new_file("only_remote.rs")]);
        let files = compute_status_from_trees(&left, &right, &[]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatusKind::RightOnly);
    }

    #[test]
    fn test_status_both_exist() {
        let left = make_tree(vec![FileNode::new_file("common.rs")]);
        let right = make_tree(vec![FileNode::new_file("common.rs")]);
        let files = compute_status_from_trees(&left, &right, &[]);
        assert_eq!(files.len(), 1);
        // コンテンツ未比較なので Modified
        assert_eq!(files[0].status, FileStatusKind::Modified);
    }

    #[test]
    fn test_status_sensitive_flag() {
        let left = make_tree(vec![FileNode::new_file(".env")]);
        let right = make_tree(vec![FileNode::new_file(".env")]);
        let patterns = vec![".env".into()];
        let files = compute_status_from_trees(&left, &right, &patterns);
        assert!(files[0].sensitive);
    }

    #[test]
    fn test_status_nested_files() {
        let left = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )]);
        let right = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("b.rs"), FileNode::new_file("c.rs")],
        )]);
        let files = compute_status_from_trees(&left, &right, &[]);
        assert_eq!(files.len(), 3);

        let a = files.iter().find(|f| f.path == "src/a.rs").unwrap();
        assert_eq!(a.status, FileStatusKind::LeftOnly);

        let b = files.iter().find(|f| f.path == "src/b.rs").unwrap();
        assert_eq!(b.status, FileStatusKind::Modified);

        let c = files.iter().find(|f| f.path == "src/c.rs").unwrap();
        assert_eq!(c.status, FileStatusKind::RightOnly);
    }

    // ── compute_summary ──

    #[test]
    fn test_summary() {
        let files = vec![
            FileStatus {
                path: "a".into(),
                status: FileStatusKind::Modified,
                sensitive: false,
                hunks: None,
            },
            FileStatus {
                path: "b".into(),
                status: FileStatusKind::LeftOnly,
                sensitive: false,
                hunks: None,
            },
            FileStatus {
                path: "c".into(),
                status: FileStatusKind::RightOnly,
                sensitive: false,
                hunks: None,
            },
            FileStatus {
                path: "d".into(),
                status: FileStatusKind::Equal,
                sensitive: false,
                hunks: None,
            },
        ];
        let summary = compute_summary(&files);
        assert_eq!(summary.modified, 1);
        assert_eq!(summary.left_only, 1);
        assert_eq!(summary.right_only, 1);
        assert_eq!(summary.equal, 1);
    }

    // ── build_status_output ──

    #[test]
    fn test_build_status_output_with_files() {
        let files = vec![FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::Modified,
            sensitive: false,
            hunks: None,
        }];
        let output = build_status_output(
            SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            SourceInfo {
                label: "dev".into(),
                root: "/var/www".into(),
            },
            files,
            false,
        );
        assert!(output.files.is_some());
        assert_eq!(output.summary.modified, 1);
    }

    #[test]
    fn test_build_status_output_summary_only() {
        let files = vec![FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::Modified,
            sensitive: false,
            hunks: None,
        }];
        let output = build_status_output(
            SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            SourceInfo {
                label: "dev".into(),
                root: "/var/www".into(),
            },
            files,
            true,
        );
        assert!(output.files.is_none());
        assert_eq!(output.summary.modified, 1);
    }

    // ── exit code ──

    #[test]
    fn test_exit_code_no_diff() {
        let summary = StatusSummary {
            modified: 0,
            left_only: 0,
            right_only: 0,
            equal: 5,
        };
        assert_eq!(status_exit_code(&summary), exit_code::SUCCESS);
    }

    #[test]
    fn test_exit_code_has_diff() {
        let summary = StatusSummary {
            modified: 1,
            left_only: 0,
            right_only: 0,
            equal: 5,
        };
        assert_eq!(status_exit_code(&summary), exit_code::DIFF_FOUND);
    }

    #[test]
    fn test_exit_code_left_only() {
        let summary = StatusSummary {
            modified: 0,
            left_only: 1,
            right_only: 0,
            equal: 0,
        };
        assert_eq!(status_exit_code(&summary), exit_code::DIFF_FOUND);
    }

    // ── メタデータ比較 ──

    fn make_file_with_meta(
        name: &str,
        size: u64,
        mtime: Option<chrono::DateTime<chrono::Utc>>,
    ) -> FileNode {
        let mut node = FileNode::new_file(name);
        node.size = Some(size);
        node.mtime = mtime;
        node
    }

    #[test]
    fn test_status_equal_when_same_size_and_mtime() {
        use chrono::TimeZone;
        let ts = chrono::Utc.timestamp_opt(1700000000, 0).unwrap();
        let left = make_tree(vec![make_file_with_meta("a.rs", 100, Some(ts))]);
        let right = make_tree(vec![make_file_with_meta("a.rs", 100, Some(ts))]);
        let files = compute_status_from_trees(&left, &right, &[]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatusKind::Equal);
    }

    #[test]
    fn test_status_modified_when_different_size() {
        use chrono::TimeZone;
        let ts = chrono::Utc.timestamp_opt(1700000000, 0).unwrap();
        let left = make_tree(vec![make_file_with_meta("a.rs", 100, Some(ts))]);
        let right = make_tree(vec![make_file_with_meta("a.rs", 200, Some(ts))]);
        let files = compute_status_from_trees(&left, &right, &[]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatusKind::Modified);
    }

    #[test]
    fn test_status_modified_when_same_size_different_mtime() {
        use chrono::TimeZone;
        let ts1 = chrono::Utc.timestamp_opt(1700000000, 0).unwrap();
        let ts2 = chrono::Utc.timestamp_opt(1700000001, 0).unwrap();
        let left = make_tree(vec![make_file_with_meta("a.rs", 100, Some(ts1))]);
        let right = make_tree(vec![make_file_with_meta("a.rs", 100, Some(ts2))]);
        let files = compute_status_from_trees(&left, &right, &[]);
        assert_eq!(files.len(), 1);
        // size一致 + mtime異なる → Modified（コンテンツ比較候補）
        assert_eq!(files[0].status, FileStatusKind::Modified);
    }

    #[test]
    fn test_status_modified_when_no_metadata() {
        // size/mtime が None → Modified（安全側）
        let left = make_tree(vec![FileNode::new_file("a.rs")]);
        let right = make_tree(vec![FileNode::new_file("a.rs")]);
        let files = compute_status_from_trees(&left, &right, &[]);
        assert_eq!(files[0].status, FileStatusKind::Modified);
    }

    // ── needs_content_compare ──

    #[test]
    fn test_needs_content_compare_filters_different_size() {
        use chrono::TimeZone;
        let ts = chrono::Utc.timestamp_opt(1700000000, 0).unwrap();
        let ts2 = chrono::Utc.timestamp_opt(1700000001, 0).unwrap();
        // a.rs: size異なる → Modified確定、コンテンツ比較不要
        // b.rs: size同じ + mtime異なる → コンテンツ比較必要
        let left = make_tree(vec![
            make_file_with_meta("a.rs", 100, Some(ts)),
            make_file_with_meta("b.rs", 200, Some(ts)),
        ]);
        let right = make_tree(vec![
            make_file_with_meta("a.rs", 999, Some(ts)),
            make_file_with_meta("b.rs", 200, Some(ts2)),
        ]);
        let files = compute_status_from_trees(&left, &right, &[]);
        let need_compare = needs_content_compare(&files, &left, &right);

        // a.rs は size 違うのでコンテンツ比較不要
        assert!(!need_compare.contains(&"a.rs".to_string()));
        // b.rs は size 同じ + mtime 違うのでコンテンツ比較必要
        assert!(need_compare.contains(&"b.rs".to_string()));
    }

    // ── refine_status_with_content ──

    #[test]
    fn test_refine_status_equal_when_content_matches() {
        let mut files = vec![FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::Modified,
            sensitive: false,
            hunks: None,
        }];
        let mut contents = std::collections::HashMap::new();
        contents.insert(
            "a.rs".to_string(),
            ("same content".to_string(), "same content".to_string()),
        );
        refine_status_with_content(&mut files, &contents);
        assert_eq!(files[0].status, FileStatusKind::Equal);
    }

    #[test]
    fn test_refine_status_stays_modified_when_content_differs() {
        let mut files = vec![FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::Modified,
            sensitive: false,
            hunks: None,
        }];
        let mut contents = std::collections::HashMap::new();
        contents.insert(
            "a.rs".to_string(),
            ("old content".to_string(), "new content".to_string()),
        );
        refine_status_with_content(&mut files, &contents);
        assert_eq!(files[0].status, FileStatusKind::Modified);
    }

    #[test]
    fn test_refine_status_skips_non_modified() {
        let mut files = vec![FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::LeftOnly,
            sensitive: false,
            hunks: None,
        }];
        let mut contents = std::collections::HashMap::new();
        contents.insert("a.rs".to_string(), ("same".to_string(), "same".to_string()));
        refine_status_with_content(&mut files, &contents);
        // LeftOnly はコンテンツ比較で変更されない
        assert_eq!(files[0].status, FileStatusKind::LeftOnly);
    }
}
