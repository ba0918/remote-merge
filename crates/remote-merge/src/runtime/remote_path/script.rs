//! リモートで "/" から要素ごとに辿るスクリプト。手順は core の `resolve_path` と同じで、
//! 基本操作は `[ -L ]`・`[ -d ]`・`[ -e ]`・`[ -f ]`、組み込みの cd・pwd・printf・command -v と
//! オプションなしの readlink だけを使う。要素を分けて積むのには、シェルの言語の機能（`set -f`・
//! `set --`・shift・eval・IFS の代入と unset）を使う（決定記録の A55）。
//!
//! 状態は core の手順と同じものを持つ。まだ辿っていない要素は、差し込んだリンク先ごとの残りを
//! 積んだものとして持ち、要素を一つ取り出すたびに写すのはいちばん上の残りだけにする。
//! - 位置パラメータ: いちばん上の残りを "/" で分けた欄。空の欄と "." は取り出すときに読み飛ばす。
//!   `tsl` は、その最後の要素の後に "/" があるか
//! - `s1`〜`sK` と `t1`〜`tK`: その下に積んだ残りを "/" でつないだものと、その `tsl`。`k` は積んだ数
//! - `n`: 読み飛ばすものを除いた、まだ辿っていない要素の数
//! - `ex`: 辿っている途中のリンク先。差し込んだときに後ろに残っていた要素の数と、差し込んだ
//!   symlink が確認する相対パスの要素だったか（1 か 0）を ":" でつないだものを空白で並べる。
//!   残っていた要素の数は差し込んだ順に小さくならないので、辿り終えたものは末尾から除く
//! - `nrel`: まだ辿っていない要素のうち、後ろにある確認する相対パスの要素の数
//!
//! 答えは欄ごとに NUL で終え、辿り始める絶対パスの root_dir を `root` の欄で、読んだリンク文字列を
//! `link` の欄で順に出してから、結果か `error` と原因の文言を出す。辿れなかったことも終了コード 0 で返し、0 以外はスクリプトの失敗にする。
//! 検索権限は `[ -x ]` ではなく cd の成否で判定する。`[ -x ]` は BusyBox ash が root のモード 000 の
//! ディレクトリを検索できないと答え、実効 uid の access と食い違う。
//! コマンド置換は出力の末尾の改行をすべて落とすので、pwd と readlink の出力には x を続けてから
//! x とコマンドが足した改行を一つずつ取り除き、名前の末尾の改行を残す

/// コマンドの先頭の並び。試験 SSH サーバは同じ並びを別に持ち、これでパスの確認を見分けて `r=` と `p=` を読む
const INSPECT_PATH_PREFIX: &str = "LC_ALL=C; r=";

