//! find コマンド出力のパース・ツリー構築と、リモートのファイル操作のシェルコマンドの組み立て。

use std::path::Path;

use crate::filter;
use crate::find_records::{parse_find_time, FindRecord, RECORD_FORMAT};
use crate::tree::FileNode;

/// find が出した一つの項目（[`crate::find_records`]）をノードにする。`base_path` 自体と exclude に
/// 当たる項目は None。大きさ・更新時刻・権限を読めない項目は、出力の形が違うので Err
pub fn parse_find_record(
    record: &FindRecord,
    base_path: &str,
    exclude: &[String],
) -> anyhow::Result<Option<FileNode>> {
    let Some(name) = relative_name(&record.path, base_path) else {
        return Ok(None);
    };

    // 除外フィルター（相対パス全体でマッチ）
    if filter::is_path_excluded(name, exclude) {
        return Ok(None);
    }

    let unreadable = |what: &str, raw: &str| {
        anyhow::anyhow!("unexpected output from find: cannot read the {what} of {name:?}: {raw:?}")
    };
    let size: u64 = record
        .size
        .parse()
        .map_err(|_| unreadable("size", &record.size))?;
    let mtime = parse_find_time(&record.mtime)
        .ok_or_else(|| unreadable("modification time", &record.mtime))?;
    let permissions = u32::from_str_radix(&record.mode, 8)
        .map_err(|_| unreadable("permissions", &record.mode))?;

    let is_link = record.kind == "l";
    let mut node = match record.kind.as_str() {
        "d" => FileNode::new_dir(name),
        "l" => FileNode::new_symlink(name, record.link_target.as_str()),
        _ => FileNode::new_file(name),
    };

    node.size = Some(size);
    node.link_is_dir = is_link && record.link_kind == "d";
    node.mtime = Some(mtime);
    node.permissions = Some(permissions);

    Ok(Some(node))
}

/// パスが UTF-8 として読めない項目を報告する名前（`base_path` からの相対パスで、読めない部分を
/// `\xNN` と書いた形）。パスが読める項目、ディレクトリ、exclude に当たる項目は None
pub fn unreadable_record_name(
    record: &FindRecord,
    base_path: &str,
    exclude: &[String],
) -> Option<String> {
    let escaped = record.unreadable_path.as_deref()?;
    let excluded = relative_name(&record.path, base_path)
        .is_none_or(|name| filter::is_path_excluded(name, exclude));
    if record.kind == "d" || excluded {
        return None;
    }
    relative_name(escaped, base_path).map(str::to_string)
}

/// 項目の `base_path` からの相対パス。`base_path` 自体は None
pub fn find_record_name<'a>(record: &'a FindRecord, base_path: &str) -> Option<&'a str> {
    relative_name(&record.path, base_path)
}

fn relative_name<'a>(full_path: &'a str, base_path: &str) -> Option<&'a str> {
    let name = full_path
        .strip_prefix(base_path)
        .unwrap_or(full_path)
        .trim_start_matches('/');
    (!name.is_empty()).then_some(name)
}

/// フラットなノードリスト（相対パス含む名前）から再帰ツリーを構築する
///
/// `parse_find_record` が返す `name` は "src/main.rs" のような相対パスになる。
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

/// 親ディレクトリを作ってから symlink を作るコマンドを構築する。
///
/// 作成先に何か（通常ファイル・ディレクトリ・symlink）があれば置き換えずに 0 以外で終わる。
/// 作成先がディレクトリでもその中にリンクを作らない GNU の `ln -T` で作り、それが使えないとき
/// だけ作成先に何もないことを確かめてから `ln` で作る。リンク先は読み込み元の symlink の文字列
/// なので、`-` で始まってもオプションとして読ませないよう前に `--` を置く。
pub fn build_create_symlink_command(target: &str, remote_path: &str) -> String {
    let t = shell_escape(target);
    let create = format!(
        "p={q}; if ! e=$(ln -s -T -- {t} \"$p\" 2>&1); then \
         if [ -e \"$p\" ] || [ -L \"$p\" ]; then printf '%s\\n' \"$e\" >&2; exit 1; fi; \
         ln -s -- {t} \"$p\"; fi",
        q = shell_escape(remote_path)
    );
    match build_mkdir_command(remote_path) {
        Some(mkdir) => format!("{mkdir} || exit 1; {create}"),
        None => create,
    }
}

