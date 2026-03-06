//! シンボリックリンクのマージ前安全性検証ロジック。
//! 純粋関数のみで構成。I/Oなし。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// シンボリックリンクの安全性検証結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymlinkValidation {
    pub warnings: Vec<SymlinkWarning>,
}

impl SymlinkValidation {
    /// 警告がなければ安全
    pub fn is_safe(&self) -> bool {
        self.warnings.is_empty()
    }
}

/// シンボリックリンクマージ時の警告種別
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymlinkWarning {
    /// リンク先に実体ファイル/ディレクトリが存在する
    TargetExists { path: String },
    /// 循環リンク検出
    CircularLink { chain: Vec<String> },
    /// 相対パスが環境間で意味が変わる可能性
    RelativePathCrossEnv,
    /// ターゲットがディレクトリ（ln -sf の挙動が変わる）
    TargetIsDirectory,
    /// 同じターゲットを参照する他のシンボリックリンクが存在
    SharedTarget {
        target: String,
        other_links: Vec<String>,
    },
}

/// シンボリックリンク情報（ツリーから収集した情報）
#[derive(Debug, Clone)]
pub struct SymlinkEntry {
    /// シンボリックリンクのパス
    pub link_path: String,
    /// リンク先パス
    pub target: String,
}

/// ファイルシステム上の既知のパス情報
#[derive(Debug, Clone)]
pub struct PathInfo {
    /// 実体として存在するか
    pub exists: bool,
    /// ディレクトリかどうか
    pub is_directory: bool,
    /// シンボリックリンクかどうか（リンクの先をたどった結果ではなくリンク自体）
    pub is_symlink: bool,
    /// シンボリックリンクの場合のターゲット
    pub symlink_target: Option<String>,
}

/// シンボリックリンクマージの安全性を検証する。
///
/// # Arguments
/// - `new_target`: マージで設定しようとしているリンク先パス
/// - `link_path`: シンボリックリンク自体のパス
/// - `target_info`: マージ先でのリンクターゲットのパス情報（存在有無等）
/// - `all_symlinks`: ツリー内の全シンボリックリンク一覧
/// - `link_chain`: 循環検出用のリンクチェーン（link_path → target → ...）
pub fn validate_symlink_merge(
    new_target: &str,
    link_path: &str,
    target_info: Option<&PathInfo>,
    all_symlinks: &[SymlinkEntry],
    link_chain: &[String],
) -> SymlinkValidation {
    let mut warnings = Vec::new();

    // 1. ターゲット衝突検知
    if let Some(info) = target_info {
        if info.exists && !info.is_symlink {
            warnings.push(SymlinkWarning::TargetExists {
                path: new_target.to_string(),
            });
        }
        if info.is_directory {
            warnings.push(SymlinkWarning::TargetIsDirectory);
        }
    }

    // 2. 循環リンク検知
    if let Some(cycle) = detect_circular_link(link_path, new_target, link_chain) {
        warnings.push(SymlinkWarning::CircularLink { chain: cycle });
    }

    // 3. 相対パス警告
    if is_relative_path(new_target) {
        warnings.push(SymlinkWarning::RelativePathCrossEnv);
    }

    // 4. 共有ターゲット検知
    let shared = find_shared_target_links(link_path, new_target, all_symlinks);
    if !shared.is_empty() {
        warnings.push(SymlinkWarning::SharedTarget {
            target: new_target.to_string(),
            other_links: shared,
        });
    }

    SymlinkValidation { warnings }
}

/// 相対パスかどうか判定
fn is_relative_path(path: &str) -> bool {
    !path.starts_with('/')
}

