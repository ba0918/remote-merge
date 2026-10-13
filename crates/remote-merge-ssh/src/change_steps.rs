//! 書き込みと symlink の作成のコマンドの部品：ない親ディレクトリを作る文、作れなかったときに作った
//! 親を消す文、新しいファイルを O_EXCL で作って書く文（docs/ir/merge/permissions.md の REQ-merge-042、
//! docs/ir/merge/concurrency.md の REQ-merge-043）。

use std::path::Path;

use crate::tree_parser::shell_escape;

/// 親の作成が失敗したことを示す目印
pub(crate) const PARENT: &str = "@RMPARENT";
/// 新しいファイルを作ろうとしたときに、書き込み先に何かが現れていたことを示す目印
pub(crate) const EXISTS: &str = "@RMEXISTS";

/// 自分の mkdir が作った親（位置引数の番号を `c` に下から順に入れたもの）を消す文。消せずに残った
/// ものは、名前と rmdir の文言を NUL で区切り、目印を付けて標準エラーに出す（名前に改行が入りうる
/// ため）。位置引数の番号は数字だけなので、eval に名前は入らない
const UNDO_PARENTS: &str = "for i in $c; do eval \"x=\\${$i}\"; e=$(rmdir -- \"$x\" 2>&1) || [ ! -e \"$x\" ] || printf '\\n@RMLEFT\\000%s\\000%s\\000' \"$x\" \"$e\" >&2; done\n";

/// 親ディレクトリを作る文。まだない親を位置引数に下から順に覚え、上から一つずつ mkdir で作る。
/// 作ったものに数えるのは自分の mkdir が成功したものだけで（番号を `c` に入れる）、作る前に別の誰かが
/// 作ったディレクトリは数えない（ローカルの経路と同じ。D39）。後で書き込み先を作れなかったときは
/// 数えたものだけを消す。mkdir が失敗したとき、親か親の祖先がすでにあって（リンク切れの symlink を
/// 含む）ディレクトリでなければ、mkdir の文言によらず "Not a directory" と答える（ローカルの経路と
/// 同じ。CentOS 5 の mkdir は "exists but is not a directory"、リンク切れの親は coreutils で
/// "File exists"、BusyBox で "No such file or directory" と出す）
pub(crate) fn parent_step(full: &str, drain: &str) -> String {
    let Some(parent) = Path::new(full).parent().and_then(Path::to_str) else {
        return "set --; c=\n".into();
    };
    if parent.is_empty() || parent == "/" {
        return "set --; c=\n".into();
    }
    format!(
        concat!(
            "d={parent}\n",
            "set --; a=$d\n",
            "while ! {{ [ -e \"$a\" ] || [ -L \"$a\" ]; }}; do set -- \"$@\" \"$a\"\n",
            " case $a in */*) a=${{a%/*}}; [ -n \"$a\" ] || break;; *) break;; esac\n",
            "done\n",
            "c=; f=0; m=; i=$#\n",
            "while [ $i -gt 0 ]; do eval \"x=\\${{$i}}\"\n",
            " if e=$(mkdir -- \"$x\" 2>&1); then c=\"$i $c\"\n",
            " elif [ ! -d \"$x\" ]; then f=1; m=$e; break; fi\n",
            " i=$((i-1))\n",
            "done\n",
            "[ $f = 0 ] && [ ! -d \"$d\" ] && {{ f=1; m='Not a directory'; }}\n",
            "if [ $f = 1 ]; then\n",
            "  a=$d\n",
            "  while :; do\n",
            "   if {{ [ -e \"$a\" ] || [ -L \"$a\" ]; }} && [ ! -d \"$a\" ]; then m='Not a directory'; break; fi\n",
            "   case $a in */*) a=${{a%/*}};; *) break;; esac\n",
            "   [ -z \"$a\" ] && break\n",
            "  done\n",
            "  {undo}",
            "  {drain}printf '{marker}\\000%s\\000' \"$m\" >&2; exit 1\n",
            "fi\n",
        ),
        parent = shell_escape(parent),
        drain = drain,
        marker = PARENT,
        undo = UNDO_PARENTS,
    )
}

/// 書き込み先を作るコマンド `create` を動かし、失敗したら作った親を消して同じ終了コードで終える文
pub(crate) fn creating(create: &str) -> String {
    format!("( {create}\n); s=$?\nif [ $s != 0 ]; then\n{UNDO_PARENTS}exit $s; fi\n")
}

/// 標準入力を `writer` に通した中身を、新しいファイル `full` として作って書くコマンド。noclobber
/// （`set -C`）の `>` は O_EXCL で開くので、確かめた後に同じパスに何かが現れていれば（別のファイルへの
/// ハードリンクや symlink を含む）開かずに失敗し、目印を出す。noclobber は、すでにある通常ファイル
/// でないもの（デバイスなど）を指す symlink を辿って開くので、開いた後にパスが symlink なら書かずに
/// 失敗する。失敗で終わるときも標準入力を読み切る
pub(crate) fn new_file(full: &str, writer: &str) -> String {
    format!(
        concat!(
            "p={p}; set -C; r=1\n",
            "{{ r=0\n",
            " if [ -L \"$p\" ]; then cat >/dev/null; printf 'Too many levels of symbolic links\\n' >&2; exit 1; fi\n",
            " {writer}\n",
            "}} >\"$p\" || {{\n",
            " cat >/dev/null\n",
            " if [ $r = 1 ] && {{ [ -e \"$p\" ] || [ -L \"$p\" ]; }}; then printf '\\n{exists}\\n' >&2; fi\n",
            " exit 1; }}",
        ),
        p = shell_escape(full),
        writer = writer,
        exists = EXISTS,
    )
}
