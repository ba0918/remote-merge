//! CLI パス引数の解決ロジック。
//!
//! ユーザー指定パス（ファイル・ディレクトリ・空）をツリーデータから
//! 対象ファイルリストに展開する純粋関数。I/O なし。

use std::collections::BTreeSet;

use crate::service::types::{FileStatus, FileStatusKind, MergeSkipped};
use crate::tree::{FileNode, FileTree};

/// パスリストに `"."` や `"./"` などのルートマーカーが含まれているかを判定する。
///
/// ルートマーカーが含まれている場合、全ファイルを対象にする。
fn has_root_marker(paths: &[String]) -> bool {
    paths.iter().any(|p| {
        let n = p.trim_end_matches('/');
        n == "." || n.is_empty()
    })
}

/// パスにトラバーサルコンポーネント（`..`）が含まれていないか検証する。
///
/// `../foo`, `foo/..`, `foo/../bar` のいずれもエラーとして拒否する。
pub fn check_path_traversal(paths: &[String]) -> anyhow::Result<()> {
    for path in paths {
        let has_traversal = path.split('/').any(|component| component == "..");
        if has_traversal {
            anyhow::bail!("path traversal not allowed: {}", path);
        }
    }
    Ok(())
}

/// パス引数をファイルリストに解決する。
///
/// - ファイルパス → そのまま返す
/// - ディレクトリパス → 配下のファイルを再帰収集
/// - 空 → 全ファイルを返す
/// - `../` を含むパス → エラー（パストラバーサル防止）
/// - 重複パスは BTreeSet で自動除外
pub fn resolve_target_files(paths: &[String], tree: &FileTree) -> anyhow::Result<Vec<String>> {
    check_path_traversal(paths)?;

    // "." or "./" → treat as "all files" (same as empty paths)
    if paths.is_empty() || has_root_marker(paths) {
        let mut all = collect_all_files(tree);
        all.sort();
        return Ok(all);
    }

    let mut result = BTreeSet::new();
    for path in paths {
        let normalized = path.trim_end_matches('/');
        if is_directory_in_tree(normalized, tree) {
            for file in collect_files_under(normalized, tree) {
                result.insert(file);
            }
        } else {
            // For non-directory paths, use normalized form too
            result.insert(if normalized.is_empty() {
                path.clone()
            } else {
                normalized.to_string()
            });
        }
    }

    Ok(result.into_iter().collect())
}

/// ツリー上で指定パスがディレクトリかどうかを判定する。
fn is_directory_in_tree(path: &str, tree: &FileTree) -> bool {
    let normalized = path.trim_end_matches('/');
    if normalized.is_empty() {
        return false;
    }
    tree.find_node(normalized)
        .map(|node| node.is_dir())
        .unwrap_or(false)
}

/// ツリーからすべてのファイルパスを再帰収集する。
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

/// 指定プレフィックス配下のファイルパスを再帰収集する。
fn collect_files_under(prefix: &str, tree: &FileTree) -> Vec<String> {
    let node = match tree.find_node(prefix) {
        Some(n) => n,
        None => return Vec::new(),
    };

    if !node.is_dir() {
        return vec![prefix.to_string()];
    }

    let mut files = Vec::new();
    collect_files_recursive(node, prefix, &mut files);
    files
}

