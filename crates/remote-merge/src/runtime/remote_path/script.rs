//! リモートで "/" から要素ごとに辿るスクリプト。手順は core の `resolve_path` と同じで、
//! 基本操作は `[ -L ]`・`[ -d ]`・`[ -e ]`・`[ -f ]`、組み込みの cd・pwd・printf・command -v と
//! オプションなしの readlink だけを使う。
//!
//! 状態は core の手順と同じものを文字列で持つ。
//! - `q`: まだ辿っていない要素を "/" でつないだもの。空の要素と "." は入れる前に除き、最後の要素の
//!   後に "/" があったときだけ末尾に "/" を残す。`n` はその要素の数
//! - `ex`: 辿っている途中のリンク先。差し込んだときに後ろに残っていた要素の数と、差し込んだ
//!   symlink が確認する相対パスの要素だったか（1 か 0）を ":" でつないだものを空白で並べる
//! - `nrel`: `q` の後ろにある確認する相対パスの要素の数
//!
//! 答えは欄ごとに NUL で終え、読んだリンク文字列を `link` の欄で順に出してから、結果か
//! `error` と原因の文言を出す。辿れなかったことも終了コード 0 で返し、0 以外はスクリプトの失敗にする。
//! 検索権限は `[ -x ]` ではなく cd の成否で判定する。`[ -x ]` は BusyBox ash が root のモード 000 の
//! ディレクトリを検索できないと答え、実効 uid の access と食い違う

/// コマンドの先頭の並び。試験 SSH サーバは同じ並びを別に持ち、これでパスの確認を見分けて `r=` と `p=` を読む
const INSPECT_PATH_PREFIX: &str = "LC_ALL=C; r=";

const SCRIPT: &str = r#"emit() { printf '%s\000' "$@"; };
fail() { emit error "$1"; exit 0; };
normalize() {
  s=$1; nq=; nc=0; last_after=;
  while [ -n "$s" ]; do
    case $s in */*) part=${s%%/*}; s=${s#*/}; after=1 ;; *) part=$s; s=; after= ;; esac;
    case $part in ''|.) ;; *) nq=${nq:+$nq/}$part; nc=$((nc + 1)); last_after=$after ;; esac;
  done;
  if [ -n "$last_after" ]; then nq=$nq/; fi;
};
push() {
  if [ "$nc" -gt 0 ]; then
    if [ -n "$q" ]; then q=${nq%/}/$q; elif [ -n "$sl" ]; then q=${nq%/}/; else q=$nq; fi;
    n=$((n + nc));
  fi;
};
case $r in /*) ;; *) d=$(pwd -P) || fail "cannot read"; r=$d/$r ;; esac;
normalize "$r"; q=$nq; n=$nc;
normalize "$p"; nrel=$nc;
if [ "$nc" -gt 0 ]; then if [ -n "$q" ]; then q=${q%/}/$nq; else q=$nq; fi; n=$((n + nc)); fi;
real=/; ex=; tl=; tlset=; mp=; nl=0;
while [ "$n" -gt 0 ]; do
  case $q in */*) c=${q%%/*}; q=${q#*/}; sl=1 ;; *) c=$q; q=; sl= ;; esac;
  n=$((n - 1)); rest=$n;
  kept=; for e in $ex; do if [ "${e%:*}" -le "$rest" ]; then kept="$kept $e"; fi; done; ex=$kept;
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
    t=$(readlink -- "$next" && echo x) || fail "cannot read";
    t=${t%x}; t=${t%?};
    emit link "$t";
    if [ "$fr" = 1 ] && [ "$rest" -eq 0 ]; then tl=$t; tlset=1; fi;
    case $t in /*) real=/ ;; esac;
    ex="$ex $rest:$fr";
    normalize "$t"; push;
  elif [ -d "$next" ]; then real=$next;
  elif [ -e "$next" ]; then
    if [ -f "$next" ]; then kind=regular; else kind=other; fi;
    if [ "$rest" -gt 0 ]; then
      for e in $ex; do if [ "${e%:*}" -lt "$rest" ]; then fail "not a directory"; fi; done;
      origin=$fr; if [ -n "$ex" ]; then f=${ex# }; f=${f%% *}; origin=${f#*:}; fi;
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

    /// 試験 SSH サーバ（tests/contract/ssh_server.rs）は同じ並びを持ち、これでパスの確認を見分ける
    #[test]
    fn the_command_starts_with_the_locale_and_the_root_dir_assignment() {
        assert!(build_inspect_path_command("/srv/app", "a.txt")
            .starts_with("LC_ALL=C; r='/srv/app'; p='a.txt'; "));
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
