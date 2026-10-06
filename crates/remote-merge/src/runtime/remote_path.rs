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
/// - `kernel_form PATH`: カーネルが辿ると PATH と同じ結果になる短い形（`k`）。途中の "." と続く "/" を除き、
///   末尾の "/" と "/." はディレクトリを求める印として "/." にそろえる。それでも PATH_MAX に収まらなければ、
///   収まる最も長い祖先がディレクトリのときだけ、その祖先を辿った実パスに置き換える。ローカルの
///   `canonicalize`（realpath）は要素ごとに辿るので、つないだ文字列が PATH_MAX を超えても解決できる。
///   存在するかの判定とその解決はこの形で行う。".." は symlink の先で意味が変わるので文字列では畳まない
/// - `rust_parent PATH`: Rust の `Path::parent()` と同じ親（`s`）。末尾の "/" と "." の要素を除いてから
///   末尾の要素を除き、残りの末尾の "/" と "." の要素も除く。途中の "." と続く "/" は残す。
///   ローカルは解決が ENOENT で失敗すると、この形の文字列をそのまま lstat し、さらにその親へ上る
/// - `drop_trailing_dots PATH`: PATH の末尾の "/" と "/." を除いた形（`d`）。存在しない部分の "." は
///   ローカルの解決と同じく答えのパスに残さない。末尾の "." はここで、途中の "." は `rust_parent` で除く
/// - `parent_of`・`base_of`: 末尾の "/" を除いた親（`s`）と末尾の要素（`b`）。`parent_of` は "." も
///   一つの要素として扱い、カーネルが一つずつ辿る順に親へ上る（`canonical_is_missing` が使う）
/// - `name_fits DIR NAME`: DIR の下で NAME が NAME_MAX に収まるか。カーネルは存在するディレクトリの
///   すぐ下の要素が長すぎると ENOENT ではなく ENAMETOOLONG で失敗するので、その要素だけを測る。
///   長さはバイトで数えるため `LC_ALL=C` にし、`getconf` が答えなければ 255 とする
/// - `path_fits PATH`: PATH の文字列が PATH_MAX に収まるか。カーネルは終端の NUL を含めて PATH_MAX を
///   超えるパスを ENOENT ではなく ENAMETOOLONG で拒むので、バイトで数えて PATH_MAX 未満かを見る。
///   測るのはローカルの検査が lstat する文字列で、存在しないパスはそのパス、ぶら下がったリンク先は
///   つないだリンク先の `rust_parent`。`getconf` が答えなければ 4096 とし、一度求めた値を使い回す
/// - 存在しない部分の ".." は、ローカルと同じく辿る先が決まらないものとして失敗にする
/// - `link_of`・`real_of`: `readlink` と `readlink -f` の答え（`t`・`r`）。`$(...)` は末尾の改行を
///   すべて削るので、番兵の `x` を足してから外し、`readlink` が足した改行を一つだけ削る。
///   改行で終わる名前やリンク先も、ローカルと同じパスとして読む
///
/// 試験 SSH サーバは先頭の `resolve_existing_prefix()` と、後に続く `; p=<パス>; if [` で
/// この検査を見分けるため、その並びを崩さない。
const INSPECT_PATH_FUNCTIONS: &str = r#"resolve_existing_prefix() {
  drop_trailing_dots "$1"; q=$d; suffix=; base_of "$q"; first=$b; rust_parent "$q"; path_fits "$s" || return 1;
  while :; do
    rust_parent "$q"; [ "$s" != "$q" ] || return 1; q=$s;
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
kernel_form() {
  rest=$1; k=; tail=;
  case $rest in /*) k=/ ;; esac;
  case $rest in */ | */. | .) tail=. ;; esac;
  while [ -n "$rest" ]; do
    case $rest in */*) part=${rest%%/*}; rest=${rest#*/} ;; *) part=$rest; rest= ;; esac;
    case $part in ''|.) ;; *) case $k in ''|*/) k=$k$part ;; *) k=$k/$part ;; esac ;; esac;
  done;
  if [ -n "$tail" ]; then case $k in ''|*/) k=$k. ;; *) k=$k/. ;; esac; fi;
  [ -n "$k" ] || k=.;
  while ! path_fits "$k"; do
    head=$k; while ! path_fits "$head"; do case $head in */*) head=${head%/*} ;; *) break ;; esac; done;
    [ -n "$head" ] && [ -d "$head" ] && real_of "$head" && [ "${#r}" -lt "${#head}" ] || break;
    case $r in /) k=${k#"$head"} ;; *) k=$r${k#"$head"} ;; esac;
  done;
};
rust_parent() {
  drop_trailing_dots "$1"; parent_of "$d"; drop_trailing_dots "$s"; s=$d;
};
drop_trailing_dots() {
  d=$1;
  while :; do case $d in ?*/.) d=${d%/.} ;; ?*/) d=${d%/} ;; *) break ;; esac; done;
};
path_fits() {
  LC_ALL=C;
  if [ -z "$path_max" ]; then
    path_max=$(getconf PATH_MAX / 2>/dev/null) || path_max=4096;
    case $path_max in ''|*[!0-9]*) path_max=4096 ;; esac;
  fi;
  [ "${#1}" -lt "$path_max" ];
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
};
path_max="#;

/// 本体。`p` の lstat の結果で分け、symlink はローカルの `resolved_symlink` と同じくリンク先を
/// 解決し、解決が ENOENT で失敗したときだけ存在する祖先からの形にする。答えは種類とパスを
/// それぞれ NUL で終えて出す。パスには改行を含められるが NUL は含められないため
const INSPECT_PATH_BODY: &str = r#"if [ -L "$p" ]; then
  link_of "$p" || exit 1; target=$t;
  case $target in /*) c=$target ;; *) parent_of "$p"; c=$s/$target ;; esac;
  kernel_form "$c";
  if [ -e "$k" ]; then real_of "$k" || exit 1; resolved=$r;
  else
    canonical_is_missing "$k" || exit 1;
    drop_trailing_dots "$c"; c=$d; base_of "$c"; case $b in ''|..) exit 1 ;; esac; name=$b;
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
