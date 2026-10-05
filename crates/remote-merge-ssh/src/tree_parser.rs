//! find コマンド出力のパース・ツリー構築と、リモートのファイル操作のシェルコマンドの組み立て。

use std::path::Path;

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
    let parts: Vec<&str> = line.splitn(7, '\t').collect();
    if parts.len() < 5 {
        tracing::warn!(
            target: "remote_merge::ssh::tree_parser",
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
    node.link_is_dir = file_type == "l" && parts.get(6).is_some_and(|kind| *kind == "d");
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
        /// 中間構造から FileNode に一括変換する。
        /// children は BTreeMap<String, FileNode> として格納される（キー昇順は自動保証）。
        fn into_file_node(self) -> FileNode {
            let mut node = self.file_node;
            if node.is_dir() || node.link_is_dir {
                let children_map: std::collections::BTreeMap<String, FileNode> = self
                    .children
                    .into_iter()
                    .map(|(k, tn)| (k, tn.into_file_node()))
                    .collect();
                node.children = Some(children_map);
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
                    node.children = Some(std::collections::BTreeMap::new());
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
                d.children = Some(std::collections::BTreeMap::new());
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

    // 最終段階で一括変換 + ソート（children は BTreeMap のためソート済み）
    let mut result: Vec<FileNode> = root_map
        .into_values()
        .map(|tn| tn.into_file_node())
        .collect();
    result.sort_by(dir_first_sort);
    result
}

/// シェル引数をエスケープする
pub fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// リモートパスの親ディレクトリを作成する `mkdir -p` コマンドを構築する。
///
/// 親ディレクトリがない（ルート直下のファイル等）場合は `None` を返す。
pub fn build_mkdir_command(remote_path: &str) -> Option<String> {
    let parent = Path::new(remote_path).parent()?;
    let parent_str = parent.to_string_lossy();
    if parent_str.is_empty() {
        return None;
    }
    Some(format!("mkdir -p {}", shell_escape(&parent_str)))
}

/// ファイルか symlink を消し、ないパスでは 0 以外で終わるコマンドを構築する。
///
/// 存在を確かめてから `rm -f` で消すと、その間に消えた対象を消したと報告するので、確かめずに
/// `-f` のない `rm` で消してないパスを rm の失敗にする。`-f` のない `rm` は書き込めないファイルで
/// 確認を求めるのは標準入力が端末のときだけで、SSH の exec には端末がない。
pub fn build_remove_command(remote_path: &str) -> String {
    format!("rm -- {}", shell_escape(remote_path))
}

/// 親ディレクトリを作ってから symlink を作成・更新するコマンドを構築する。
///
/// 作成先が symlink でない既存のディレクトリなら、`ln -sfn` がその中にリンクを作ってしまうので
/// 何もせずに 0 以外で終わる。既存の symlink は `-n` によりリンクそのものを置き換える。
pub fn build_create_symlink_command(target: &str, remote_path: &str) -> String {
    let q = shell_escape(remote_path);
    let create = format!(
        "{{ [ -L {q} ] || [ ! -d {q} ]; }} && ln -sfn {} {q}",
        shell_escape(target)
    );
    match build_mkdir_command(remote_path) {
        Some(mkdir) => format!("{mkdir} && {create}"),
        None => create,
    }
}

/// 標準入力の base64 をデコードして書き込むコマンドを構築する（[`replace_by_rename`]）。
///
/// `-A` は改行なしの1行 base64 を受け付けるモード（Rust の base64::STANDARD.encode は改行を
/// 挿入しないため必須。`-A` なしだと内部バッファ 80 文字で切り捨てられる）。
pub fn build_write_bytes_command(remote_path: &str) -> String {
    replace_by_rename(remote_path, "openssl base64 -d -A")
}

/// 標準入力をそのまま書き込むコマンドを構築する（[`replace_by_rename`]）。
pub fn build_write_text_command(remote_path: &str) -> String {
    replace_by_rename(remote_path, "cat")
}

/// 標準入力を `writer` で同じディレクトリの一時ファイルに書き、書き込み先の名前に付け替えるコマンド。
///
/// シェルには O_NOFOLLOW がないので、書き込み先が symlink かの確認と書き込みの間に symlink に
/// 替えられてもリンク先へ書かないよう、書き込み先は開かずに名前ごと置き換える。書き込み先が
/// symlink か symlink でないディレクトリなら何も書かずに 0 以外で終わる。
///
/// - 一時ファイルは noclobber で作り、既存のファイルや仕掛けられた symlink を開かない。作れた後に
///   だけ、失敗したときに消す trap を置く（他人のファイルを消さない）
/// - `cp -p` で既存の書き込み先のモードを一時ファイルに写す（POSIX の範囲でモードを読む手段）
/// - 付け替えは書き込み先がディレクトリでもその中に入れない GNU の `mv -T` で行い、使えないときだけ
///   ディレクトリでないことを確かめてから `mv` で行う
/// - 失敗で終わるときも標準入力を読み切るのは、送り手のデータ送信を途中で切らず、終了コードで
///   失敗を返すため
fn replace_by_rename(remote_path: &str, writer: &str) -> String {
    let q = shell_escape(remote_path);
    format!(
        "p={q}; t=\"$p.remote-merge-tmp.$$\"; \
         fail() {{ cat >/dev/null; exit 1; }}; \
         if [ -L \"$p\" ] || [ -d \"$p\" ]; then fail; fi; \
         (set -C; : >\"$t\") 2>/dev/null || fail; \
         trap 'rm -f -- \"$t\"' EXIT; \
         if [ -e \"$p\" ]; then cp -p -- \"$p\" \"$t\" || fail; fi; \
         {writer} >\"$t\" || fail; \
         mv -f -T -- \"$t\" \"$p\" 2>/dev/null || {{ [ ! -d \"$p\" ] && mv -f -- \"$t\" \"$p\"; }} || exit 1; \
         trap - EXIT"
    )
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

    let link_action = r#"-exec sh -c 'set -e; for path do size=$(stat -c %s -- "$path"); mtime=$(stat -c %Y -- "$path"); mode=$(stat -c %a -- "$path"); target=$(readlink -- "$path"); if test -d "$path"; then kind=d; else kind=f; fi; printf "l\t%s\t%s\t%s\t%s\t%s\t%s\n" "$size" "$mtime" "$mode" "$path" "$target" "$kind"; done' _ {} +"#;
    format!(
        "find -L {start_paths} -mindepth 1 -xtype l {link_action} && find -L {start_paths} -mindepth 1 ! -xtype l -printf '%y\\t%s\\t%T@\\t%m\\t%p\\t%l\\n'"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::NodeKind;

    #[cfg(unix)]
    #[test]
    fn recursive_scan_keeps_nested_link_text_and_children() {
        use std::os::unix::fs::symlink;
        let root = tempfile::TempDir::new().unwrap();
        let first = tempfile::TempDir::new().unwrap();
        let second = tempfile::TempDir::new().unwrap();
        std::fs::write(second.path().join("child.txt"), "content").unwrap();
        symlink(second.path(), first.path().join("inner")).unwrap();
        symlink(first.path(), root.path().join("outer")).unwrap();

        let command = build_find_command(root.path().to_str().unwrap(), &[]);
        let output = std::process::Command::new("sh")
            .args(["-c", &command])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let entries: Vec<_> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| parse_find_line(line, root.path().to_str().unwrap(), &[]))
            .collect();
        let tree = build_tree_from_flat(entries);
        let outer = tree.iter().find(|node| node.name == "outer").unwrap();
        assert!(outer.is_symlink(), "{outer:?}");
        assert_eq!(
            outer.kind,
            NodeKind::Symlink {
                target: first.path().to_string_lossy().to_string()
            }
        );
        let inner = &outer.children.as_ref().unwrap()["inner"];
        assert!(inner.is_symlink(), "{inner:?}");
        assert_eq!(
            inner.kind,
            NodeKind::Symlink {
                target: second.path().to_string_lossy().to_string()
            }
        );
        assert!(
            inner.children.as_ref().unwrap().contains_key("child.txt"),
            "{inner:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn recursive_scan_reports_a_cycle_instead_of_a_partial_tree() {
        use std::os::unix::fs::symlink;
        let root = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(root.path().join("folder")).unwrap();
        symlink(root.path(), root.path().join("folder/back")).unwrap();
        let command = build_find_command(root.path().to_str().unwrap(), &[]);
        let output = std::process::Command::new("sh")
            .args(["-c", &command])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        let stderr = String::from_utf8(output.stderr).unwrap().to_lowercase();
        assert!(
            stderr.contains("loop") || stderr.contains("cycle"),
            "{stderr}"
        );
    }

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
    fn a_remote_directory_link_can_be_expanded_without_changing_its_link_kind() {
        let line = "l\t10\t1705312800.0\t777\t/var/www/app/linked\t../shared\td";
        let node = parse_find_line(line, "/var/www/app", &[]).unwrap();
        assert!(node.is_symlink());
        assert!(node.link_is_dir);
        assert!(node.children.is_none());
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
                n.children = Some(std::collections::BTreeMap::new());
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
        let first_child = children.values().next().unwrap();
        assert_eq!(first_child.name, "main.rs");
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
        let b = tree[0].children.as_ref().unwrap().get("b").unwrap();
        assert_eq!(b.name, "b");
        let c = b.children.as_ref().unwrap().get("c").unwrap();
        assert_eq!(c.name, "c");
        let deep = c.children.as_ref().unwrap().get("deep.txt").unwrap();
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
        src1.children = Some(std::collections::BTreeMap::new());
        src1.size = Some(100);
        src1.mtime = Some(Utc.timestamp_opt(1000, 0).unwrap());
        src1.permissions = Some(0o755);

        let mut src2 = FileNode::new_dir("src");
        src2.children = Some(std::collections::BTreeMap::new());
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
                d.children = Some(std::collections::BTreeMap::new());
                d
            },
            {
                let mut d = FileNode::new_dir("b_dir");
                d.children = Some(std::collections::BTreeMap::new());
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
                d.children = Some(std::collections::BTreeMap::new());
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
        assert_eq!(
            tree[1]
                .children
                .as_ref()
                .unwrap()
                .get("lib.rs")
                .unwrap()
                .name,
            "lib.rs"
        );

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
        src1.children = Some(std::collections::BTreeMap::new());
        src1.size = Some(100);
        src1.mtime = Some(Utc.timestamp_opt(1000, 0).unwrap());
        src1.permissions = Some(0o755);

        let mut src2 = FileNode::new_dir("src");
        src2.children = Some(std::collections::BTreeMap::new());
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
        let cmd = build_find_command("/var/www", &[]);
        assert_eq!(cmd.matches("find -L '/var/www' -mindepth 1").count(), 2);
    }

    #[test]
    fn test_build_find_command_with_include() {
        let include = vec!["ja/Back".to_string(), "ja/API".to_string()];
        let cmd = build_find_command("/var/www", &include);
        assert_eq!(
            cmd.matches("find -L '/var/www/ja/Back' '/var/www/ja/API' -mindepth 1")
                .count(),
            2
        );
    }

    #[test]
    fn test_build_find_command_include_single() {
        let include = vec!["src".to_string()];
        let cmd = build_find_command("/var/www", &include);
        assert_eq!(cmd.matches("find -L '/var/www/src' -mindepth 1").count(), 2);
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

    /// 生成したコマンドを実際の sh で実行し、成功したかを返す
    fn sh_succeeds(command: &str) -> bool {
        std::process::Command::new("sh")
            .args(["-c", command])
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
            .status
            .success()
    }

    #[cfg(unix)]
    #[test]
    fn removing_deletes_a_file_or_a_dangling_link_and_fails_on_a_missing_path() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        let dangling = dir.path().join("dangling");
        symlink(dir.path().join("nowhere"), &dangling).unwrap();
        let target = dir.path().join("target.txt");
        std::fs::write(&target, "kept").unwrap();
        let link = dir.path().join("link");
        symlink(&target, &link).unwrap();
        let missing = dir.path().join("missing.txt");
        let read_only = dir.path().join("read-only.txt");
        std::fs::write(&read_only, "x").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&read_only, std::fs::Permissions::from_mode(0o444)).unwrap();
        }

        assert!(sh_succeeds(&build_remove_command(file.to_str().unwrap())));
        assert!(sh_succeeds(&build_remove_command(
            read_only.to_str().unwrap()
        )));
        assert!(sh_succeeds(&build_remove_command(
            dangling.to_str().unwrap()
        )));
        assert!(sh_succeeds(&build_remove_command(link.to_str().unwrap())));
        assert!(!sh_succeeds(&build_remove_command(
            missing.to_str().unwrap()
        )));

        assert!(file.symlink_metadata().is_err());
        assert!(read_only.symlink_metadata().is_err());
        assert!(dangling.symlink_metadata().is_err());
        assert!(link.symlink_metadata().is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "kept");
        assert!(missing.symlink_metadata().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn creating_a_link_makes_its_missing_parent() {
        let dir = tempfile::TempDir::new().unwrap();
        let link = dir.path().join("new/dir/link");

        assert!(sh_succeeds(&build_create_symlink_command(
            "x",
            link.to_str().unwrap()
        )));

        assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("x"));
    }

    #[cfg(unix)]
    #[test]
    fn creating_a_link_over_a_link_retargets_only_the_link() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::TempDir::new().unwrap();
        let old_target = dir.path().join("old.txt");
        std::fs::write(&old_target, "old").unwrap();
        let link = dir.path().join("link");
        symlink(&old_target, &link).unwrap();

        assert!(sh_succeeds(&build_create_symlink_command(
            "new.txt",
            link.to_str().unwrap()
        )));

        assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("new.txt"));
        assert_eq!(std::fs::read_to_string(&old_target).unwrap(), "old");
    }

    #[cfg(unix)]
    #[test]
    fn creating_a_link_where_a_directory_or_a_file_parent_is_fails_without_change() {
        let dir = tempfile::TempDir::new().unwrap();
        let existing_dir = dir.path().join("existing");
        std::fs::create_dir(&existing_dir).unwrap();
        let file_parent = dir.path().join("file.txt");
        std::fs::write(&file_parent, "x").unwrap();

        assert!(!sh_succeeds(&build_create_symlink_command(
            "x",
            existing_dir.to_str().unwrap()
        )));
        assert!(!sh_succeeds(&build_create_symlink_command(
            "x",
            file_parent.join("link").to_str().unwrap()
        )));

        assert_eq!(std::fs::read_dir(&existing_dir).unwrap().count(), 0);
        assert_eq!(std::fs::read_to_string(&file_parent).unwrap(), "x");
    }

    #[test]
    fn test_build_mkdir_command_nested_path() {
        let cmd = build_mkdir_command("/var/www/app/src/handler/mod.rs");
        assert_eq!(cmd.unwrap(), "mkdir -p '/var/www/app/src/handler'");
    }

    #[test]
    fn test_build_mkdir_command_single_depth() {
        let cmd = build_mkdir_command("/var/www/app/main.rs");
        assert_eq!(cmd.unwrap(), "mkdir -p '/var/www/app'");
    }

    #[test]
    fn test_build_mkdir_command_root_file() {
        // ルート直下のファイルは mkdir 不要
        let cmd = build_mkdir_command("/main.rs");
        assert_eq!(cmd.unwrap(), "mkdir -p '/'");
    }

    #[test]
    fn test_build_mkdir_command_no_parent() {
        // 親がないパスは None
        let cmd = build_mkdir_command("file.rs");
        assert!(cmd.is_none());
    }

    #[test]
    fn test_build_mkdir_command_special_chars() {
        let cmd = build_mkdir_command("/var/www/my app/src/file.rs");
        assert_eq!(cmd.unwrap(), "mkdir -p '/var/www/my app/src'");
    }

    /// 生成した書き込みのコマンドを、標準入力に `input` を渡して実際の sh で実行し、成功したかを返す
    fn sh_with_input_succeeds(command: &str, input: &[u8]) -> bool {
        use std::io::Write;
        let mut child = std::process::Command::new("sh")
            .args(["-c", command])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait().unwrap().success()
    }

    /// `real.txt` を指す symlink `link.txt` を置いた一時ディレクトリ
    #[cfg(unix)]
    fn dir_with_link_to_real_file() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("real.txt"), "original").unwrap();
        std::os::unix::fs::symlink("real.txt", dir.path().join("link.txt")).unwrap();
        dir
    }

    #[cfg(unix)]
    fn assert_link_and_target_unchanged(dir: &tempfile::TempDir) {
        assert_eq!(
            std::fs::read_link(dir.path().join("link.txt")).unwrap(),
            Path::new("real.txt")
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("real.txt")).unwrap(),
            "original"
        );
    }

    #[cfg(unix)]
    #[test]
    fn binary_write_refuses_a_symlink_destination() {
        let dir = dir_with_link_to_real_file();
        let link = dir.path().join("link.txt");

        let written = sh_with_input_succeeds(
            &build_write_bytes_command(link.to_str().unwrap()),
            b"bmV3\n",
        );

        assert!(!written);
        assert_link_and_target_unchanged(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn text_write_refuses_a_symlink_destination() {
        let dir = dir_with_link_to_real_file();
        let link = dir.path().join("link.txt");

        let written =
            sh_with_input_succeeds(&build_write_text_command(link.to_str().unwrap()), b"new");

        assert!(!written);
        assert_link_and_target_unchanged(&dir);
    }

    #[test]
    fn writes_replace_a_regular_file_and_create_a_missing_one() {
        let dir = tempfile::TempDir::new().unwrap();
        let existing = dir.path().join("existing.txt");
        std::fs::write(&existing, "old").unwrap();
        let missing = dir.path().join("missing.bin");

        assert!(sh_with_input_succeeds(
            &build_write_text_command(existing.to_str().unwrap()),
            b"new text"
        ));
        assert!(sh_with_input_succeeds(
            &build_write_bytes_command(missing.to_str().unwrap()),
            b"AP8K\n"
        ));

        assert_eq!(std::fs::read(&existing).unwrap(), b"new text");
        assert_eq!(std::fs::read(&missing).unwrap(), [0x00, 0xff, b'\n']);
    }

    /// 書き込みのコマンドを `sh` で実行し、`first` を送って一時ファイルが現れるのを待ってから
    /// `swap` で書き込み先を差し替え、`rest` を送り終えて、成功したかを返す。
    #[cfg(unix)]
    fn write_with_swap_midway(
        command: &str,
        dir: &Path,
        first: &[u8],
        swap: impl FnOnce(),
        rest: &[u8],
    ) -> bool {
        use std::io::Write;
        let before: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        let mut child = std::process::Command::new("sh")
            .args(["-c", command])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(first).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let temporary_appeared = loop {
            let appeared = std::fs::read_dir(dir)
                .unwrap()
                .any(|e| !before.contains(&e.unwrap().path()));
            if appeared || std::time::Instant::now() > deadline {
                break appeared;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        if !temporary_appeared {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            panic!("no temporary file appeared next to the destination");
        }
        swap();
        stdin.write_all(rest).unwrap();
        drop(stdin);
        child.wait().unwrap().success()
    }

    /// 書き込み先 `dest.txt` を置いたディレクトリと、その外の `outside.txt` を置いたディレクトリ
    #[cfg(unix)]
    fn dest_and_outside() -> (tempfile::TempDir, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("dest.txt"), "old").unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        std::fs::write(outside.path().join("outside.txt"), "outside").unwrap();
        (dir, outside)
    }

    #[cfg(unix)]
    #[test]
    fn a_link_swapped_in_during_a_write_keeps_its_target_unchanged() {
        type BuildWrite = fn(&str) -> String;
        let cases: [(BuildWrite, &[u8], &[u8]); 2] = [
            (build_write_bytes_command, b"bmV3", b"\n"),
            (build_write_text_command, b"ne", b"w"),
        ];
        for (build, first, rest) in cases {
            let (dir, outside) = dest_and_outside();
            let dest = dir.path().join("dest.txt");
            let outside_file = outside.path().join("outside.txt");

            write_with_swap_midway(
                &build(dest.to_str().unwrap()),
                dir.path(),
                first,
                || {
                    std::fs::remove_file(&dest).unwrap();
                    std::os::unix::fs::symlink(&outside_file, &dest).unwrap();
                },
                rest,
            );

            assert_eq!(std::fs::read_to_string(&outside_file).unwrap(), "outside");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_write_leaves_no_temporary_file() {
        let (dir, _outside) = dest_and_outside();
        let dest = dir.path().join("dest.txt");

        let written = write_with_swap_midway(
            &build_write_text_command(dest.to_str().unwrap()),
            dir.path(),
            b"new",
            || {
                std::fs::remove_file(&dest).unwrap();
                std::fs::create_dir(&dest).unwrap();
            },
            b"",
        );

        assert!(!written);
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["dest.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_write_keeps_the_mode_of_the_existing_destination() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let dest = dir.path().join("run.sh");
        std::fs::write(&dest, "old").unwrap();
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o751)).unwrap();

        assert!(sh_with_input_succeeds(
            &build_write_text_command(dest.to_str().unwrap()),
            b"new"
        ));

        assert_eq!(std::fs::read(&dest).unwrap(), b"new");
        let mode = std::fs::metadata(&dest).unwrap().permissions().mode();
        assert_eq!(mode & 0o7777, 0o751);
    }

    #[cfg(unix)]
    #[test]
    fn a_write_onto_a_directory_fails_without_change() {
        let dir = tempfile::TempDir::new().unwrap();
        let dest = dir.path().join("folder");
        std::fs::create_dir(&dest).unwrap();

        assert!(!sh_with_input_succeeds(
            &build_write_text_command(dest.to_str().unwrap()),
            b"new"
        ));

        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