/// ノードの子を再帰的に走査してファイルパスを収集する。
fn collect_files_recursive(node: &FileNode, current_path: &str, files: &mut Vec<String>) {
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

/// パス引数を status ベースでファイルリストに解決する。
///
/// tree ベースの `resolve_target_files` とは異なり、左右両方のツリーから
/// 計算済みの FileStatus リストを使うことで right-only ファイルも漏れなく含める。
pub fn resolve_target_files_from_statuses(
    paths: &[String],
    statuses: &[FileStatus],
    left_tree: &FileTree,
    right_tree: &FileTree,
) -> anyhow::Result<Vec<String>> {
    check_path_traversal(paths)?;

    // Empty paths or "." / "./" = all files from statuses
    if paths.is_empty() || has_root_marker(paths) {
        let mut all: Vec<String> = statuses.iter().map(|s| s.path.clone()).collect();
        all.sort();
        return Ok(all);
    }

    let mut result = BTreeSet::new();
    for path in paths {
        // Check if it's a directory in either tree
        let is_dir =
            is_directory_in_tree(path, left_tree) || is_directory_in_tree(path, right_tree);
        if is_dir {
            // Collect all status entries under this directory prefix
            let normalized = path.trim_end_matches('/');
            let prefix_with_slash = format!("{}/", normalized);
            for status in statuses {
                if status.path.starts_with(&prefix_with_slash) || status.path == normalized {
                    result.insert(status.path.clone());
                }
            }
        } else {
            result.insert(path.clone());
        }
    }

    Ok(result.into_iter().collect())
}

/// target_files から statuses に存在しないパス（どちら側のツリーにも存在しない）を検出する。
///
/// 返り値: (存在するパス, 存在しないパス) のタプル
pub fn partition_existing_files(
    target_files: &[String],
    statuses: &[FileStatus],
) -> (Vec<String>, Vec<String>) {
    let status_set: std::collections::HashSet<&str> =
        statuses.iter().map(|s| s.path.as_str()).collect();
    let mut existing = Vec::new();
    let mut missing = Vec::new();
    for path in target_files {
        if status_set.contains(path.as_str()) {
            existing.push(path.clone());
        } else {
            missing.push(path.clone());
        }
    }
    (existing, missing)
}

/// status から差分のあるファイルのみをフィルタする（Equal を除外）。
///
/// diff / merge の両方で使用する共通フィルタ。
pub fn filter_changed_files(target_files: &[String], statuses: &[FileStatus]) -> Vec<String> {
    let status_map: std::collections::HashMap<&str, &FileStatusKind> = statuses
        .iter()
        .map(|s| (s.path.as_str(), &s.status))
        .collect();
    target_files
        .iter()
        .filter(|path| !matches!(status_map.get(path.as_str()), Some(FileStatusKind::Equal)))
        .cloned()
        .collect()
}

/// merge 対象のファイルを選別する。
///
/// Equal ファイルは無条件に除外する。
/// RightOnly ファイルは merge 対象から除外し、`delete=false` 時のみ skipped に記録する。
/// `delete=true` 時は `plan_deletions()` で別途処理されるため skipped にも含めない。
/// statuses に存在しないパスは merge 対象に含める（上流で検証済みの前提）。
///
/// 戻り値: (merge対象ファイル, RightOnlyスキップ情報)
pub fn filter_merge_candidates(
    target_files: &[String],
    statuses: &[FileStatus],
    delete: bool,
) -> (Vec<String>, Vec<MergeSkipped>) {
    let status_map: std::collections::HashMap<&str, &FileStatusKind> = statuses
        .iter()
        .map(|s| (s.path.as_str(), &s.status))
        .collect();

    let mut merge_files = Vec::new();
    let mut skipped = Vec::new();

    for path in target_files {
        match status_map.get(path.as_str()) {
            Some(FileStatusKind::Equal) => continue,
            Some(FileStatusKind::RightOnly) => {
                if !delete {
                    skipped.push(MergeSkipped {
                        path: path.clone(),
                        reason: "right-only file (use --delete to remove)".into(),
                    });
                }
                // --delete あり → plan_deletions() で処理するのでここでは何もしない
            }
            _ => merge_files.push(path.clone()),
        }
    }

    (merge_files, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::FileNode;
    use std::path::PathBuf;

    /// テスト用ツリーを構築する。
    ///
    /// ```text
    /// root/
    /// ├── src/
    /// │   ├── main.rs
    /// │   ├── lib.rs
    /// │   └── utils/
    /// │       └── helper.rs
    /// ├── config.toml
    /// └── empty_dir/   (空ディレクトリ)
    /// ```
    fn make_test_tree() -> FileTree {
        FileTree {
            root: PathBuf::from("/project"),
            nodes: vec![
                FileNode::new_dir_with_children(
                    "src",
                    vec![
                        FileNode::new_file("main.rs"),
                        FileNode::new_file("lib.rs"),
                        FileNode::new_dir_with_children(
                            "utils",
                            vec![FileNode::new_file("helper.rs")],
                        ),
                    ],
                ),
                FileNode::new_file("config.toml"),
                FileNode::new_dir_with_children("empty_dir", vec![]),
            ],
        }
    }

    #[test]
    fn file_path_returned_as_is() {
        let tree = make_test_tree();
        let paths = vec!["src/main.rs".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(result, vec!["src/main.rs"]);
    }

    #[test]
    fn directory_expands_to_child_files() {
        let tree = make_test_tree();
        let paths = vec!["src".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(
            result,
            vec!["src/lib.rs", "src/main.rs", "src/utils/helper.rs"]
        );
    }

    #[test]
    fn empty_paths_returns_all_files() {
        let tree = make_test_tree();
        let result = resolve_target_files(&[], &tree).unwrap();
        assert_eq!(
            result,
            vec![
                "config.toml",
                "src/lib.rs",
                "src/main.rs",
                "src/utils/helper.rs",
            ]
        );
    }

    #[test]
    fn path_traversal_rejected() {
        let tree = make_test_tree();
        let paths = vec!["../etc/passwd".to_string()];
        let err = resolve_target_files(&paths, &tree).unwrap_err();
        assert!(
            err.to_string().contains("path traversal not allowed"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn dotdot_alone_rejected() {
        let tree = make_test_tree();
        let paths = vec!["..".to_string()];
        assert!(resolve_target_files(&paths, &tree).is_err());
    }

    #[test]
    fn trailing_dotdot_rejected() {
        let tree = make_test_tree();
        let paths = vec!["src/..".to_string()];
        assert!(resolve_target_files(&paths, &tree).is_err());
    }

    #[test]
    fn middle_dotdot_rejected() {
        let tree = make_test_tree();
        let paths = vec!["src/../etc/passwd".to_string()];
        assert!(resolve_target_files(&paths, &tree).is_err());
    }

    #[test]
    fn duplicate_paths_deduplicated() {
        let tree = make_test_tree();
        let paths = vec![
            "src/main.rs".to_string(),
            "src/main.rs".to_string(),
            "config.toml".to_string(),
        ];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(result, vec!["config.toml", "src/main.rs"]);
    }

    #[test]
    fn nonexistent_path_returned_as_is() {
        let tree = make_test_tree();
        let paths = vec!["nonexistent.txt".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(result, vec!["nonexistent.txt"]);
    }

    #[test]
    fn mixed_file_and_directory() {
        let tree = make_test_tree();
        let paths = vec!["config.toml".to_string(), "src/utils".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(result, vec!["config.toml", "src/utils/helper.rs"]);
    }

    #[test]
    fn empty_directory_returns_no_files() {
        let tree = make_test_tree();
        let paths = vec!["empty_dir".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn directory_and_overlapping_file_deduplicated() {
        let tree = make_test_tree();
        let paths = vec!["src".to_string(), "src/main.rs".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        // src expands to all files, src/main.rs is a duplicate → deduplicated
        assert_eq!(
            result,
            vec!["src/lib.rs", "src/main.rs", "src/utils/helper.rs"]
        );
    }

    #[test]
    fn filter_changed_excludes_equal() {
        let targets = vec!["a.txt".into(), "b.txt".into(), "c.txt".into()];
        let statuses = vec![
            FileStatus {
                path: "a.txt".into(),
                status: FileStatusKind::Modified,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
            FileStatus {
                path: "b.txt".into(),
                status: FileStatusKind::Equal,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
            FileStatus {
                path: "c.txt".into(),
                status: FileStatusKind::LeftOnly,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
        ];
        let result = filter_changed_files(&targets, &statuses);
        assert_eq!(result, vec!["a.txt", "c.txt"]);
    }

    #[test]
    fn filter_changed_includes_unknown_paths() {
        let targets = vec!["unknown.txt".into()];
        let statuses = vec![];
        let result = filter_changed_files(&targets, &statuses);
        assert_eq!(result, vec!["unknown.txt"]);
    }

    // ── resolve_target_files_from_statuses tests ──

    fn make_status(path: &str, kind: FileStatusKind) -> FileStatus {
        FileStatus {
            path: path.to_string(),
            status: kind,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }
    }

    /// right-only ファイルがディレクトリ指定時に含まれることを検証する。
    /// left_tree には src/main.rs のみ、right_tree には src/extra.rs のみ存在する場合、
    /// "src" を指定すると両方が返る。
    #[test]
    fn from_statuses_includes_right_only_files_in_directory() {
        let left_tree = FileTree {
            root: PathBuf::from("/project"),
            nodes: vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("main.rs")],
            )],
        };
        let right_tree = FileTree {
            root: PathBuf::from("/remote"),
            nodes: vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("extra.rs")],
            )],
        };
        let statuses = vec![
            make_status("src/main.rs", FileStatusKind::LeftOnly),
            make_status("src/extra.rs", FileStatusKind::RightOnly),
        ];
        let paths = vec!["src".to_string()];
        let result =
            resolve_target_files_from_statuses(&paths, &statuses, &left_tree, &right_tree).unwrap();
        assert_eq!(result, vec!["src/extra.rs", "src/main.rs"]);
    }

    /// 空パス指定時は statuses の全ファイルが返ることを検証する。
    #[test]
    fn from_statuses_empty_paths_returns_all_status_files() {
        let left_tree = make_test_tree();
        let right_tree = FileTree {
            root: PathBuf::from("/remote"),
            nodes: vec![],
        };
        let statuses = vec![
            make_status("config.toml", FileStatusKind::Modified),
            make_status("src/main.rs", FileStatusKind::LeftOnly),
            make_status("src/lib.rs", FileStatusKind::Equal),
        ];
        let result =
            resolve_target_files_from_statuses(&[], &statuses, &left_tree, &right_tree).unwrap();
        assert_eq!(result, vec!["config.toml", "src/lib.rs", "src/main.rs"]);
    }

    /// パストラバーサルが拒否されることを検証する。
    #[test]
    fn from_statuses_rejects_path_traversal() {
        let tree = make_test_tree();
        let statuses = vec![];
        let paths = vec!["../etc/passwd".to_string()];
        let err = resolve_target_files_from_statuses(&paths, &statuses, &tree, &tree).unwrap_err();
        assert!(err.to_string().contains("path traversal not allowed"));
    }

    /// right_tree にのみ存在するディレクトリが正しく認識されることを検証する。
    #[test]
    fn from_statuses_directory_only_in_right_tree() {
        let left_tree = FileTree {
            root: PathBuf::from("/project"),
            nodes: vec![],
        };
        let right_tree = FileTree {
            root: PathBuf::from("/remote"),
            nodes: vec![FileNode::new_dir_with_children(
                "deploy",
                vec![FileNode::new_file("run.sh")],
            )],
        };
        let statuses = vec![make_status("deploy/run.sh", FileStatusKind::RightOnly)];
        let paths = vec!["deploy".to_string()];
        let result =
            resolve_target_files_from_statuses(&paths, &statuses, &left_tree, &right_tree).unwrap();
        assert_eq!(result, vec!["deploy/run.sh"]);
    }

    /// ファイルパス指定時はそのまま返る（ディレクトリ展開されない）。
    #[test]
    fn from_statuses_file_path_returned_as_is() {
        let tree = make_test_tree();
        let statuses = vec![make_status("config.toml", FileStatusKind::Modified)];
        let paths = vec!["config.toml".to_string()];
        let result = resolve_target_files_from_statuses(&paths, &statuses, &tree, &tree).unwrap();
        assert_eq!(result, vec!["config.toml"]);
    }

    // ── Trailing slash tests ──

    #[test]
    fn trailing_slash_directory_expands() {
        let tree = make_test_tree();
        let paths = vec!["src/".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(
            result,
            vec!["src/lib.rs", "src/main.rs", "src/utils/helper.rs"]
        );
    }

    #[test]
    fn trailing_slash_nested_directory() {
        let tree = make_test_tree();
        let paths = vec!["src/utils/".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(result, vec!["src/utils/helper.rs"]);
    }

    #[test]
    fn trailing_slash_nonexistent_treated_as_file() {
        let tree = make_test_tree();
        let paths = vec!["nonexistent/".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(result, vec!["nonexistent"]);
    }

    #[test]
    fn slash_only_treated_as_all_files() {
        let tree = make_test_tree();
        let paths = vec!["/".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        // "/" normalizes to empty string via trim_end_matches('/'),
        // which is treated as a root marker → returns all files
        assert_eq!(
            result,
            vec![
                "config.toml",
                "src/lib.rs",
                "src/main.rs",
                "src/utils/helper.rs",
            ]
        );
    }

    #[test]
    fn mixed_trailing_slash_paths() {
        let tree = make_test_tree();
        let paths = vec!["src/".to_string(), "config.toml".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(
            result,
            vec![
                "config.toml",
                "src/lib.rs",
                "src/main.rs",
                "src/utils/helper.rs"
            ]
        );
    }

    #[test]
    fn from_statuses_trailing_slash_directory() {
        let left_tree = FileTree {
            root: PathBuf::from("/project"),
            nodes: vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("main.rs"), FileNode::new_file("lib.rs")],
            )],
        };
        let right_tree = FileTree {
            root: PathBuf::from("/remote"),
            nodes: vec![FileNode::new_dir_with_children(
                "src",
                vec![
                    FileNode::new_file("main.rs"),
                    FileNode::new_file("extra.rs"),
                ],
            )],
        };
        let statuses = vec![
            make_status("src/main.rs", FileStatusKind::Modified),
            make_status("src/lib.rs", FileStatusKind::LeftOnly),
            make_status("src/extra.rs", FileStatusKind::RightOnly),
        ];
        let paths = vec!["src/".to_string()];
        let result =
            resolve_target_files_from_statuses(&paths, &statuses, &left_tree, &right_tree).unwrap();
        assert_eq!(result, vec!["src/extra.rs", "src/lib.rs", "src/main.rs"]);
    }

    // ── partition_existing_files tests ──

    #[test]
    fn partition_separates_existing_and_missing() {
        let targets = vec!["a.txt".into(), "b.txt".into(), "missing.txt".into()];
        let statuses = vec![
            make_status("a.txt", FileStatusKind::Modified),
            make_status("b.txt", FileStatusKind::Equal),
        ];
        let (existing, missing) = partition_existing_files(&targets, &statuses);
        assert_eq!(existing, vec!["a.txt", "b.txt"]);
        assert_eq!(missing, vec!["missing.txt"]);
    }

    #[test]
    fn partition_all_existing() {
        let targets = vec!["a.txt".into()];
        let statuses = vec![make_status("a.txt", FileStatusKind::Modified)];
        let (existing, missing) = partition_existing_files(&targets, &statuses);
        assert_eq!(existing, vec!["a.txt"]);
        assert!(missing.is_empty());
    }

    #[test]
    fn partition_all_missing() {
        let targets = vec!["gone.txt".into()];
        let statuses = vec![];
        let (existing, missing) = partition_existing_files(&targets, &statuses);
        assert!(existing.is_empty());
        assert_eq!(missing, vec!["gone.txt"]);
    }

    #[test]
    fn partition_empty_targets() {
        let targets: Vec<String> = vec![];
        let statuses = vec![make_status("a.txt", FileStatusKind::Modified)];
        let (existing, missing) = partition_existing_files(&targets, &statuses);
        assert!(existing.is_empty());
        assert!(missing.is_empty());
    }

    #[test]
    fn multiple_trailing_slashes_normalized() {
        let tree = make_test_tree();
        // Multiple trailing slashes: "src//" -> trim_end_matches('/') -> "src"
        let paths = vec!["src//".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(
            result,
            vec!["src/lib.rs", "src/main.rs", "src/utils/helper.rs"]
        );
    }

    // ── "." root marker tests ──

    #[test]
    fn dot_returns_all_files() {
        let tree = make_test_tree();
        let paths = vec![".".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(
            result,
            vec![
                "config.toml",
                "src/lib.rs",
                "src/main.rs",
                "src/utils/helper.rs",
            ]
        );
    }

    #[test]
    fn dot_slash_returns_all_files() {
        let tree = make_test_tree();
        let paths = vec!["./".to_string()];
        let result = resolve_target_files(&paths, &tree).unwrap();
        assert_eq!(
            result,
            vec![
                "config.toml",
                "src/lib.rs",
                "src/main.rs",
                "src/utils/helper.rs",
            ]
        );
    }

    #[test]
    fn from_statuses_dot_returns_all_status_files() {
        let left_tree = make_test_tree();
        let right_tree = FileTree {
            root: PathBuf::from("/remote"),
            nodes: vec![],
        };
        let statuses = vec![
            make_status("config.toml", FileStatusKind::Modified),
            make_status("src/main.rs", FileStatusKind::LeftOnly),
        ];
        let paths = vec![".".to_string()];
        let result =
            resolve_target_files_from_statuses(&paths, &statuses, &left_tree, &right_tree).unwrap();
        assert_eq!(result, vec!["config.toml", "src/main.rs"]);
    }

    #[test]
    fn from_statuses_dot_slash_returns_all_status_files() {
        let left_tree = make_test_tree();
        let right_tree = FileTree {
            root: PathBuf::from("/remote"),
            nodes: vec![],
        };
        let statuses = vec![
            make_status("config.toml", FileStatusKind::Modified),
            make_status("src/main.rs", FileStatusKind::LeftOnly),
        ];
        let paths = vec!["./".to_string()];
        let result =
            resolve_target_files_from_statuses(&paths, &statuses, &left_tree, &right_tree).unwrap();
        assert_eq!(result, vec!["config.toml", "src/main.rs"]);
    }

    // ── filter_merge_candidates tests ──

    #[test]
    fn filter_merge_candidates_excludes_equal_and_right_only() {
        let targets = vec![
            "a.txt".into(),
            "b.txt".into(),
            "c.txt".into(),
            "d.txt".into(),
        ];
        let statuses = vec![
            make_status("a.txt", FileStatusKind::Modified),
            make_status("b.txt", FileStatusKind::Equal),
            make_status("c.txt", FileStatusKind::LeftOnly),
            make_status("d.txt", FileStatusKind::RightOnly),
        ];
        let (merge_files, skipped) = filter_merge_candidates(&targets, &statuses, false);
        assert_eq!(merge_files, vec!["a.txt", "c.txt"]);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].path, "d.txt");
        assert!(skipped[0].reason.contains("right-only"));
    }

    #[test]
    fn filter_merge_candidates_delete_true_no_skipped() {
        let targets = vec!["a.txt".into(), "b.txt".into()];
        let statuses = vec![
            make_status("a.txt", FileStatusKind::Modified),
            make_status("b.txt", FileStatusKind::RightOnly),
        ];
        let (merge_files, skipped) = filter_merge_candidates(&targets, &statuses, true);
        assert_eq!(merge_files, vec!["a.txt"]);
        assert!(
            skipped.is_empty(),
            "delete=true should not add RightOnly to skipped"
        );
    }

    #[test]
    fn filter_merge_candidates_equal_excluded() {
        let targets = vec!["a.txt".into()];
        let statuses = vec![make_status("a.txt", FileStatusKind::Equal)];
        let (merge_files, skipped) = filter_merge_candidates(&targets, &statuses, false);
        assert!(merge_files.is_empty());
        assert!(
            skipped.is_empty(),
            "Equal files should not appear in skipped"
        );
    }

    #[test]
    fn filter_merge_candidates_empty_input() {
        let (merge_files, skipped) = filter_merge_candidates(&[], &[], false);
        assert!(merge_files.is_empty());
        assert!(skipped.is_empty());
    }

    #[test]
    fn filter_merge_candidates_unknown_path_included() {
        // statuses に存在しないパスは merge 対象に含まれる（filter_changed_files と同じ挙動）
        let targets = vec!["unknown.txt".into()];
        let statuses = vec![];
        let (merge_files, skipped) = filter_merge_candidates(&targets, &statuses, false);
        assert_eq!(merge_files, vec!["unknown.txt"]);
        assert!(skipped.is_empty());
    }
}