/// 循環リンクを検出する。
/// link_chain は既に辿ったリンクのパス列。
/// new_target がチェーン内に存在すれば循環。
fn detect_circular_link(
    link_path: &str,
    new_target: &str,
    link_chain: &[String],
) -> Option<Vec<String>> {
    // new_target を正規化して比較
    let resolved = resolve_relative_target(link_path, new_target);
    let resolved_str = resolved.to_string_lossy().to_string();

    // チェーン内に同じパスがあれば循環
    if link_chain.contains(&resolved_str) || link_chain.contains(&link_path.to_string()) {
        let mut chain: Vec<String> = link_chain.to_vec();
        chain.push(link_path.to_string());
        chain.push(resolved_str);
        return Some(chain);
    }

    None
}

/// 相対パスターゲットをリンクのディレクトリからの相対で解決する
fn resolve_relative_target(link_path: &str, target: &str) -> PathBuf {
    if target.starts_with('/') {
        return PathBuf::from(target);
    }
    let link_dir = Path::new(link_path).parent().unwrap_or(Path::new(""));
    link_dir.join(target)
}

/// 同じターゲットを参照する他のシンボリックリンクを見つける
fn find_shared_target_links(
    link_path: &str,
    new_target: &str,
    all_symlinks: &[SymlinkEntry],
) -> Vec<String> {
    // ターゲットの正規化マップを作成
    let new_resolved = resolve_relative_target(link_path, new_target);

    all_symlinks
        .iter()
        .filter(|entry| {
            // 自分自身は除外
            if entry.link_path == link_path {
                return false;
            }
            let entry_resolved = resolve_relative_target(&entry.link_path, &entry.target);
            entry_resolved == new_resolved
        })
        .map(|entry| entry.link_path.clone())
        .collect()
}

/// ツリー内の全シンボリックリンクをターゲット別にグルーピングする。
/// 複数リンクが同じターゲットを持つものだけ返す。
pub fn group_shared_targets(symlinks: &[SymlinkEntry]) -> HashMap<String, Vec<String>> {
    let mut target_map: HashMap<String, Vec<String>> = HashMap::new();

    for entry in symlinks {
        let resolved = resolve_relative_target(&entry.link_path, &entry.target);
        let key = resolved.to_string_lossy().to_string();
        target_map
            .entry(key)
            .or_default()
            .push(entry.link_path.clone());
    }

    // 2つ以上のリンクがあるものだけ返す
    target_map.retain(|_, links| links.len() > 1);
    target_map
}

