//! SSH の経路の書き込みと symlink の作成のコマンド（docs/ir/merge/concurrency.md の REQ-merge-038〜040、
//! docs/ir/paths/operations.md の REQ-paths-020）。
//!
//! どちらも一つのコマンドで、ない親ディレクトリを作ってから行う。失敗の種類は、道具や親の作成が
//! 標準エラーに出した文言の終わりで手元が決める。書き込みに要る道具がないときと、親を作れなかった
//! ときは、目印を付けて標準エラーに出す。書き込みは失敗で終わるときも標準入力を読み切る。

use std::path::Path;

use remote_merge_core::op_failure::{from_tool_message, OpFailure, Tool};

use crate::tree_parser::{
    build_create_symlink_command, build_write_bytes_command, build_write_text_command, shell_escape,
};

const NO_TOOL: &str = "@RMNOTOOL";
/// 書き込みのコマンドの書き出し。製品の書き込みのコマンドかを見分けるのに使える
pub const WRITE_PREFIX: &str = "LC_ALL=C; export LC_ALL; : RMWRITE\n";
const PARENT: &str = "@RMPARENT";

/// 作った親ディレクトリ（位置引数に下から順に入れたもの）を消す文。消せずに残ったものは目印を
/// 付けて標準エラーに出す
const UNDO_PARENTS: &str = "for x do rmdir -- \"$x\" 2>/dev/null || [ ! -e \"$x\" ] || printf '@RMLEFT %s\\n' \"$x\" >&2; done\n";

/// 親ディレクトリを作る文。まだない親を位置引数に下から順に覚え、後で書き込み先を作れなかったときに
/// 消せるようにする（ローカルの経路と同じ。REQ-merge-042）。mkdir が失敗したとき、親か親の祖先が
/// すでにあって（リンク切れの symlink を含む）ディレクトリでなければ、mkdir の文言によらず
/// "Not a directory" と答える（ローカルの経路と同じ。CentOS 5 の mkdir は
/// "exists but is not a directory"、リンク切れの親は coreutils で "File exists"、BusyBox で
/// "No such file or directory" と出す）
fn parent_step(full: &str, drain: &str) -> String {
    let Some(parent) = Path::new(full).parent().and_then(Path::to_str) else {
        return "set --\n".into();
    };
    if parent.is_empty() || parent == "/" {
        return "set --\n".into();
    }
    format!(
        concat!(
            "d={parent}\n",
            "set --; a=$d\n",
            "while ! {{ [ -e \"$a\" ] || [ -L \"$a\" ]; }}; do set -- \"$@\" \"$a\"\n",
            " case $a in */*) a=${{a%/*}}; [ -n \"$a\" ] || break;; *) break;; esac\n",
            "done\n",
            "if [ ! -d \"$d\" ]; then\n",
            " m=$(mkdir -p -- \"$d\" 2>&1) || {{\n",
            "  a=$d\n",
            "  while :; do\n",
            "   if {{ [ -e \"$a\" ] || [ -L \"$a\" ]; }} && [ ! -d \"$a\" ]; then m='Not a directory'; break; fi\n",
            "   case $a in */*) a=${{a%/*}};; *) break;; esac\n",
            "   [ -z \"$a\" ] && break\n",
            "  done\n",
            "  {undo}",
            "  {drain}printf '{marker}\\000%s\\000' \"$m\" >&2; exit 1; }}\n",
            "fi\n",
        ),
        parent = shell_escape(parent),
        drain = drain,
        marker = PARENT,
        undo = UNDO_PARENTS,
    )
}

/// 書き込み先を作るコマンド `create` を動かし、失敗したら作った親を消して同じ終了コードで終える文
fn creating(create: &str) -> String {
    format!("( {create}\n); s=$?\nif [ $s != 0 ]; then\n{UNDO_PARENTS}exit $s; fi\n")
}

