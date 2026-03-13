//! find コマンド出力のパース・ツリー構築。

use chrono::{DateTime, TimeZone, Utc};

use crate::filter;
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

    // 除外フィルター（相対パス全体でマッチ）
    if filter::is_path_excluded(name, exclude) {
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

/// フラットなノードリスト（相対パス含む名前）から再帰ツリーを構築する
///
/// `parse_find_line` が返す `name` は "src/main.rs" のような相対パスになる。
/// これを "/" で分割して BTreeMap ベースの中間構造に挿入し、
/// 最終段階で一括変換することで Vec↔BTreeMap 往復を排除する。
pub fn build_tree_from_flat(flat_nodes: Vec<FileNode>) -> Vec<FileNode> {
    use std::collections::BTreeMap;

    /// ディレクトリ優先ソート（dir before file, 同種内はアルファベット順）
    fn dir_first_sort(a: &FileNode, b: &FileNode) -> std::cmp::Ordering {
        match (a.is_dir(), b.is_dir()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    }

    /// BTreeMap ベースの中間ツリーノード
    struct TreeNode {
        file_node: FileNode,
        children: BTreeMap<String, TreeNode>,
    }

    impl TreeNode {
        /// 中間構造から FileNode に一括変換（ディレクトリ優先ソート付き）
        fn into_sorted_file_node(self) -> FileNode {
            let mut node = self.file_node;
            if node.is_dir() {
                let mut children: Vec<FileNode> = self
                    .children
                    .into_values()
                    .map(|tn| tn.into_sorted_file_node())
                    .collect();
                children.sort_by(dir_first_sort);
                node.children = Some(children);
            }
            node
        }
    }

    /// パスの各セグメントに沿って中間ツリーにノードを挿入する
    fn insert_into_tree(
        tree: &mut BTreeMap<String, TreeNode>,
        parts: &[&str],
        original_node: &FileNode,
    ) {
        if parts.is_empty() {
            return;
        }

        let name = parts[0];

        if parts.len() == 1 {
            // 末端ノード: 既存エントリがあればメタデータを後勝ちでマージ
            // （後勝ち: 後から来た値が Some なら上書き、None なら既存値を保持）
            if let Some(existing) = tree.get_mut(name) {
                existing.file_node.size = original_node.size.or(existing.file_node.size);
                existing.file_node.mtime = original_node.mtime.or(existing.file_node.mtime);
                existing.file_node.permissions =
                    original_node.permissions.or(existing.file_node.permissions);
            } else {
                let mut node = original_node.clone();
                node.name = name.to_string();
                if node.is_dir() && node.children.is_none() {
                    node.children = Some(Vec::new());
                }
                tree.insert(
                    name.to_string(),
                    TreeNode {
                        file_node: node,
                        children: BTreeMap::new(),
                    },
                );
            }
        } else {
            // 中間ディレクトリ: 存在しなければ暗黙に作成
            let dir = tree.entry(name.to_string()).or_insert_with(|| {
                let mut d = FileNode::new_dir(name);
                d.children = Some(Vec::new());
                TreeNode {
                    file_node: d,
                    children: BTreeMap::new(),
                }
            });
            insert_into_tree(&mut dir.children, &parts[1..], original_node);
        }
    }

    let mut root_map: BTreeMap<String, TreeNode> = BTreeMap::new();

    for node in &flat_nodes {
        let parts: Vec<&str> = node.name.split('/').collect();
        insert_into_tree(&mut root_map, &parts, node);
    }

    // 最終段階で一括変換 + ソート
    let mut result: Vec<FileNode> = root_map
        .into_values()
        .map(|tn| tn.into_sorted_file_node())
        .collect();
    result.sort_by(dir_first_sort);
    result
}

/// シェル引数をエスケープする
pub fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// `find` コマンド文字列を生成する（再帰スキャン用）
///
/// - `include` が空の場合: `root_dir` 単体を走査対象にする
/// - `include` が非空の場合: 各 include パスを `root_dir` と結合して複数の開始パスにする
///
/// 全パスは `shell_escape` でエスケープされる。
pub fn build_find_command(root_dir: &str, include: &[String]) -> String {
    let start_paths = if include.is_empty() {
        shell_escape(root_dir)
    } else {
        let root = root_dir.trim_end_matches('/');
        include
            .iter()
            .map(|p| shell_escape(&format!("{}/{}", root, p)))
            .collect::<Vec<_>>()
            .join(" ")
    };

    format!(
        "find -P {} -mindepth 1 -printf '%y\\t%s\\t%T@\\t%m\\t%p\\t%l\\n'",
        start_paths
    )
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

    #[test]
    fn test_build_tree_from_flat_empty() {
        let tree = build_tree_from_flat(vec![]);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_build_tree_from_flat_duplicate_dir_last_wins() {
        // 同名ディレクトリ "src" が2回来た場合、後勝ちセマンティクス
        use chrono::{TimeZone, Utc};

        let mut src1 = FileNode::new_dir("src");
        src1.children = Some(Vec::new());
        src1.size = Some(100);
        src1.mtime = Some(Utc.timestamp_opt(1000, 0).unwrap());
        src1.permissions = Some(0o755);

        let mut src2 = FileNode::new_dir("src");
        src2.children = Some(Vec::new());
        src2.size = Some(200);
        src2.mtime = Some(Utc.timestamp_opt(2000, 0).unwrap());
        src2.permissions = Some(0o700);

        let tree = build_tree_from_flat(vec![src1, src2]);
        assert_eq!(tree.len(), 1);
        let src = &tree[0];
        assert_eq!(src.name, "src");
        assert!(src.is_dir());
        // 後勝ち: src2 の値が優先される
        assert_eq!(src.size, Some(200));
        assert_eq!(src.mtime, Some(Utc.timestamp_opt(2000, 0).unwrap()));
        assert_eq!(src.permissions, Some(0o700));
    }

    #[test]
    fn test_build_tree_from_flat_dir_before_file_sort() {
        // ディレクトリ優先ソート: dir が file より前、同種内はアルファベット順
        let flat = vec![
            FileNode::new_file("z_file.txt"),
            FileNode::new_file("a_file.txt"),
            {
                let mut d = FileNode::new_dir("m_dir");
                d.children = Some(Vec::new());
                d
            },
            {
                let mut d = FileNode::new_dir("b_dir");
                d.children = Some(Vec::new());
                d
            },
        ];
        let tree = build_tree_from_flat(flat);
        assert_eq!(tree.len(), 4);
        // ディレクトリが先
        assert_eq!(tree[0].name, "b_dir");
        assert!(tree[0].is_dir());
        assert_eq!(tree[1].name, "m_dir");
        assert!(tree[1].is_dir());
        // ファイルが後
        assert_eq!(tree[2].name, "a_file.txt");
        assert!(tree[2].is_file());
        assert_eq!(tree[3].name, "z_file.txt");
        assert!(tree[3].is_file());
    }

    #[test]
    fn test_build_tree_from_flat_root_mixed() {
        // ルートレベル混在: ディレクトリ + ファイル + 空ディレクトリ
        let flat = vec![
            FileNode::new_file("readme.md"),
            {
                let mut d = FileNode::new_dir("empty_dir");
                d.children = Some(Vec::new());
                d
            },
            {
                let mut n = FileNode::new_file("lib.rs");
                n.name = "src/lib.rs".to_string();
                n
            },
            FileNode::new_file("Cargo.toml"),
        ];
        let tree = build_tree_from_flat(flat);
        assert_eq!(tree.len(), 4);

        // ディレクトリが先（empty_dir, src）、ファイルが後（Cargo.toml, readme.md）
        assert_eq!(tree[0].name, "empty_dir");
        assert!(tree[0].is_dir());
        assert!(tree[0].children.as_ref().unwrap().is_empty());

        assert_eq!(tree[1].name, "src");
        assert!(tree[1].is_dir());
        assert_eq!(tree[1].children.as_ref().unwrap().len(), 1);
        assert_eq!(tree[1].children.as_ref().unwrap()[0].name, "lib.rs");

        assert_eq!(tree[2].name, "Cargo.toml");
        assert!(tree[2].is_file());
        assert_eq!(tree[3].name, "readme.md");
        assert!(tree[3].is_file());
    }

    #[test]
    fn test_build_tree_from_flat_duplicate_dir_none_fallback() {
        // 後から来た値が None の場合、既存値を保持する（後勝ち + フォールバック）
        use chrono::{TimeZone, Utc};

        let mut src1 = FileNode::new_dir("src");
        src1.children = Some(Vec::new());
        src1.size = Some(100);
        src1.mtime = Some(Utc.timestamp_opt(1000, 0).unwrap());
        src1.permissions = Some(0o755);

        let mut src2 = FileNode::new_dir("src");
        src2.children = Some(Vec::new());
        src2.size = None; // 後から来たが None → src1 の値にフォールバック
        src2.mtime = Some(Utc.timestamp_opt(2000, 0).unwrap());
        src2.permissions = None;

        let tree = build_tree_from_flat(vec![src1, src2]);
        assert_eq!(tree.len(), 1);
        let src = &tree[0];
        // size: src2 が None なので src1 の 100 にフォールバック
        assert_eq!(src.size, Some(100));
        // mtime: src2 が Some なので後勝ち
        assert_eq!(src.mtime, Some(Utc.timestamp_opt(2000, 0).unwrap()));
        // permissions: src2 が None なので src1 の 755 にフォールバック
        assert_eq!(src.permissions, Some(0o755));
    }

    // ── build_find_command ──

    #[test]
    fn test_build_find_command_no_include() {
        // include 空 → root_dir のみ（従来通り）
        let cmd = build_find_command("/var/www", &[]);
        assert_eq!(
            cmd,
            "find -P '/var/www' -mindepth 1 -printf '%y\\t%s\\t%T@\\t%m\\t%p\\t%l\\n'"
        );
    }

    #[test]
    fn test_build_find_command_with_include() {
        // include あり → 複数の開始パス
        let include = vec!["ja/Back".to_string(), "ja/API".to_string()];
        let cmd = build_find_command("/var/www", &include);
        assert_eq!(
            cmd,
            "find -P '/var/www/ja/Back' '/var/www/ja/API' -mindepth 1 -printf '%y\\t%s\\t%T@\\t%m\\t%p\\t%l\\n'"
        );
    }

    #[test]
    fn test_build_find_command_include_single() {
        let include = vec!["src".to_string()];
        let cmd = build_find_command("/var/www", &include);
        assert_eq!(
            cmd,
            "find -P '/var/www/src' -mindepth 1 -printf '%y\\t%s\\t%T@\\t%m\\t%p\\t%l\\n'"
        );
    }

    #[test]
    fn test_build_find_command_include_with_special_chars() {
        // スペースや特殊文字を含むパス → shell_escape で保護
        let include = vec!["my dir/sub".to_string(), "it's here".to_string()];
        let cmd = build_find_command("/var/www", &include);
        assert!(cmd.contains("'/var/www/my dir/sub'"));
        assert!(cmd.contains("'/var/www/it'\\''s here'"));
    }

    #[test]
    fn test_build_find_command_root_trailing_slash() {
        // root_dir の末尾スラッシュが重複しない
        let include = vec!["src".to_string()];
        let cmd = build_find_command("/var/www/", &include);
        assert!(cmd.contains("'/var/www/src'"));
        // "/var/www//src" にならないことを確認
        assert!(!cmd.contains("//"));
    }
}
