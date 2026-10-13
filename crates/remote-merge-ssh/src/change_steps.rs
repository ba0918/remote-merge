//! 書き込みと symlink の作成のコマンドの部品：ない親ディレクトリを作る文、作れなかったときに作った
//! 親を消す文、新しいファイルを O_EXCL で作って書く文（docs/ir/merge/permissions.md の REQ-merge-042、
//! docs/ir/merge/concurrency.md の REQ-merge-043）。

use std::path::Path;

use crate::tree_parser::shell_escape;

/// 親の作成が失敗したことを示す目印
pub(crate) const PARENT: &str = "@RMPARENT";
/// 新しいファイルを作ろうとしたときに、書き込み先に何かが現れていたことを示す目印
pub(crate) const EXISTS: &str = "@RMEXISTS";
/// 道具がないことを示す目印（後に道具の名前が続く）
pub(crate) const NO_TOOL: &str = "@RMNOTOOL";
/// root_dir がない（ディレクトリでない）ことを示す目印
pub(crate) const ROOT_GONE: &str = "@RMROOTGONE";

/// 自分の mkdir が作った親（位置引数の番号を `c` に下から順に入れたもの）を消す文。消せずに残った
/// ものは、名前と rmdir の文言を NUL で区切り、目印を付けて標準エラーに出す（名前に改行が入りうる
/// ため）。消えていたもの（リンク切れの symlink も含めて何もないもの）は消せたものとし、別のものに
/// 替わっていたものは rmdir が消さないので残ったものになる（ローカルの経路と同じ。D43）。位置引数の
/// 番号は数字だけなので、eval に名前は入らない
const UNDO_PARENTS: &str = "for i in $c; do eval \"x=\\${$i}\"; e=$(rmdir -- \"$x\" 2>&1) || { [ ! -e \"$x\" ] && [ ! -L \"$x\" ]; } || printf '\\n@RMLEFT\\000%s\\000%s\\000' \"$x\" \"$e\" >&2; done\n";