/// 標準入力の中身を `full` にその場で書くコマンド。`binary` なら標準入力は base64 で、openssl で
/// 復号する（openssl がなければ書き込み先を開かずに失敗する）
pub fn write(full: &str, binary: bool) -> String {
    let drain = "cat >/dev/null; ";
    let mut script = String::from(WRITE_PREFIX);
    // cat がなければ標準入力を dd で読み切る（どちらもなければ読み切らずに終える）
    script.push_str(&format!(
        "command -v cat >/dev/null 2>&1 || {{ dd of=/dev/null 2>/dev/null; printf '\\n{NO_TOOL} cat\\n' >&2; exit 1; }}\n"
    ));
    if binary {
        script.push_str(&format!(
            "command -v openssl >/dev/null 2>&1 || {{ {drain}printf '\\n{NO_TOOL} openssl\\n' >&2; exit 1; }}\n"
        ));
    }
    script.push_str(&parent_step(full, drain));
    // dd が oflag=nofollow を受け付けず書き込み先が symlink なら、辿らずに断る理由をローカルの経路の
    // O_NOFOLLOW と同じ文言で出す
    script.push_str(&format!(
        concat!(
            "w={full}\n",
            "if [ -L \"$w\" ] && ! dd if=/dev/null of=/dev/null oflag=nofollow 2>/dev/null; then\n",
            " {drain}printf 'Too many levels of symbolic links\\n' >&2; exit 1\n",
            "fi\n",
        ),
        full = shell_escape(full),
        drain = drain,
    ));
    script.push_str(&creating(&if binary {
        build_write_bytes_command(full)
    } else {
        build_write_text_command(full)
    }));
    script
}

/// [`write`] のコマンドに送る標準入力。`binary` なら 1 行の base64 に改行を付ける（openssl base64 -d
/// -A は改行のない 1 行を受け付け、終わりの改行がないと復号しない）
pub fn write_input(content: &[u8], binary: bool) -> Vec<u8> {
    if !binary {
        return content.to_vec();
    }
    use base64::Engine;
    let mut encoded = base64::engine::general_purpose::STANDARD
        .encode(content)
        .into_bytes();
    encoded.push(b'\n');
    encoded
}

/// `full` に `target` を指す symlink を作るコマンド。作成先に何かがあれば置き換えずに失敗する
pub fn symlink(target: &str, full: &str) -> String {
    format!(
        "LC_ALL=C; export LC_ALL\n{}{}",
        parent_step(full, ""),
        creating(&build_create_symlink_command(target, full))
    )
}

/// 0 以外で終わった書き込みか symlink の作成の標準エラーを、失敗の種類にする。目印は行の始まりに
/// あるときだけ読む（道具の文言に含まれるパスの中の同じ文字列と取り違えない）
pub fn failure(stderr: &[u8]) -> OpFailure {
    let mut left = Vec::new();
    let mut rest: Vec<&[u8]> = Vec::new();
    for line in stderr.split(|byte| *byte == b'\n') {
        match line.strip_prefix(b"@RMLEFT ") {
            Some(dir) => left.push(String::from_utf8_lossy(dir).into_owned()),
            None => rest.push(line),
        }
    }
    let failure = kind_of(&rest.join(&b'\n'));
    if left.is_empty() {
        return failure;
    }
    OpFailure::Io(format!(
        "{failure}; these created directories were left because they could not be removed: {}",
        left.join(", ")
    ))
}

/// 目印を除いた標準エラーの種類
fn kind_of(stderr: &[u8]) -> OpFailure {
    for line in stderr.split(|byte| *byte == b'\n') {
        match line.strip_prefix(NO_TOOL.as_bytes()) {
            Some(b" cat") => return OpFailure::ToolMissing(Tool::Cat),
            Some(b" openssl") => return OpFailure::ToolMissing(Tool::OpensslBase64),
            _ => {}
        }
    }
    let parent = [PARENT.as_bytes(), b"\0"].concat();
    let at_line_start = |at: usize| at == 0 || stderr[at - 1] == b'\n';
    let found =
        (0..stderr.len()).find(|&at| at_line_start(at) && stderr[at..].starts_with(&parent));
    if let Some(at) = found {
        let message = stderr[at + parent.len()..]
            .split(|byte| *byte == 0)
            .next()
            .unwrap_or_default();
        return from_tool_message(message);
    }
    from_tool_message(stderr)
}

#[cfg(all(test, unix))]
#[path = "change_scripts_tests.rs"]
mod tests;
