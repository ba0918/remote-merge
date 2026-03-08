//! CLI パス引数の解決ロジック。
//!
//! ユーザー指定パス（ファイル・ディレクトリ・空）をツリーデータから
//! 対象ファイルリストに展開する純粋関数。I/O なし。

use std::collections::BTreeSet;

use crate::service::types::{FileStatus, FileStatusKind};
use crate::tree::{FileNode, FileTree};

/// パスにトラバーサルコンポーネント（`..`）が含まれていないか検証する。
///
/// `../foo`, `foo/..`, `foo/../bar` のいずれもエラーとして拒否する。
fn check_path_traversal(paths: &[String]) -> anyhow::Result<()> {
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

    if paths.is_empty() {
        let mut all = collect_all_files(tree);
        all.sort();
        return Ok(all);
    }

    let mut result = BTreeSet::new();
    for path in paths {
        if is_directory_in_tree(path, tree) {
            for file in collect_files_under(path, tree) {
                result.insert(file);
            }
        } else {
            result.insert(path.clone());
        }
    }

    Ok(result.into_iter().collect())
}

/// ツリー上で指定パスがディレクトリかどうかを判定する。
fn is_directory_in_tree(path: &str, tree: &FileTree) -> bool {
    tree.find_node(path)
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

    // Empty paths = all files from statuses
    if paths.is_empty() {
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
            let prefix_with_slash = format!("{}/", path);
            for status in statuses {
                if status.path.starts_with(&prefix_with_slash) || status.path == *path {
                    result.insert(status.path.clone());
                }
            }
        } else {
            result.insert(path.clone());
        }
    }

    Ok(result.into_iter().collect())
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
}