const SCRIPT: &str = r#"emit() { printf '%s\000' "$@"; };
fail() { emit error "$1"; exit 0; };
count() { nc=0; for f in "$@"; do case $f in ''|.) ;; *) nc=$((nc + 1)) ;; esac; done; };
set -f;
case $r in /*) ;; *) d=$(pwd -P && printf x) || fail "cannot read"; d=${d%x}; d=${d%?}; r=$d/$r ;; esac;
emit root "$r";
k=0;
IFS=/; set -- $p; unset IFS; count "$@"; nrel=$nc;
if [ "$nrel" -gt 0 ]; then k=1; IFS=/; s1="$*"; unset IFS; case $p in */) t1=1 ;; *) t1= ;; esac; fi;
IFS=/; set -- $r; unset IFS; count "$@"; n=$((nc + nrel));
case $r in */) tsl=1 ;; *) tsl= ;; esac; if [ "$nrel" -gt 0 ]; then tsl=1; fi;
real=/; ex=; tl=; tlset=; mp=; nl=0;
while [ "$n" -gt 0 ]; do
  while :; do
    while [ "$#" -eq 0 ]; do
      if [ "$k" -eq 0 ]; then exit 1; fi;
      IFS=/; eval "set -- \$s$k"; unset IFS; eval "tsl=\$t$k"; k=$((k - 1));
    done;
    c=$1; shift;
    case $c in ''|.) ;; *) break ;; esac;
  done;
  if [ "$#" -gt 0 ] || [ -n "$tsl" ]; then sl=1; else sl=; fi;
  n=$((n - 1)); rest=$n;
  while [ -n "$ex" ]; do e=${ex##* }; if [ "${e%:*}" -le "$rest" ]; then break; fi; ex=${ex% *}; done;
  if [ -z "$ex" ] && [ "$rest" -lt "$nrel" ]; then fr=1; nrel=$rest; else fr=0; fi;
  if [ "$c" = .. ]; then
    if [ -n "$mp" ]; then fail "parent reference in missing part"; fi;
    case $real in /*/*) real=${real%/*} ;; *) real=/ ;; esac;
    continue;
  fi;
  case $real in /) next=/$c ;; *) next=$real/$c ;; esac;
  if [ "${#c}" -gt 255 ] || [ "${#next}" -ge 4096 ]; then fail "path too long"; fi;
  if [ -n "$mp" ]; then real=$next; continue; fi;
  cd -- "$real" 2>/dev/null || fail "permission denied";
  if [ -L "$next" ]; then
    if [ "$nl" -eq 40 ]; then fail "too many levels of symbolic links"; fi;
    nl=$((nl + 1));
    command -v readlink >/dev/null 2>&1 || fail "readlink not available";
    t=$(readlink -- "$next" && printf x) || fail "cannot read";
    t=${t%x}; t=${t%?};
    emit link "$t";
    if [ "$fr" = 1 ] && [ "$rest" -eq 0 ]; then tl=$t; tlset=1; fi;
    case $t in /*) real=/ ;; esac;
    ex="$ex $rest:$fr";
    case $t in */) ttr=1 ;; *) ttr= ;; esac; if [ "$n" -gt 0 ] || [ -n "$sl" ]; then ttr=1; fi;
    if [ "$#" -gt 0 ]; then k=$((k + 1)); IFS=/; eval "s$k=\"\$*\""; unset IFS; eval "t$k=\$tsl"; fi;
    IFS=/; set -- $t; unset IFS; count "$@"; n=$((n + nc)); tsl=$ttr;
  elif [ -d "$next" ]; then real=$next;
  elif [ -e "$next" ]; then
    if [ -f "$next" ]; then kind=regular; else kind=other; fi;
    if [ "$rest" -gt 0 ]; then
      origin=$fr;
      if [ -n "$ex" ]; then
        f=${ex# }; f=${f%% *};
        if [ "${f%:*}" -lt "$rest" ]; then fail "not a directory"; fi;
        origin=${f#*:};
      fi;
      if [ "$origin" != 1 ]; then fail "not a directory"; fi;
      emit notdir "$next" "$kind"; exit 0;
    fi;
    if [ -n "$sl" ]; then fail "not a directory"; fi;
    real=$next;
  else
    if [ -n "$ex" ]; then f=${ex##* }; if [ "${f%:*}" -gt 0 ]; then fail "dangling symbolic link"; fi; fi;
    if [ "$rest" -eq 0 ]; then mp=1; else mp=0; fi;
    real=$next;
  fi;
done;
if [ -n "$tlset" ]; then emit symlink "$tl" "$real";
elif [ -n "$mp" ]; then case $real in /*/*) par=${real%/*} ;; *) par=/ ;; esac; emit missing "$par" "$mp";
else emit exists "$real"; fi"#;

/// 一行のスクリプトにする。各行は `;` かキーワードで終わるので、改行を空白に置き換えてよい
fn one_line(script: &str) -> String {
    script.lines().map(str::trim).collect::<Vec<_>>().join(" ")
}

/// root_dir `root` の下の `rel` を辿るコマンド。長さをバイトで数えるよう、先頭で LC_ALL=C を代入する
/// （BusyBox ash は外から渡した UTF-8 のロケールでは `${#var}` を文字数で数える）
pub fn build_inspect_path_command(root: &str, rel: &str) -> String {
    let root = crate::ssh::tree_parser::shell_escape(root);
    let rel = crate::ssh::tree_parser::shell_escape(rel);
    format!("{INSPECT_PATH_PREFIX}{root}; p={rel}; {}", one_line(SCRIPT))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 長さをバイトで数えるよう、ほかの何よりも先に LC_ALL=C を代入する（決定記録の A49）
    #[test]
    fn the_command_assigns_the_c_locale_before_anything_else() {
        assert!(build_inspect_path_command("/srv/app", "a.txt").starts_with("LC_ALL=C;"));
    }

    #[cfg(unix)]
    #[test]
    fn a_relative_root_dir_is_resolved_from_the_working_directory() {
        let directory = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(directory.path()).unwrap();
        std::fs::create_dir(base.join("root")).unwrap();
        std::fs::write(base.join("root/file.txt"), "x").unwrap();
        let inspect = |root: &str| {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(build_inspect_path_command(root, "file.txt"))
                .current_dir(&base)
                .output()
                .unwrap();
            super::super::parse_inspect_path_output(&output.stdout).unwrap()
        };

        assert_eq!(
            inspect("root"),
            inspect(&format!("{}/root", base.display()))
        );
    }
}
