//! find コマンド出力のパース・ツリー構築。

use chrono::{DateTime, TimeZone, Utc};

use crate::tree::FileNode;

/// `find -printf` の出力行をパースする
///
/// フォーマット: `%y\t%s\t%T@\t%m\t%p\t%l`
/// - %y: ファイルタイプ (f=file, d=dir, l=symlink)
/// - %s: サイズ
/// - %T@: mtime (Unix timestamp)
/// - %m: パーミッション (8進数)
/// - %p: フルパス
/// - %l: シンボリックリンク先（リンクでない場合は空）
pub fn parse_find_line(line: &str, base_path: &str, exclude: &[String]) -> Option<FileNode> {
    let parts: Vec<&str> = line.splitn(6, '\t').collect();
    if parts.len() < 5 {
        tracing::warn!(
            "Failed to parse find output (insufficient columns): {}",
            line
        );
        return None;
    }

    let file_type = parts[0];
    let size: Option<u64> = parts[1].parse().ok();
    let mtime_ts: Option<f64> = parts[2].parse().ok();
    let permissions: Option<u32> = u32::from_str_radix(parts[3], 8).ok();
    let full_path = parts[4];
    let link_target = if parts.len() >= 6 { parts[5] } else { "" };

    // ファイル名を抽出
    let name = full_path
        .strip_prefix(base_path)
        .unwrap_or(full_path)
        .trim_start_matches('/');
    if name.is_empty() {
        return None;
    }

    // 除外フィルター
    if should_exclude(name, exclude) {
        return None;
    }

    // mtime 変換
    let mtime: Option<DateTime<Utc>> = mtime_ts.and_then(|ts| {
        Utc.timestamp_opt(ts as i64, ((ts.fract()) * 1_000_000_000.0) as u32)
            .single()
    });

    let mut node = match file_type {
        "d" => FileNode::new_dir(name),
        "l" => FileNode::new_symlink(name, link_target.trim()),
        _ => FileNode::new_file(name),
    };

    node.size = size;
    node.mtime = mtime;
    node.permissions = permissions;

    Some(node)
}

/// ファイル名が除外パターンにマッチするか
pub fn should_exclude(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if glob_match::glob_match(pattern, name) {
            return true;
        }
    }
    false
}

/// フラットなノードリスト（相対パス含む名前）から再帰ツリーを構築する
///
/// `parse_find_line` が返す `name` は "src/main.rs" のような相対パスになる。
/// これを "/" で分割して再帰的にディレクトリ構造に埋め込む。
pub fn build_tree_from_flat(flat_nodes: Vec<FileNode>) -> Vec<FileNode> {
    use std::collections::BTreeMap;

    fn insert_into_tree(
        tree: &mut BTreeMap<String, FileNode>,
        parts: &[&str],
        original_node: &FileNode,
    ) {
        if parts.is_empty() {
            return;
        }

        let name = parts[0];

        if parts.len() == 1 {
            let mut node = original_node.clone();
            node.name = name.to_string();
            if node.is_dir() && node.children.is_none() {
                node.children = Some(Vec::new());
            }
            if let Some(existing) = tree.get_mut(name) {
                existing.size = original_node.size.or(existing.size);
                existing.mtime = original_node.mtime.or(existing.mtime);
                existing.permissions = original_node.permissions.or(existing.permissions);
            } else {
                tree.insert(name.to_string(), node);
            }
        } else {
            let dir = tree.entry(name.to_string()).or_insert_with(|| {
                let mut d = FileNode::new_dir(name);
                d.children = Some(Vec::new());
                d
            });
            if dir.children.is_none() {
                dir.children = Some(Vec::new());
            }
            let children = dir.children.take().unwrap_or_default();
            let mut child_map: BTreeMap<String, FileNode> = BTreeMap::new();
            for child in children {
                child_map.insert(child.name.clone(), child);
            }
            insert_into_tree(&mut child_map, &parts[1..], original_node);
            let mut sorted: Vec<FileNode> = child_map.into_values().collect();
            sorted.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            });
            dir.children = Some(sorted);
        }
    }

    let mut root_map: BTreeMap<String, FileNode> = BTreeMap::new();

    for node in &flat_nodes {
        let parts: Vec<&str> = node.name.split('/').collect();
        insert_into_tree(&mut root_map, &parts, node);
    }

    let mut result: Vec<FileNode> = root_map.into_values().collect();
    result.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    result
}

