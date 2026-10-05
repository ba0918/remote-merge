use std::path::PathBuf;

use super::core::CoreRuntime;
use super::target_io::TargetPath;

/// 検査のスクリプトが使う関数。ローカルの検査（`inspect_local_path`）と同じ規則で、
/// lstat が ENOENT で失敗したパスだけを missing にし、それ以外の失敗は終了コード 1 にする。
///
/// - `resolve_existing_prefix PATH`: PATH の祖先のうち lstat できる最も近いものを辿った実パスに、
///   その下の存在しない要素（PATH の末尾の要素は除く）を足して `r` に入れる。その祖先が辿った先で
///   検索できるディレクトリでなければ、PATH の lstat は ENOENT ではないので失敗する
/// - `canonical_is_missing PATH`: PATH の実パスへの解決が ENOENT で失敗するときだけ成功する。
///   symlink を辿る回数はカーネルと同じ 40 回までで、超えれば ELOOP として失敗する
/// - `parent_of`・`base_of`: 末尾の "/" を除いた親（`s`）と末尾の要素（`b`）
/// - `name_fits DIR NAME`: DIR の下で NAME が NAME_MAX に収まるか。カーネルは存在するディレクトリの
///   すぐ下の要素が長すぎると ENOENT ではなく ENAMETOOLONG で失敗するので、その要素だけを測る。
///   長さはバイトで数えるため `LC_ALL=C` にし、`getconf` が答えなければ 255 とする
/// - `path_fits PATH`: PATH 全体が PATH_MAX に収まるか。カーネルは終端の NUL を含めて PATH_MAX を
///   超えるパスを ENOENT ではなく ENAMETOOLONG で拒むので、バイトで数えて PATH_MAX 未満かを見る。
///   ローカルの検査が最初に lstat するパスと同じものを測る。存在しないパスはそのパス、
///   ぶら下がったリンク先は `resolve_existing_prefix` が最初に辿る親。`getconf` が答えなければ 4096
/// - 存在しない部分の ".." は、ローカルと同じく辿る先が決まらないものとして失敗にする
/// - `link_of`・`real_of`: `readlink` と `readlink -f` の答え（`t`・`r`）。`$(...)` は末尾の改行を
///   すべて削るので、番兵の `x` を足してから外し、`readlink` が足した改行を一つだけ削る。
///   改行で終わる名前やリンク先も、ローカルと同じパスとして読む
///
/// 試験 SSH サーバは先頭の `resolve_existing_prefix()` と、後に続く `; p=<パス>; if [` で
/// この検査を見分けるため、その並びを崩さない。
const INSPECT_PATH_FUNCTIONS: &str = r#"resolve_existing_prefix() {
  q=$1; suffix=; base_of "$q"; first=$b; parent_of "$q"; path_fits "$s" || return 1;
  while :; do
    parent_of "$q"; [ "$s" != "$q" ] || return 1; q=$s;
    if [ -e "$q" ] || [ -L "$q" ]; then break; fi;
    base_of "$q"; [ "$b" != .. ] || return 1; suffix=/$b$suffix; first=$b;
  done;
  [ -d "$q" ] && [ -x "$q" ] && name_fits "$q" "$first" || return 1;
  real_of "$q" || return 1;
  case $r in /) r=${suffix:-/} ;; *) r=$r$suffix ;; esac;
};
link_of() {
  t=$(readlink -- "$1" && echo x) || return 1; t=${t%x}; t=${t%?};
};
real_of() {
  r=$(readlink -f -- "$1" && echo x) || return 1; r=${r%x}; r=${r%?};
};
path_fits() {
  LC_ALL=C; m=$(getconf PATH_MAX / 2>/dev/null) || m=4096;
  case $m in ''|*[!0-9]*) m=4096 ;; esac;
  [ "${#1}" -lt "$m" ];
};
name_fits() {
  LC_ALL=C; m=$(getconf NAME_MAX "$1" 2>/dev/null) || m=255;
  case $m in ''|*[!0-9]*) m=255 ;; esac;
  [ "${#2}" -le "$m" ];
};
parent_of() {
  s=$1;
  while [ "${s%/}" != "$s" ] && [ -n "${s%/}" ]; do s=${s%/}; done;
  case $s in */*) s=${s%/*}; [ -n "$s" ] || s=/ ;; *) s=. ;; esac;
};
base_of() {
  b=$1;
  while [ "${b%/}" != "$b" ] && [ -n "${b%/}" ]; do b=${b%/}; done;
  b=${b##*/};
};
canonical_is_missing() {
  x=$1; n=0;
  while [ "$n" -lt 40 ]; do
    n=$((n + 1)); y=$x;
    while ! { [ -e "$y" ] || [ -L "$y" ]; }; do base_of "$y"; first=$b; parent_of "$y"; [ "$s" != "$y" ] || return 1; y=$s; done;
    if [ "$y" != "$x" ] && [ -d "$y" ] && [ -x "$y" ]; then name_fits "$y" "$first"; return; fi;
    [ -L "$y" ] && ! [ -e "$y" ] || return 1;
    link_of "$y" || return 1;
    case $t in /*) ;; *) parent_of "$y"; t=$s/$t ;; esac;
    x=$t${x#"$y"};
  done;
  return 1;
}"#;

/// 本体。`p` の lstat の結果で分け、symlink はローカルの `resolved_symlink` と同じくリンク先を
/// 解決し、解決が ENOENT で失敗したときだけ存在する祖先からの形にする。答えは種類とパスを
/// それぞれ NUL で終えて出す。パスには改行を含められるが NUL は含められないため
const INSPECT_PATH_BODY: &str = r#"if [ -L "$p" ]; then
  link_of "$p" || exit 1; target=$t;
  case $target in /*) c=$target ;; *) parent_of "$p"; c=$s/$target ;; esac;
  if [ -e "$c" ]; then real_of "$c" || exit 1; resolved=$r;
  else
    canonical_is_missing "$c" || exit 1;
    base_of "$c"; case $b in ''|..) exit 1 ;; esac; name=$b;
    resolve_existing_prefix "$c" || exit 1;
    case $r in /) resolved=/$name ;; *) resolved=$r/$name ;; esac;
  fi;
  printf 'symlink\000%s\000%s\000' "$target" "$resolved";
elif [ -e "$p" ]; then real_of "$p" || exit 1; printf 'file\000%s\000' "$r";
else path_fits "$p" && resolve_existing_prefix "$p" || exit 1; printf 'missing\000%s\000' "$r";
fi"#;

/// 一行のスクリプトにする。各行は `;` かキーワードで終わるので、改行を空白に置き換えてよい
fn one_line(script: &str) -> String {
    script.lines().map(str::trim).collect::<Vec<_>>().join(" ")
}

pub(crate) fn build_inspect_path_command(path: &str) -> String {
    let path = crate::ssh::tree_parser::shell_escape(path);
    format!(
        "{}; p={path}; {}",
        one_line(INSPECT_PATH_FUNCTIONS),
        one_line(INSPECT_PATH_BODY)
    )
}

/// 辿った先が通常ファイルかを問うコマンド。失敗を終了コードにしないよう、答えを出力で返す
pub(crate) fn build_is_regular_file_command(path: &str) -> String {
    let path = crate::ssh::tree_parser::shell_escape(path);
    format!("if [ -f {path} ]; then echo regular; else echo other; fi")
}

pub(crate) fn parse_is_regular_file_output(output: &str) -> anyhow::Result<bool> {
    match output.trim_end_matches(['\r', '\n']) {
        "regular" => Ok(true),
        "other" => Ok(false),
        other => anyhow::bail!("unknown file kind output: {other}"),
    }
}

impl CoreRuntime {
    /// SSH の経路で、root_dir からの `rel_path` が辿った先で通常ファイルかを問う
    pub(crate) fn remote_is_regular_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> anyhow::Result<bool> {
        let root = self
            .get_server_config(server_name)?
            .root_dir
            .to_string_lossy()
            .to_string();
        let full_path = crate::merge::executor::validate_remote_path(&root, rel_path)?;
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {server_name}"))?;
        let output = self.rt.block_on(crate::ssh::exec_strict(
            client,
            &build_is_regular_file_command(&full_path),
        ))?;
        parse_is_regular_file_output(&output)
    }
}

/// 検査の答えを読む。答えは種類と一つか二つのパスで、どれも NUL で終わる
pub(crate) fn parse_inspect_path_output(output: &str) -> anyhow::Result<TargetPath> {
    let fields = output
        .strip_suffix('\0')
        .ok_or_else(|| anyhow::anyhow!("invalid path inspection output"))?
        .split('\0')
        .collect::<Vec<_>>();
    if fields[1..].iter().any(|field| field.is_empty()) {
        anyhow::bail!("path inspection returned an empty path");
    }
    match fields.as_slice() {
        ["file", real_path] => Ok(TargetPath::File {
            real_path: PathBuf::from(real_path),
        }),
        ["symlink", link_target, real_path] => Ok(TargetPath::Symlink {
            link_target: PathBuf::from(link_target),
            real_path: PathBuf::from(real_path),
        }),
        ["missing", real_parent] => Ok(TargetPath::Missing {
            real_parent: PathBuf::from(real_parent),
        }),
        [kind, ..] => anyhow::bail!("unknown path inspection output: {kind}"),
        [] => anyhow::bail!("invalid path inspection output"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(unix)]
    use std::process::Command;

    #[test]
    fn command_quotes_the_remote_path() {
        let command = build_inspect_path_command("/var/www/a file's.txt");
        assert!(command.contains("'/var/www/a file'\\''s.txt'"));
        assert!(command.contains("readlink -f"));
    }

    #[test]
    fn parser_returns_a_file_real_path() {
        assert_eq!(
            parse_inspect_path_output("file\0/mnt/shared/file.txt\0").unwrap(),
            TargetPath::File {
                real_path: PathBuf::from("/mnt/shared/file.txt")
            }
        );
    }

    #[test]
    fn parser_returns_a_symlink_target_and_its_resolved_path() {
        assert_eq!(
            parse_inspect_path_output("symlink\0../shared/file.txt\0/mnt/shared/file.txt\0")
                .unwrap(),
            TargetPath::Symlink {
                link_target: PathBuf::from("../shared/file.txt"),
                real_path: PathBuf::from("/mnt/shared/file.txt")
            }
        );
    }

    /// 検査のスクリプトを `sh` で実行し、出力を読んだ結果を返す。終了コードが 0 以外ならエラー
    #[cfg(unix)]
    fn inspect_with_script(path: &std::path::Path) -> anyhow::Result<TargetPath> {
        let output = Command::new("sh")
            .arg("-c")
            .arg(build_inspect_path_command(path.to_str().unwrap()))
            .output()?;
        if !output.status.success() {
            anyhow::bail!("exit status {}", output.status);
        }
        parse_inspect_path_output(&String::from_utf8(output.stdout)?)
    }

    /// root につなげたパス全体が `total` バイトになる、存在しない要素だけの相対パス。
    /// 各要素は NAME_MAX に収まる
    #[cfg(unix)]
    fn missing_rel_path_of_length(root: &std::path::Path, total: usize) -> String {
        let mut remaining = total - root.as_os_str().len() - 1;
        let mut rel = String::new();
        while remaining > 201 {
            rel.push_str(&"d".repeat(200));
            rel.push('/');
            remaining -= 201;
        }
        rel.push_str(&"f".repeat(remaining));
        rel
    }

    #[cfg(unix)]
    #[test]
    fn script_and_local_inspection_agree_on_every_path_state() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::create_dir(root.join("dir")).unwrap();
        std::fs::write(root.join("file.txt"), "file\n").unwrap();
        std::fs::create_dir(root.join("locked")).unwrap();
        std::fs::write(root.join("locked/inner.txt"), "inner\n").unwrap();
        symlink("self", root.join("self")).unwrap();
        symlink("missing/file.txt", root.join("dangling")).unwrap();
        symlink("dangling", root.join("chain")).unwrap();
        symlink("nowhere", root.join("broken")).unwrap();
        symlink("loop", root.join("loop")).unwrap();
        symlink("dir", root.join("linkdir")).unwrap();
        symlink("file.txt", root.join("linkfile")).unwrap();
        // リンク先の末尾や途中の改行は、リンク先の名前の一部として読む
        std::fs::write(root.join("newline\n"), "newline\n").unwrap();
        symlink("file.txt\n", root.join("trailing_newline")).unwrap();
        symlink("newline\n", root.join("existing_newline")).unwrap();
        symlink("dir\n\n", root.join("two_newlines")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_root = std::fs::canonicalize(outside.path()).unwrap();
        let escape = format!("{}/x\n{}/file.txt", outside_root.display(), root.display());
        symlink(&escape, root.join("escape")).unwrap();
        // 存在しない部分の "." と ".."、NAME_MAX を超える名前は、ローカルの解決と同じに扱う
        let long = "n".repeat(256);
        symlink("missing/./file.txt", root.join("dot_inside")).unwrap();
        symlink("missing/.", root.join("dot_end")).unwrap();
        symlink("./missing", root.join("dot_start")).unwrap();
        symlink("missing/../file.txt", root.join("dotdot_missing")).unwrap();
        symlink("dir/../missing", root.join("dotdot_existing")).unwrap();
        symlink("file.txt/.", root.join("dot_under_file")).unwrap();
        symlink(&long, root.join("long_link")).unwrap();
        symlink("long_link", root.join("long_chain")).unwrap();
        symlink(format!("missing/{long}"), root.join("long_below_missing")).unwrap();
        std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o600))
            .unwrap();
        // 権限を無視できる実効ユーザーでは検索権限のない状態を作れない
        let can_lock = std::fs::symlink_metadata(root.join("locked/inner.txt")).is_err();

        let mut cases = vec![
            "file.txt",
            "dir",
            "dir/missing.txt",
            "nodir/sub/missing.txt",
            "file.txt/below.txt",
            "self",
            "dangling",
            "chain",
            "broken/below.txt",
            "loop/below.txt",
            "linkdir/missing.txt",
            "linkfile",
            "trailing_newline",
            "existing_newline",
            "two_newlines",
            "escape",
            "newline\n",
            "missing\n/below.txt",
            "dot_inside",
            "dot_end",
            "dot_start",
            "dotdot_missing",
            "dotdot_existing",
            "dot_under_file",
            "long_link",
            "long_chain",
            "long_below_missing",
        ];
        let long_paths = [
            long.clone(),
            format!("{long}/below.txt"),
            format!("nodir/{long}"),
        ];
        cases.extend(long_paths.iter().map(String::as_str));
        // パス全体の長さはカーネルが NUL を含めて PATH_MAX（4096 バイト）まで受け付ける。
        // 検査するパス全体と、ぶら下がったリンク先の親の長さを、その境界の両側で比べる
        let mut path_max_paths = Vec::new();
        for total in [4095, 4096] {
            path_max_paths.push(missing_rel_path_of_length(root, total));
            let name = format!("long_target_{total}");
            let target = format!("{}/file.txt", missing_rel_path_of_length(root, total));
            symlink(&target, root.join(&name)).unwrap();
            path_max_paths.push(name);
        }
        cases.extend(path_max_paths.iter().map(String::as_str));
        if can_lock {
            cases.push("locked/inner.txt");
        }
        let mismatches: Vec<String> = cases
            .into_iter()
            .filter_map(|rel| {
                let local = remote_merge_engine::local_io::inspect_local_path(root, rel);
                let script = inspect_with_script(&root.join(rel));
                match (&local, &script) {
                    (Ok(local), Ok(script)) if local == script => None,
                    (Err(_), Err(_)) => None,
                    _ => Some(format!("{rel}: local {local:?}, script {script:?}")),
                }
            })
            .collect();
        std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
        assert!(mismatches.is_empty(), "{mismatches:#?}");
        // 改行を含むリンク先が root の外なら、SSH の検査でも root の外のまま
        match inspect_with_script(&root.join("escape")).unwrap() {
            TargetPath::Symlink { real_path, .. } => assert!(
                real_path.starts_with(&outside_root) && !real_path.starts_with(root),
                "{real_path:?}"
            ),
            other => panic!("{other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn command_resolves_a_dangling_terminal_symlink_to_the_path_it_names() {
        let directory = tempfile::tempdir().unwrap();
        let link = directory.path().join("current");
        symlink("missing/file.txt", &link).unwrap();

        let output = Command::new("sh")
            .arg("-c")
            .arg(build_inspect_path_command(link.to_str().unwrap()))
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "path inspection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            parse_inspect_path_output(&String::from_utf8(output.stdout).unwrap()).unwrap(),
            TargetPath::Symlink {
                link_target: PathBuf::from("missing/file.txt"),
                real_path: directory.path().join("missing/file.txt"),
            }
        );
    }
}