/// 標準入力の base64 をデコードして書き込むコマンドを構築する（[`write_in_place`]）。
///
/// `-A` は改行なしの1行 base64 を受け付けるモード（Rust の base64::STANDARD.encode は改行を
/// 挿入しないため必須。`-A` なしだと内部バッファ 80 文字で切り捨てられる）。
pub fn build_write_bytes_command(remote_path: &str) -> String {
    write_in_place(remote_path, "openssl base64 -d -A")
}

/// 標準入力をそのまま書き込むコマンドを構築する（[`write_in_place`]）。
pub fn build_write_text_command(remote_path: &str) -> String {
    write_in_place(remote_path, "cat")
}

/// 標準入力を `writer` に通した中身を、書き込み先のファイルをその場で開いて書くコマンド。
///
/// シェルのリダイレクトには O_NOFOLLOW がないので、書き込み先は dd の `oflag=nofollow` で開き、
/// 最後の要素が symlink なら開かずに失敗する。一時ファイルを作って名前を付け替えると、所有者と
/// ハードリンクが変わり、予測できる一時ファイルの名前を他のユーザーに差し替えられる。
///
/// - `writer` のコマンドがリモートにないときは、書き込み先を開く（切り詰める）前に失敗する
/// - POSIX の sh には pipefail がないので、`writer` の終了ステータスは fd 3 からコマンド置換で
///   取り出し、パイプの終了ステータス（dd のもの）と合わせて確かめる
/// - dd が `oflag=nofollow` を受け付けないときだけ、直前に symlink でないことを確かめてから
///   リダイレクトで書く。確かめた後に symlink に替えられる競合はこの場合だけ残る
/// - 転送量の表示を消す `status=none` は、受け付けない古い dd（coreutils 8.20 より前）には
///   付けない。そのときの表示は誰も読まない標準エラーに出る
/// - 失敗で終わるときも標準入力を読み切るのは、送り手のデータ送信を途中で切らず、終了コードで
///   失敗を返すため
fn write_in_place(remote_path: &str, writer: &str) -> String {
    let q = shell_escape(remote_path);
    let program = writer.split(' ').next().unwrap_or(writer);
    format!(
        "p={q}; fail() {{ cat >/dev/null; exit 1; }}; \
         command -v {program} >/dev/null 2>&1 || fail; \
         if dd if=/dev/null of=/dev/null oflag=nofollow 2>/dev/null; then \
         q=; if dd if=/dev/null of=/dev/null status=none 2>/dev/null; then q=status=none; fi; \
         s=$( {{ {{ {writer} 3>&-; echo $? >&3; }} | dd of=\"$p\" bs=65536 oflag=nofollow $q 3>&-; }} 3>&1 ) || fail; \
         [ \"$s\" = 0 ] || fail; \
         else \
         if [ -L \"$p\" ]; then fail; fi; \
         {writer} >\"$p\" || fail; \
         fi"
    )
}