/// シェル引数をエスケープする
pub fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::NodeKind;

    #[test]
    fn test_parse_find_line_file() {
        let line = "f\t1024\t1705312800.0\t644\t/var/www/app/index.html\t";
        let node = parse_find_line(line, "/var/www/app", &[]).unwrap();

        assert_eq!(node.name, "index.html");
        assert!(node.is_file());
        assert_eq!(node.size, Some(1024));
        assert!(node.mtime.is_some());
        assert_eq!(node.permissions, Some(0o644));
    }

    #[test]
    fn test_parse_find_line_directory() {
        let line = "d\t4096\t1705312800.0\t755\t/var/www/app/src\t";
        let node = parse_find_line(line, "/var/www/app", &[]).unwrap();

        assert_eq!(node.name, "src");
        assert!(node.is_dir());
        assert!(!node.is_loaded());
    }

    #[test]
    fn test_parse_find_line_symlink() {
        let line = "l\t10\t1705312800.0\t777\t/var/www/app/link\t../shared/config";
        let node = parse_find_line(line, "/var/www/app", &[]).unwrap();

        assert_eq!(node.name, "link");
        assert!(node.is_symlink());
        if let NodeKind::Symlink { ref target } = node.kind {
            assert_eq!(target, "../shared/config");
        }
    }

    #[test]
    fn test_parse_find_line_exclude() {
        let line = "d\t4096\t1705312800.0\t755\t/var/www/app/node_modules\t";
        let exclude = vec!["node_modules".to_string()];
        let node = parse_find_line(line, "/var/www/app", &exclude);
        assert!(node.is_none());
    }

    #[test]
    fn test_parse_find_line_root_itself() {
        let line = "d\t4096\t1705312800.0\t755\t/var/www/app\t";
        let node = parse_find_line(line, "/var/www/app", &[]);
        assert!(node.is_none());
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("/var/www/app"), "'/var/www/app'");
        assert_eq!(shell_escape("it's a test"), "'it'\\''s a test'");
    }

    #[test]
    fn test_shell_escape_special_chars() {
        assert_eq!(
            shell_escape("/path/to/my file.txt"),
            "'/path/to/my file.txt'"
        );
        assert_eq!(shell_escape("/path;rm -rf /"), "'/path;rm -rf /'");
        assert_eq!(shell_escape("/path/\"quoted\""), "'/path/\"quoted\"'");
    }

    #[test]
    fn test_build_tree_from_flat_simple() {
        let flat = vec![FileNode::new_file("a.txt"), FileNode::new_file("b.txt")];
        let tree = build_tree_from_flat(flat);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].name, "a.txt");
        assert_eq!(tree[1].name, "b.txt");
    }

    #[test]
    fn test_build_tree_from_flat_nested() {
        let flat = vec![
            {
                let mut n = FileNode::new_dir("src");
                n.children = Some(Vec::new());
                n.name = "src".to_string();
                n
            },
            {
                let mut n = FileNode::new_file("main.rs");
                n.name = "src/main.rs".to_string();
                n
            },
            FileNode::new_file("README.md"),
        ];
        let tree = build_tree_from_flat(flat);

        assert_eq!(tree.len(), 2);

        let src = tree.iter().find(|n| n.name == "src").unwrap();
        assert!(src.is_dir());
        let children = src.children.as_ref().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "main.rs");
    }

    #[test]
    fn test_build_tree_from_flat_deep() {
        let flat = vec![{
            let mut n = FileNode::new_file("deep.txt");
            n.name = "a/b/c/deep.txt".to_string();
            n
        }];
        let tree = build_tree_from_flat(flat);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "a");
        let b = &tree[0].children.as_ref().unwrap()[0];
        assert_eq!(b.name, "b");
        let c = &b.children.as_ref().unwrap()[0];
        assert_eq!(c.name, "c");
        let deep = &c.children.as_ref().unwrap()[0];
        assert_eq!(deep.name, "deep.txt");
    }
}