/// 親ディレクトリを作る文。まだない親を位置引数に下から順に覚え、上から一つずつ mkdir で作る。
/// 作ったものに数えるのは自分の mkdir が成功したものだけで（番号を `c` に入れる）、作る前に別の誰かが
/// 作ったディレクトリは数えない（ローカルの経路と同じ。D39）。後で書き込み先を作れなかったときは
/// 数えたものだけを消す。mkdir が失敗したとき、親か親の祖先がすでにあって（リンク切れの symlink を
/// 含む）ディレクトリでなければ、mkdir の文言によらず "Not a directory" と答える（ローカルの経路と
/// 同じ。CentOS 5 の mkdir は "exists but is not a directory"、リンク切れの親は coreutils で
/// "File exists"、BusyBox で "No such file or directory" と出す）
///
/// 作る親は root_dir `root` より下に限る。root_dir がない（ディレクトリでない）ときは何も作らずに
/// 目印を出して終える（ローカルの経路と同じ。D47）。`root` が "/" なら限らない
pub(crate) fn parent_step(full: &str, root: &str, drain: &str) -> String {
    // ディレクトリでないときは ls -d の文言の終わりで分ける。検索の権限がない下のパスは
    // "Permission denied" で、権限がない失敗にする（docs/remote-tools.md）
    let root_check = format!(
        concat!(
            "r={root}; if [ ! -d \"$r\" ]; then {drain}\n",
            " if ! command -v ls >/dev/null 2>&1; then printf '\\n{no_tool} ls\\n' >&2\n",
            " elif e=$(ls -d -- \"$r\" 2>&1) || [ \"${{e%Permission denied}}\" = \"$e\" ]; then printf '\\n{gone}\\n' >&2\n",
            " else printf '\\n{parent}\\000%s\\000' \"$e\" >&2; fi\n",
            " exit 1\n",
            "fi\n",
        ),
        root = shell_escape(root),
        drain = drain,
        gone = ROOT_GONE,
        parent = PARENT,
        no_tool = NO_TOOL,
    );
    let Some(parent) = Path::new(full).parent().and_then(Path::to_str) else {
        return format!("{root_check}set --; c=\n");
    };
    if parent.is_empty() || parent == "/" {
        return format!("{root_check}set --; c=\n");
    }
    root_check
        + &format!(
        concat!(
            "d={parent}\n",
            "set --; a=$d\n",
            "while [ \"$a\" != \"$r\" ] && ! {{ [ -e \"$a\" ] || [ -L \"$a\" ]; }}; do set -- \"$@\" \"$a\"\n",
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

/// 確かめたときにあった書き込み先（`n=0`）が、その後に親ごと消えていないかを、標準入力を読まずに
/// 確かめ、消えていれば新しいファイルの手順（`n=1`）に回す文。ローカルの経路が書き込み先を作らずに
/// 開いて "ない" と分かったときに、ない親を作って新しいファイルとして作るのと同じ（D41・D45）。dd が
/// `conv=nocreat` を受け付ければ（coreutils）、書き込み先を作らず・切り詰めずに開いて確かめる。受け付け
/// なければ（BusyBox）パスがあるかを確かめ直す。親を作る文より前に置く（親を作る文は一度だけ動かし、
/// 作った親の記録を消さない）
pub(crate) const GONE_CHECK: &str = concat!(
    "if [ $n = 0 ]; then\n",
    " if dd if=/dev/null of=/dev/null oflag=nofollow conv=nocreat count=0 2>/dev/null; then\n",
    "  e=$(dd if=/dev/null of=\"$w\" oflag=nofollow conv=nocreat,notrunc count=0 2>&1) || [ \"${e%No such file or directory}\" = \"$e\" ] || n=1\n",
    " elif [ ! -e \"$w\" ] && [ ! -L \"$w\" ]; then n=1; fi\n",
    "fi\n",
);

/// 作ったディレクトリ `dirs`（下から順）を、書き込みのコマンドが失敗したときと同じ文で消すコマンド。
/// 消せずに残ったものは同じ目印で標準エラーに出す（三つの経路の判断を比べる契約テストで使う）
pub fn remove_created_dirs(dirs: &[&str]) -> String {
    let words: String = dirs
        .iter()
        .map(|dir| format!(" {}", shell_escape(dir)))
        .collect();
    let numbers: Vec<String> = (1..=dirs.len()).map(|i| i.to_string()).collect();
    format!(
        "LC_ALL=C; export LC_ALL\nset --{words}; c='{}'\n{UNDO_PARENTS}",
        numbers.join(" ")
    )
}

/// 標準エラーから、dd の転送量の行（"N+N records in"・"N+N records out"・"N bytes … copied"）を除く。
/// `status=none` を受け付けない古い dd（CentOS 5 の coreutils 5.97）は、書けずに終わったときも
/// これらの行を理由の後に出すので、理由の文言に混ぜない。行の始まりが数字の dd の形だけを除き、
/// パスに同じ語を含む理由の行は残す
pub(crate) fn without_dd_transfer_lines(stderr: &[u8]) -> Vec<u8> {
    let kept: Vec<&[u8]> = stderr
        .split(|byte| *byte == b'\n')
        .filter(|line| !is_dd_transfer_line(&String::from_utf8_lossy(line)))
        .collect();
    kept.join(&b'\n')
}

/// dd の転送量の行か
fn is_dd_transfer_line(line: &str) -> bool {
    let line = line.trim_end();
    let Some((_, rest)) = split_number(line) else {
        return false;
    };
    if let Some(rest) = rest.strip_prefix('+') {
        return split_number(rest)
            .is_some_and(|(_, tail)| tail == " records in" || tail == " records out");
    }
    rest.starts_with(" bytes ") && rest.contains(" copied")
}

/// 先頭の数字の並びと残り。数字で始まらなければ None
fn split_number(text: &str) -> Option<(&str, &str)> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    (end > 0).then(|| text.split_at(end))
}

/// 書き込み先を作るコマンド `create` を動かし、失敗したら作った親を消して同じ終了コードで終える文
pub(crate) fn creating(create: &str) -> String {
    format!("( {create}\n); s=$?\nif [ $s != 0 ]; then\n{UNDO_PARENTS}exit $s; fi\n")
}

/// 標準入力を `writer` に通した中身を、新しいファイル `full` として作って書くコマンド。確かめた後に
/// 同じパスに何かが現れていれば（別のファイルへのハードリンクや symlink を含む）開かずに失敗し、
/// ローカルの経路の EEXIST と同じく "File exists" にする目印を出す。失敗で終わるときも標準入力を
/// 読み切る。
///
/// dd が `oflag=nofollow` と `conv=excl` を受け付けるか（coreutils）を実行時に確かめ、受け付ければ dd が
/// O_EXCL|O_NOFOLLOW で開く。受け付けなければ（BusyBox）noclobber（`set -C`）の `>` で O_EXCL で開く。
/// noclobber は、すでにある通常ファイルでないもの（デバイスや FIFO）を指す symlink を辿って開くので、
/// 開いた後にパスが symlink なら書かずに同じ目印で失敗する（開くまでの窓は FLAG-merge-021 に残す）。
/// コマンド置換の中では case を使わない（CentOS 5 の bash 3.2 は、パターンの ")" をコマンド置換の
/// 終わりと読んで構文の誤りにする）
pub(crate) fn new_file(full: &str, writer: &str) -> String {
    format!(
        concat!(
            "p={p}; fail() {{ cat >/dev/null; exit 1; }}\n",
            "if dd if=/dev/null of=/dev/null oflag=nofollow 2>/dev/null && dd if=/dev/null conv=excl count=0 </dev/null >/dev/null 2>&1; then\n",
            " q=; if dd if=/dev/null of=/dev/null status=none 2>/dev/null; then q=status=none; fi\n",
            " s=$( {{ {{ {writer} 3>&-; echo $? >&3; }} | {{ e=$(dd of=\"$p\" bs=65536 oflag=nofollow conv=excl $q 2>&1) && exit 0; c=$?\n",
            "  if [ \"${{e%File exists}}\" != \"$e\" ]; then printf '\\n{exists}\\n' >&2; else printf '%s\\n' \"$e\" >&2; fi; exit $c; }} 3>&-; }} 3>&1 ) || fail\n",
            " [ \"$s\" = 0 ] || fail\n",
            "else\n",
            " set -C; r=1\n",
            " {{ r=0\n",
            "  if [ -L \"$p\" ]; then cat >/dev/null; printf '\\n{exists}\\n' >&2; exit 1; fi\n",
            "  {writer}\n",
            " }} >\"$p\" || {{\n",
            "  if [ $r = 1 ] && {{ [ -e \"$p\" ] || [ -L \"$p\" ]; }}; then printf '\\n{exists}\\n' >&2; fi\n",
            "  fail; }}\n",
            "fi",
        ),
        p = shell_escape(full),
        writer = writer,
        exists = EXISTS,
    )
}