/// 警告メッセージを英語で生成する
pub fn warning_message(warning: &SymlinkWarning) -> String {
    match warning {
        SymlinkWarning::TargetExists { path } => {
            format!(
                "Target '{}' already exists as a regular file/directory",
                path
            )
        }
        SymlinkWarning::CircularLink { chain } => {
            format!("Circular symlink detected: {}", chain.join(" -> "))
        }
        SymlinkWarning::RelativePathCrossEnv => {
            "Relative path may resolve differently across environments".to_string()
        }
        SymlinkWarning::TargetIsDirectory => {
            "Target is a directory - ln -sfn will be used to avoid unexpected behavior".to_string()
        }
        SymlinkWarning::SharedTarget {
            target,
            other_links,
        } => {
            format!(
                "Target '{}' is also referenced by: {}",
                target,
                other_links.join(", ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_symlinks(entries: &[(&str, &str)]) -> Vec<SymlinkEntry> {
        entries
            .iter()
            .map(|(link, target)| SymlinkEntry {
                link_path: link.to_string(),
                target: target.to_string(),
            })
            .collect()
    }

    #[test]
    fn test_safe_symlink_no_warnings() {
        let result = validate_symlink_merge("/absolute/target", "/link/path", None, &[], &[]);
        assert!(result.is_safe());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_target_exists_warning() {
        let info = PathInfo {
            exists: true,
            is_directory: false,
            is_symlink: false,
            symlink_target: None,
        };
        let result = validate_symlink_merge("/existing/file", "/link/path", Some(&info), &[], &[]);
        assert!(!result.is_safe());
        assert!(result
            .warnings
            .iter()
            .any(|w| matches!(w, SymlinkWarning::TargetExists { .. })));
    }

    #[test]
    fn test_target_is_directory_warning() {
        let info = PathInfo {
            exists: true,
            is_directory: true,
            is_symlink: false,
            symlink_target: None,
        };
        let result = validate_symlink_merge("/existing/dir", "/link/path", Some(&info), &[], &[]);
        assert!(result
            .warnings
            .iter()
            .any(|w| matches!(w, SymlinkWarning::TargetIsDirectory)));
    }

    #[test]
    fn test_circular_link_detection() {
        let chain = vec!["/a".to_string(), "/b".to_string()];
        let result = validate_symlink_merge("/a", "/c", None, &[], &chain);
        assert!(result
            .warnings
            .iter()
            .any(|w| matches!(w, SymlinkWarning::CircularLink { .. })));
    }

    #[test]
    fn test_relative_path_warning() {
        let result =
            validate_symlink_merge("../shared/config", "/app/links/config", None, &[], &[]);
        assert!(result
            .warnings
            .iter()
            .any(|w| matches!(w, SymlinkWarning::RelativePathCrossEnv)));
    }

    #[test]
    fn test_shared_target_detection() {
        let symlinks = make_symlinks(&[("ja/", "shared"), ("en/", "shared"), ("fr/", "other")]);
        let result = validate_symlink_merge("shared", "ja/", None, &symlinks, &[]);
        assert!(result.warnings.iter().any(|w| match w {
            SymlinkWarning::SharedTarget { other_links, .. } => {
                other_links.contains(&"en/".to_string())
            }
            _ => false,
        }));
    }

    #[test]
    fn test_shared_target_other_links_correct() {
        let symlinks = make_symlinks(&[
            ("link_a", "/shared/target"),
            ("link_b", "/shared/target"),
            ("link_c", "/shared/target"),
        ]);
        let result = validate_symlink_merge("/shared/target", "link_a", None, &symlinks, &[]);
        if let Some(SymlinkWarning::SharedTarget { other_links, .. }) = result
            .warnings
            .iter()
            .find(|w| matches!(w, SymlinkWarning::SharedTarget { .. }))
        {
            assert_eq!(other_links.len(), 2);
            assert!(other_links.contains(&"link_b".to_string()));
            assert!(other_links.contains(&"link_c".to_string()));
        } else {
            panic!("Expected SharedTarget warning");
        }
    }

    #[test]
    fn test_group_shared_targets() {
        let symlinks = make_symlinks(&[("ja/", "shared"), ("en/", "shared"), ("fr/", "unique")]);
        let grouped = group_shared_targets(&symlinks);
        assert_eq!(grouped.len(), 1);
        let shared_group = grouped.values().next().unwrap();
        assert_eq!(shared_group.len(), 2);
    }

    #[test]
    fn test_no_shared_targets() {
        let symlinks = make_symlinks(&[("link_a", "/target_a"), ("link_b", "/target_b")]);
        let grouped = group_shared_targets(&symlinks);
        assert!(grouped.is_empty());
    }

    #[test]
    fn test_existing_symlink_target_no_warning() {
        // ターゲットが既にシンボリックリンクの場合は TargetExists 警告を出さない
        let info = PathInfo {
            exists: true,
            is_directory: false,
            is_symlink: true,
            symlink_target: Some("/other".to_string()),
        };
        let result =
            validate_symlink_merge("/existing/symlink", "/link/path", Some(&info), &[], &[]);
        assert!(!result
            .warnings
            .iter()
            .any(|w| matches!(w, SymlinkWarning::TargetExists { .. })));
    }

    #[test]
    fn test_warning_message_generation() {
        let msg = warning_message(&SymlinkWarning::TargetExists {
            path: "/foo".to_string(),
        });
        assert!(msg.contains("/foo"));

        let msg = warning_message(&SymlinkWarning::RelativePathCrossEnv);
        assert!(msg.contains("Relative"));

        let msg = warning_message(&SymlinkWarning::SharedTarget {
            target: "shared".to_string(),
            other_links: vec!["en/".to_string()],
        });
        assert!(msg.contains("en/"));
    }
}