/// `find` コマンド文字列を生成する（再帰スキャン用）
///
/// - `include` が空の場合: `root_dir` 単体を走査対象にし、`root_dir` 自体は出さない
/// - `include` が非空の場合: 各 include パスを `root_dir` と結合して複数の開始パスにし、
///   開始パス自体も出す（ファイルや symlink の値はその 1 件になり、ディレクトリ symlink の配下は
///   リンクの名前の下に出る）
///
/// `exclude` のうち find のパターンで同じ意味に書けるものは、find がその下へ降りないように
/// 刈り込む（[`crate::find_prune`]）。刈り込めないものは出力を読む側で除く。
///
/// 二つの find は、前の find が失敗しても両方を動かし、失敗した方の終了コードを返す（前の find を
/// 優先する）。診断の文言を見分けられるよう C ロケールで動かす（[`crate::find_diagnostics`]）。
/// 最初の find の前に、ログインシェルの起動時の出力と find の診断を分ける行を標準エラーに書く。
///
/// 全パスは `shell_escape` でエスケープされる。
pub fn build_find_command(root_dir: &str, include: &[String], exclude: &[String]) -> String {
    let root = root_dir.trim_end_matches('/');
    let start_paths = if include.is_empty() {
        format!("{} -mindepth 1", shell_escape(root_dir))
    } else {
        include
            .iter()
            .map(|p| shell_escape(&format!("{}/{}", root, p)))
            .collect::<Vec<_>>()
            .join(" ")
    };

    // find -L の `%s`・`%T@`・`%m`・`%l` はリンク先のものになるので、symlink はリンクを辿らない find で
    // 一つずつ出す。リンク先の種類（`%Y`）は辿った先の種類
    let link_action = format!(
        r#"-exec sh -c 'set -e; for path do find "$path" -maxdepth 0 -printf "{RECORD_FORMAT}"; done' _ {{}} +"#
    );
    let prune = crate::find_prune::prune_clause(root, exclude);
    let marker = shell_escape(crate::find_diagnostics::FIND_STDERR_MARKER);
    format!(
        "printf '%s\\n' {marker} >&2; \
         LC_ALL=C find -L {start_paths} {prune}-xtype l {link_action}; l=$?; \
         LC_ALL=C find -L {start_paths} {prune}! -xtype l -printf '{RECORD_FORMAT}'; p=$?; \
         [ \"$l\" -ne 0 ] && exit \"$l\"; exit \"$p\""
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find_records::parse_find_records;
    use crate::tree::NodeKind;

    /// TAB で区切って書いた列（足りない列は空）を、find が出す一つの項目にする
    fn record(fields: &str) -> FindRecord {
        let mut fields: Vec<&str> = fields.split('\t').collect();
        fields.resize(7, "");
        parse_find_records(format!("{}\0", fields.join("\0")).as_bytes())
            .unwrap()
            .remove(0)
    }

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

        let command = build_find_command(root.path().to_str().unwrap(), &[], &[]);
        let output = std::process::Command::new("sh")
            .args(["-c", &command])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let entries: Vec<_> = parse_find_records(&output.stdout)
            .unwrap()
            .iter()
            .filter_map(|record| {
                parse_find_record(record, root.path().to_str().unwrap(), &[]).unwrap()
            })
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

    #[test]
    fn startup_output_of_the_shell_on_stderr_does_not_fail_the_scan() {
        let root = tempfile::TempDir::new().unwrap();
        std::fs::write(root.path().join("a.txt"), "a").unwrap();
        let base = root.path().to_str().unwrap();
        let command = build_find_command(base, &[], &[]);
        let output = std::process::Command::new("sh")
            .args(["-c", &format!("echo 'startup message' >&2; {command}")])
            .output()
            .unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        let exit_code = output.status.code().unwrap() as u32;
        assert!(
            !crate::find_diagnostics::scan_failed(exit_code, &stderr, base, &[]),
            "{stderr}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn recursive_scan_does_not_descend_into_an_excluded_directory() {
        use std::os::unix::fs::symlink;
        let root = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("a/b")).unwrap();
        std::fs::write(root.path().join("a/keep.txt"), "x").unwrap();
        symlink(root.path(), root.path().join("a/b/back")).unwrap();
        for exclude in [["a/b".to_string()], ["b".to_string()]] {
            let command = build_find_command(root.path().to_str().unwrap(), &[], &exclude);
            let output = std::process::Command::new("sh")
                .args(["-c", &command])
                .output()
                .unwrap();
            assert!(output.status.success(), "{exclude:?}: {output:?}");
            let records = parse_find_records(&output.stdout).unwrap();
            let names: Vec<_> = records
                .iter()
                .filter_map(|record| find_record_name(record, root.path().to_str().unwrap()))
                .map(str::to_string)
                .collect();
            assert_eq!(names, ["a", "a/keep.txt"], "{exclude:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn recursive_scan_reports_a_cycle_instead_of_a_partial_tree() {
        use std::os::unix::fs::symlink;
        let root = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(root.path().join("folder")).unwrap();
        symlink(root.path(), root.path().join("folder/back")).unwrap();
        let command = build_find_command(root.path().to_str().unwrap(), &[], &[]);
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
    fn test_parse_find_record_file() {
        let line = "f\t1024\t1705312800.0\t644\t/var/www/app/index.html\t";
        let node = parse_find_record(&record(line), "/var/www/app", &[])
            .unwrap()
            .unwrap();

        assert_eq!(node.name, "index.html");
        assert!(node.is_file());
        assert_eq!(node.size, Some(1024));
        assert!(node.mtime.is_some());
        assert_eq!(node.permissions, Some(0o644));
    }

    #[test]
    fn test_parse_find_record_directory() {
        let line = "d\t4096\t1705312800.0\t755\t/var/www/app/src\t";
        let node = parse_find_record(&record(line), "/var/www/app", &[])
            .unwrap()
            .unwrap();

        assert_eq!(node.name, "src");
        assert!(node.is_dir());
        assert!(!node.is_loaded());
    }

    #[test]
    fn test_parse_find_record_symlink() {
        let line = "l\t10\t1705312800.0\t777\t/var/www/app/link\t../shared/config";
        let node = parse_find_record(&record(line), "/var/www/app", &[])
            .unwrap()
            .unwrap();

        assert_eq!(node.name, "link");
        assert!(node.is_symlink());
        if let NodeKind::Symlink { ref target } = node.kind {
            assert_eq!(target, "../shared/config");
        }
    }

    #[test]
    fn a_remote_directory_link_can_be_expanded_without_changing_its_link_kind() {
        let line = "l\t10\t1705312800.0\t777\t/var/www/app/linked\t../shared\td";
        let node = parse_find_record(&record(line), "/var/www/app", &[])
            .unwrap()
            .unwrap();
        assert!(node.is_symlink());
        assert!(node.link_is_dir);
        assert!(node.children.is_none());
    }

    #[test]
    fn a_record_whose_size_mode_or_modification_time_cannot_be_read_is_malformed_output() {
        for line in [
            "f\tx\t1705312800.0\t644\t/r/a.txt\t",
            "f\t\t1705312800.0\t644\t/r/a.txt\t",
            "f\t1\t1705312800.0\t9z\t/r/a.txt\t",
            "f\t1\t1705312800.0\t\t/r/a.txt\t",
            "f\t1\tnan\t644\t/r/a.txt\t",
            "f\t1\tinf\t644\t/r/a.txt\t",
            "f\t1\t1e9\t644\t/r/a.txt\t",
            "f\t1\t\t644\t/r/a.txt\t",
        ] {
            assert!(
                parse_find_record(&record(line), "/r", &[]).is_err(),
                "{line:?}"
            );
        }
    }

    #[test]
    fn a_modification_time_before_1970_with_a_fraction_is_read_as_find_printed_it() {
        // find は -1000000.25 秒を、秒を切り下げて正の小数を足した形で出す
        let line = "f\t1\t-1000001.7500000000\t644\t/r/old.txt\t";
        let node = parse_find_record(&record(line), "/r", &[])
            .unwrap()
            .unwrap();
        let mtime = node.mtime.unwrap();
        assert_eq!(
            (mtime.timestamp(), mtime.timestamp_subsec_nanos()),
            (-1_000_001, 750_000_000)
        );
    }

    #[test]
    fn test_parse_find_record_exclude() {
        let line = "d\t4096\t1705312800.0\t755\t/var/www/app/node_modules\t";
        let exclude = vec!["node_modules".to_string()];
        let node = parse_find_record(&record(line), "/var/www/app", &exclude).unwrap();
        assert!(node.is_none());
    }

    #[test]
    fn test_parse_find_record_root_itself() {
        let line = "d\t4096\t1705312800.0\t755\t/var/www/app\t";
        let node = parse_find_record(&record(line), "/var/www/app", &[]).unwrap();
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
        let cmd = build_find_command("/var/www", &[], &[]);
        assert_eq!(cmd.matches("find -L '/var/www' -mindepth 1").count(), 2);
    }

    #[test]
    fn test_build_find_command_include_with_special_chars() {
        // スペースや特殊文字を含むパス → shell_escape で保護
        let include = vec!["my dir/sub".to_string(), "it's here".to_string()];
        let cmd = build_find_command("/var/www", &include, &[]);
        assert!(cmd.contains("'/var/www/my dir/sub'"));
        assert!(cmd.contains("'/var/www/it'\\''s here'"));
    }

    #[test]
    fn test_build_find_command_root_trailing_slash() {
        // root_dir の末尾スラッシュが重複しない
        let include = vec!["src".to_string()];
        let cmd = build_find_command("/var/www/", &include, &[]);
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
    fn creating_a_link_whose_target_starts_with_a_dash_uses_it_as_the_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let link = dir.path().join("link");
        let target = format!("--target-directory={}", sub.display());

        assert!(sh_succeeds(&build_create_symlink_command(
            &target,
            link.to_str().unwrap()
        )));

        assert_eq!(std::fs::read_link(&link).unwrap(), Path::new(&target));
        assert_eq!(std::fs::read_dir(&sub).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn creating_a_link_where_a_link_or_a_file_exists_fails_without_change() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::TempDir::new().unwrap();
        let old_target = dir.path().join("old.txt");
        std::fs::write(&old_target, "old").unwrap();
        let link = dir.path().join("link");
        symlink(&old_target, &link).unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();

        assert!(!sh_succeeds(&build_create_symlink_command(
            "new.txt",
            link.to_str().unwrap()
        )));
        assert!(!sh_succeeds(&build_create_symlink_command(
            "new.txt",
            file.to_str().unwrap()
        )));

        assert_eq!(std::fs::read_link(&link).unwrap(), old_target);
        assert_eq!(std::fs::read_to_string(&old_target).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "x");
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
    fn test_build_mkdir_command_no_parent() {
        // 親がないパスは None
        let cmd = build_mkdir_command("file.rs");
        assert!(cmd.is_none());
    }
    /// 書き込みのテストを実行するログインシェル。入っていないシェルは飛ばす。
    fn installed_login_shells() -> Vec<&'static str> {
        crate::login_shell::LOGIN_SHELLS
            .into_iter()
            .filter(|shell| {
                let installed = std::process::Command::new(shell)
                    .args(["-c", "true"])
                    .status()
                    .is_ok();
                if !installed {
                    crate::login_shell::skip_missing_shell(shell);
                }
                installed
            })
            .collect()
    }

    /// 書き込みのコマンドを、リモートと同じく `sh -c` に包んでログインシェル `shell` から実行する
    /// プロセス
    fn write_process(shell: &str, command: &str) -> std::process::Command {
        let mut process = std::process::Command::new(shell);
        process
            .args(["-c", &crate::login_shell::wrap_for_login_shell(command)])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        process
    }

    /// 書き込みのコマンドを `shell` から実行する。`fake_bin` があれば PATH の先頭に置く。
    fn spawn_write(shell: &str, command: &str, fake_bin: Option<&Path>) -> std::process::Child {
        let mut process = write_process(shell, command);
        if let Some(bin) = fake_bin {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let paths = std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&path));
            process.env("PATH", std::env::join_paths(paths).unwrap());
        }
        process.spawn().unwrap()
    }

    /// 起動した書き込みの標準入力に `input` を渡して、成功したかを返す
    fn finish_write(mut child: std::process::Child, input: &[u8]) -> bool {
        use std::io::Write;
        // 失敗するコマンドが先に終わると書き込みが EPIPE になるが、確かめるのは終了ステータス
        let _ = child.stdin.take().unwrap().write_all(input);
        child.wait().unwrap().success()
    }

    /// 書き込みのコマンドを `shell` から実行し、標準入力に `input` を渡して、成功したかを返す
    fn write_under(shell: &str, command: &str, input: &[u8], fake_bin: Option<&Path>) -> bool {
        finish_write(spawn_write(shell, command, fake_bin), input)
    }

    /// [`write_under`] と同じだが、PATH を `bin` だけにする。fish が利用者の設定で PATH に
    /// ディレクトリを足さないよう、設定の場所も `bin` の中の空のディレクトリにする。
    fn write_with_only(shell: &str, command: &str, input: &[u8], bin: &Path) -> bool {
        let mut process = write_process(shell, command);
        process
            .env("PATH", bin)
            .env("XDG_CONFIG_HOME", bin.join("config"));
        finish_write(process.spawn().unwrap(), input)
    }

    /// 今の PATH で `name` のコマンドが見つかる場所
    fn resolve_command(name: &str) -> Option<String> {
        let output = std::process::Command::new("sh")
            .args(["-c", "command -v \"$1\"", "_", name])
            .output()
            .unwrap();
        let found = String::from_utf8(output.stdout).unwrap();
        (output.status.success() && !found.trim().is_empty()).then(|| found.trim().to_string())
    }

    /// `dir` に `body` を中身とする実行できるスクリプト `name` を置く
    #[cfg(unix)]
    fn put_script(dir: &Path, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// `oflag=nofollow` を受け付けない dd の代わりに、いつも失敗する `dd` を置いたディレクトリ
    #[cfg(unix)]
    fn bin_without_dd_nofollow() -> tempfile::TempDir {
        let bin = tempfile::TempDir::new().unwrap();
        put_script(bin.path(), "dd", "#!/bin/sh\nexit 1\n");
        bin
    }

    /// openssl のないリモートの代わりに、sh・cat・ログインシェルと dd だけを置いたディレクトリ。
    /// `dd_nofollow` が false なら dd は `oflag=nofollow` を受け付けないいつも失敗するものにする。
    #[cfg(unix)]
    fn bin_without_openssl(dd_nofollow: bool) -> tempfile::TempDir {
        let bin = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(bin.path().join("config")).unwrap();
        let mut names = vec!["sh", "cat"];
        names.extend(crate::login_shell::LOGIN_SHELLS);
        if dd_nofollow {
            names.push("dd");
        } else {
            put_script(bin.path(), "dd", "#!/bin/sh\nexit 1\n");
        }
        for name in names {
            let link = bin.path().join(name);
            if let (Some(real), false) = (resolve_command(name), link.exists()) {
                std::os::unix::fs::symlink(real, link).unwrap();
            }
        }
        bin
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

    /// ディレクトリにある項目の名前（並べ替え済み）
    fn entry_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[cfg(unix)]
    #[test]
    fn binary_write_refuses_a_symlink_destination() {
        for shell in installed_login_shells() {
            let dir = dir_with_link_to_real_file();
            let link = dir.path().join("link.txt");

            let written = write_under(
                shell,
                &build_write_bytes_command(link.to_str().unwrap()),
                b"bmV3\n",
                None,
            );

            assert!(!written, "{shell}");
            assert_link_and_target_unchanged(&dir);
        }
    }

    #[cfg(unix)]
    #[test]
    fn text_write_refuses_a_symlink_destination() {
        for shell in installed_login_shells() {
            let dir = dir_with_link_to_real_file();
            let link = dir.path().join("link.txt");

            let written = write_under(
                shell,
                &build_write_text_command(link.to_str().unwrap()),
                b"new",
                None,
            );

            assert!(!written, "{shell}");
            assert_link_and_target_unchanged(&dir);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_write_onto_a_dangling_link_fails_without_creating_its_target() {
        for shell in installed_login_shells() {
            let dir = tempfile::TempDir::new().unwrap();
            let link = dir.path().join("link.txt");
            std::os::unix::fs::symlink("missing.txt", &link).unwrap();

            let written = write_under(
                shell,
                &build_write_text_command(link.to_str().unwrap()),
                b"new",
                None,
            );

            assert!(!written, "{shell}");
            assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("missing.txt"));
            assert_eq!(entry_names(dir.path()), ["link.txt"], "{shell}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_write_keeps_owner_mode_and_hard_links_and_truncates_old_content() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        type BuildWrite = fn(&str) -> String;
        let cases: [(BuildWrite, &[u8]); 2] = [
            (build_write_text_command, b"new"),
            (build_write_bytes_command, b"bmV3\n"),
        ];
        for shell in installed_login_shells() {
            for (build, input) in cases {
                let dir = tempfile::TempDir::new().unwrap();
                let dest = dir.path().join("run.sh");
                std::fs::write(&dest, "much longer old content").unwrap();
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o751)).unwrap();
                let other_name = dir.path().join("other-name.sh");
                std::fs::hard_link(&dest, &other_name).unwrap();
                let before = std::fs::metadata(&dest).unwrap();

                assert!(
                    write_under(shell, &build(dest.to_str().unwrap()), input, None),
                    "{shell}"
                );

                let after = std::fs::metadata(&dest).unwrap();
                assert_eq!(std::fs::read(&dest).unwrap(), b"new", "{shell}");
                assert_eq!(std::fs::read(&other_name).unwrap(), b"new", "{shell}");
                assert_eq!(after.ino(), before.ino(), "{shell}");
                assert_eq!(after.nlink(), 2, "{shell}");
                assert_eq!(after.uid(), before.uid(), "{shell}");
                assert_eq!(after.gid(), before.gid(), "{shell}");
                assert_eq!(after.mode() & 0o7777, 0o751, "{shell}");
            }
        }
    }

    #[test]
    fn writes_create_a_missing_file_and_leave_no_other_entry() {
        for shell in installed_login_shells() {
            let dir = tempfile::TempDir::new().unwrap();
            let text = dir.path().join("missing.txt");
            let binary = dir.path().join("missing.bin");

            assert!(
                write_under(
                    shell,
                    &build_write_text_command(text.to_str().unwrap()),
                    b"new text",
                    None
                ),
                "{shell}"
            );
            assert!(
                write_under(
                    shell,
                    &build_write_bytes_command(binary.to_str().unwrap()),
                    b"AP8K\n",
                    None
                ),
                "{shell}"
            );

            assert_eq!(std::fs::read(&text).unwrap(), b"new text");
            assert_eq!(std::fs::read(&binary).unwrap(), [0x00, 0xff, b'\n']);
            assert_eq!(
                entry_names(dir.path()),
                ["missing.bin", "missing.txt"],
                "{shell}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_swapped_in_during_a_write_keeps_its_target_unchanged() {
        use std::io::Write;
        for shell in installed_login_shells() {
            let dir = tempfile::TempDir::new().unwrap();
            let dest = dir.path().join("dest.txt");
            std::fs::write(&dest, "old").unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let outside_file = outside.path().join("outside.txt");
            std::fs::write(&outside_file, "outside").unwrap();

            let mut child = spawn_write(
                shell,
                &build_write_text_command(dest.to_str().unwrap()),
                None,
            );
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(b"ne").unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::fs::read(&dest).unwrap() != b"ne" {
                assert!(
                    std::time::Instant::now() < deadline,
                    "{shell}: the write did not start"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::fs::remove_file(&dest).unwrap();
            std::os::unix::fs::symlink(&outside_file, &dest).unwrap();
            stdin.write_all(b"w").unwrap();
            drop(stdin);
            child.wait().unwrap();

            assert_eq!(
                std::fs::read_to_string(&outside_file).unwrap(),
                "outside",
                "{shell}"
            );
            assert_eq!(std::fs::read_link(&dest).unwrap(), outside_file);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_write_onto_a_directory_fails_without_change() {
        for shell in installed_login_shells() {
            let dir = tempfile::TempDir::new().unwrap();
            let dest = dir.path().join("folder");
            std::fs::create_dir(&dest).unwrap();

            assert!(
                !write_under(
                    shell,
                    &build_write_text_command(dest.to_str().unwrap()),
                    b"new",
                    None
                ),
                "{shell}"
            );

            assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);
            assert_eq!(entry_names(dir.path()), ["folder"], "{shell}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn without_dd_nofollow_a_link_destination_is_refused_and_a_file_is_written() {
        use std::os::unix::fs::MetadataExt;
        let bin = bin_without_dd_nofollow();
        for shell in installed_login_shells() {
            let dir = dir_with_link_to_real_file();
            let link = dir.path().join("link.txt");
            let dest = dir.path().join("dest.txt");
            std::fs::write(&dest, "much longer old content").unwrap();
            std::fs::hard_link(&dest, dir.path().join("other-name.txt")).unwrap();

            let refused = !write_under(
                shell,
                &build_write_bytes_command(link.to_str().unwrap()),
                b"bmV3\n",
                Some(bin.path()),
            );
            let written = write_under(
                shell,
                &build_write_text_command(dest.to_str().unwrap()),
                b"new",
                Some(bin.path()),
            );

            assert!(refused, "{shell}");
            assert_link_and_target_unchanged(&dir);
            assert!(written, "{shell}");
            assert_eq!(std::fs::read(&dest).unwrap(), b"new", "{shell}");
            assert_eq!(std::fs::metadata(&dest).unwrap().nlink(), 2, "{shell}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_decoder_that_fails_after_writing_part_fails_the_write() {
        // デコードの途中で失敗する openssl の代わりに、少し書いてから失敗する `openssl`
        let bin = tempfile::TempDir::new().unwrap();
        put_script(
            bin.path(),
            "openssl",
            "#!/bin/sh\ncat >/dev/null\nprintf partial\nexit 1\n",
        );
        for shell in installed_login_shells() {
            let dir = tempfile::TempDir::new().unwrap();
            let dest = dir.path().join("dest.bin");
            std::fs::write(&dest, "original").unwrap();

            let written = write_under(
                shell,
                &build_write_bytes_command(dest.to_str().unwrap()),
                b"bmV3\n",
                Some(bin.path()),
            );

            assert!(!written, "{shell}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_write_without_openssl_fails_and_keeps_the_destination() {
        for dd_nofollow in [true, false] {
            let bin = bin_without_openssl(dd_nofollow);
            for shell in installed_login_shells() {
                let dir = tempfile::TempDir::new().unwrap();
                let dest = dir.path().join("dest.bin");
                let text = dir.path().join("text.txt");
                std::fs::write(&dest, "original").unwrap();

                let written = write_with_only(
                    shell,
                    &build_write_bytes_command(dest.to_str().unwrap()),
                    b"bmV3\n",
                    bin.path(),
                );
                // 同じ PATH でテキストは書けるので、失敗の原因は openssl がないこと
                let text_written = write_with_only(
                    shell,
                    &build_write_text_command(text.to_str().unwrap()),
                    b"new",
                    bin.path(),
                );

                let case = format!("{shell}, dd_nofollow={dd_nofollow}");
                assert!(!written, "{case}");
                assert_eq!(std::fs::read(&dest).unwrap(), b"original", "{case}");
                assert!(text_written, "{case}");
                assert_eq!(std::fs::read(&text).unwrap(), b"new", "{case}");
            }
        }
    }

    /// `status=none` を受け付けない古い dd（CentOS 5 の coreutils 5.97 など）の代わりに、それを
    /// 拒み、他は実際の dd に渡して引数を `dd.log` に残す `dd` を置いたディレクトリ
    #[cfg(unix)]
    fn bin_with_dd_lacking_status_none() -> tempfile::TempDir {
        let real_dd = resolve_command("dd").unwrap();
        let bin = tempfile::TempDir::new().unwrap();
        let log = bin.path().join("dd.log");
        put_script(
            bin.path(),
            "dd",
            &format!(
                "#!/bin/sh\nfor a do case $a in status=*) exit 1;; esac; done\n\
                 printf '%s\\n' \"$*\" >>'{}'\nexec {} \"$@\"\n",
                log.display(),
                real_dd
            ),
        );
        bin
    }

    #[cfg(unix)]
    #[test]
    fn a_dd_without_status_none_still_opens_the_destination_without_following_a_link() {
        let bin = bin_with_dd_lacking_status_none();
        for shell in installed_login_shells() {
            let dir = dir_with_link_to_real_file();
            let link = dir.path().join("link.txt");
            let dest = dir.path().join("dest.txt");
            std::fs::write(&dest, "much longer old content").unwrap();
            let _ = std::fs::remove_file(bin.path().join("dd.log"));

            let refused = !write_under(
                shell,
                &build_write_text_command(link.to_str().unwrap()),
                b"new",
                Some(bin.path()),
            );
            let written = write_under(
                shell,
                &build_write_text_command(dest.to_str().unwrap()),
                b"new",
                Some(bin.path()),
            );

            assert!(refused, "{shell}");
            assert_link_and_target_unchanged(&dir);
            assert!(written, "{shell}");
            assert_eq!(std::fs::read(&dest).unwrap(), b"new", "{shell}");
            let log = std::fs::read_to_string(bin.path().join("dd.log")).unwrap_or_default();
            let opened_with_nofollow = |path: &Path| {
                log.lines().any(|line| {
                    line.contains(&format!("of={}", path.display()))
                        && line.contains("oflag=nofollow")
                })
            };
            assert!(opened_with_nofollow(&link), "{shell}: {log}");
            assert!(opened_with_nofollow(&dest), "{shell}: {log}");
        }
    }
}
