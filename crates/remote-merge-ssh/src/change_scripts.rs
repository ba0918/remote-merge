//! SSH の経路の書き込みと symlink の作成のコマンド（docs/ir/merge/concurrency.md の REQ-merge-038〜040、
//! docs/ir/paths/operations.md の REQ-paths-020）。
//!
//! どちらも一つのコマンドで、ない親ディレクトリを作ってから行う。失敗の種類は、道具や親の作成が
//! 標準エラーに出した文言の終わりで手元が決める。書き込みに要る道具がないときと、親を作れなかった
//! ときは、目印を付けて標準エラーに出す。書き込みは失敗で終わるときも標準入力を読み切る。

use remote_merge_core::op_failure::{from_tool_message, OpFailure, Tool};

use crate::change_steps::{creating, new_file, parent_step, EXISTS, GONE_CHECK, PARENT};
use crate::tree_parser::{
    build_create_symlink_command, build_write_bytes_command, build_write_text_command, shell_escape,
};

const NO_TOOL: &str = "@RMNOTOOL";
/// 書き込みのコマンドの書き出し。シェルには何もしない文で、製品の書き込みのコマンド（標準入力に中身を
/// 送るもの）かを見分けるための印。試験の偽の SSH サーバは、この印で標準入力を待つ書き込みのコマンドを
/// 見分ける（中身に "cat >" を含むかでは、読み込みのスクリプトと取り違える）
pub const WRITE_PREFIX: &str = "LC_ALL=C; export LC_ALL; : RMWRITE\n";
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
    // 書き込み先がないかを、親を作る前に確かめる（ローカルの経路と同じ）。なければ新しいファイルとして
    // O_EXCL で作る（REQ-merge-043）
    script.push_str(&format!(
        "w={}\nif [ -e \"$w\" ] || [ -L \"$w\" ]; then n=0; else n=1; fi\n",
        shell_escape(full)
    ));
    script.push_str(GONE_CHECK);
    script.push_str(&parent_step(full, drain));
    // 既存の書き込み先で、dd が oflag=nofollow を受け付けず書き込み先が symlink なら、辿らずに断る理由を
    // ローカルの経路の O_NOFOLLOW と同じ文言で出す（新しいファイルの手順は、現れた symlink を自分で断る）
    script.push_str(&format!(
        concat!(
            "if [ $n = 0 ] && [ -L \"$w\" ] && ! dd if=/dev/null of=/dev/null oflag=nofollow 2>/dev/null; then\n",
            " {drain}printf 'Too many levels of symbolic links\\n' >&2; exit 1\n",
            "fi\n",
        ),
        drain = drain,
    ));
    let (existing, writer) = if binary {
        (build_write_bytes_command(full), "openssl base64 -d -A")
    } else {
        (build_write_text_command(full), "cat")
    };
    script.push_str("if [ $n = 1 ]; then\n");
    script.push_str(&creating(&new_file(full, writer)));
    script.push_str("else\n");
    script.push_str(&creating(&existing));
    script.push_str("fi\n");
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
    let (rest, left) = left_parents(stderr);
    let failure = kind_of(&rest);
    if left.is_empty() {
        return failure;
    }
    OpFailure::Io(format!(
        "{failure}; these created directories were left because they could not be removed: {}",
        left.join(", ")
    ))
}

/// 標準エラーから、消せずに残った親の記録（名前と、rmdir の文言の終わりの理由）を取り出し、残りの
/// 標準エラーと分ける。記録は "名前 (理由)" の形にする（ローカルの経路と同じ）
fn left_parents(stderr: &[u8]) -> (Vec<u8>, Vec<String>) {
    const MARK: &[u8] = b"\n@RMLEFT\0";
    let mut rest = Vec::new();
    let mut left = Vec::new();
    let mut tail = stderr;
    while let Some(at) = tail.windows(MARK.len()).position(|window| window == MARK) {
        rest.extend_from_slice(&tail[..at]);
        let mut fields = tail[at + MARK.len()..].splitn(3, |byte| *byte == 0);
        let (Some(name), Some(message), Some(after)) =
            (fields.next(), fields.next(), fields.next())
        else {
            rest.extend_from_slice(&tail[at..]);
            return (rest, left);
        };
        let message = String::from_utf8_lossy(message);
        let message = message.trim_end();
        let reason = message
            .rsplit_once(": ")
            .map_or(message, |(_, reason)| reason);
        left.push(format!("{} ({reason})", String::from_utf8_lossy(name)));
        tail = after;
    }
    rest.extend_from_slice(tail);
    (rest, left)
}

/// 目印を除いた標準エラーの種類
fn kind_of(stderr: &[u8]) -> OpFailure {
    for line in stderr.split(|byte| *byte == b'\n') {
        match line.strip_prefix(NO_TOOL.as_bytes()) {
            Some(b" cat") => return OpFailure::ToolMissing(Tool::Cat),
            Some(b" openssl") => return OpFailure::ToolMissing(Tool::OpensslBase64),
            _ => {}
        }
        // 新しいファイルを作ろうとしたときに何かが現れていた。ローカルの経路の EEXIST と同じ文言にする
        // （シェルの noclobber の文言は bash と dash・BusyBox で違う）
        if line == EXISTS.as_bytes() {
            return OpFailure::Io("File exists".into());
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

#[cfg(all(test, unix))]
#[path = "change_scripts_race_tests.rs"]
mod race_tests;

#[cfg(all(test, unix))]
#[path = "change_scripts_gone_tests.rs"]
mod gone_tests;
