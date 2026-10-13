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
const PARENT: &str = "@RMPARENT";

/// 親ディレクトリを作る文。親がすでにあってディレクトリでなければ、mkdir の文言によらず
/// "Not a directory" と答える（CentOS 5 の mkdir は "exists but is not a directory" と出す）
fn parent_step(full: &str, drain: &str) -> String {
    let Some(parent) = Path::new(full).parent().and_then(Path::to_str) else {
        return String::new();
    };
    if parent.is_empty() || parent == "/" {
        return String::new();
    }
    format!(
        concat!(
            "d={parent}\n",
            "if [ ! -d \"$d\" ]; then\n",
            " if [ -e \"$d\" ]; then {drain}printf '{marker}\\000Not a directory\\000' >&2; exit 1; fi\n",
            " m=$(mkdir -p -- \"$d\" 2>&1) || {{ {drain}printf '{marker}\\000%s\\000' \"$m\" >&2; exit 1; }}\n",
            "fi\n",
        ),
        parent = shell_escape(parent),
        drain = drain,
        marker = PARENT,
    )
}

/// 標準入力の中身を `full` にその場で書くコマンド。`binary` なら標準入力は base64 で、openssl で
/// 復号する（openssl がなければ書き込み先を開かずに失敗する）
pub fn write(full: &str, binary: bool) -> String {
    let drain = "cat >/dev/null; ";
    let mut script = String::from("LC_ALL=C; export LC_ALL\n");
    if binary {
        script.push_str(&format!(
            "command -v openssl >/dev/null 2>&1 || {{ {drain}echo '{NO_TOOL}' >&2; exit 1; }}\n"
        ));
    }
    script.push_str(&parent_step(full, drain));
    script.push_str(&if binary {
        build_write_bytes_command(full)
    } else {
        build_write_text_command(full)
    });
    script.push('\n');
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
        "LC_ALL=C; export LC_ALL\n{}{}\n",
        parent_step(full, ""),
        build_create_symlink_command(target, full)
    )
}

/// 0 以外で終わった書き込みか symlink の作成の標準エラーを、失敗の種類にする
pub fn failure(stderr: &[u8]) -> OpFailure {
    let has = |marker: &[u8]| stderr.windows(marker.len()).any(|window| window == marker);
    if has(NO_TOOL.as_bytes()) {
        return OpFailure::ToolMissing(Tool::OpensslBase64);
    }
    let parent = [PARENT.as_bytes(), b"\0"].concat();
    if let Some(at) = stderr
        .windows(parent.len())
        .position(|window| window == parent.as_slice())
    {
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
