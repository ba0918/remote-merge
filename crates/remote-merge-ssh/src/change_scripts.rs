//! SSH の経路の書き込みと symlink の作成のコマンド（docs/ir/merge/concurrency.md の REQ-merge-038〜040、
//! docs/ir/paths/operations.md の REQ-paths-020）。
//!
//! どちらも一つのコマンドで、ない親ディレクトリを作ってから行う。失敗の種類は、道具や親の作成が
//! 標準エラーに出した文言の終わりで手元が決める。書き込みに要る道具がないときと、親を作れなかった
//! ときは、目印を付けて標準エラーに出す。書き込みは失敗で終わるときも標準入力を読み切る。

pub use crate::change_failure::failure;

use crate::change_steps::{creating, new_file, parent_step, GONE_CHECK, NO_TOOL};
use crate::tree_parser::{
    build_create_symlink_command, build_write_bytes_command, build_write_text_command, shell_escape,
};

/// 書き込みのコマンドの書き出し。シェルには何もしない文で、製品の書き込みのコマンド（標準入力に中身を
/// 送るもの）かを見分けるための印。試験の偽の SSH サーバは、この印で標準入力を待つ書き込みのコマンドを
/// 見分ける（中身に "cat >" を含むかでは、読み込みのスクリプトと取り違える）
pub const WRITE_PREFIX: &str = "LC_ALL=C; export LC_ALL; : RMWRITE\n";
/// root_dir に限らずに `full` へ書くコマンド（[`write_under`] の root_dir を "/" にしたもの）。
/// root_dir の外に置くエージェントの配置で使う
pub fn write(full: &str, binary: bool) -> String {
    write_under("/", full, binary)
}

/// 標準入力の中身を、root_dir `root` の下の `full` にその場で書くコマンド。`binary` なら標準入力は
/// base64 で、openssl で復号する（openssl がなければ書き込み先を開かずに失敗する）。作る親は root_dir
/// より下に限り、root_dir がなければ何も作らずに失敗する（D47）
pub fn write_under(root: &str, full: &str, binary: bool) -> String {
    let root = root_dir(root);
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
    script.push_str(&parent_step(full, root, drain));
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

/// root_dir に限らずに `full` へ symlink を作るコマンド（[`symlink_under`] の root_dir を "/" にしたもの）
pub fn symlink(target: &str, full: &str) -> String {
    symlink_under("/", target, full)
}

/// root_dir `root` の下の `full` に `target` を指す symlink を作るコマンド。作成先に何かがあれば
/// 置き換えずに失敗する。作る親は [`write_under`] と同じく root_dir より下に限る
pub fn symlink_under(root: &str, target: &str, full: &str) -> String {
    format!(
        "LC_ALL=C; export LC_ALL\n{}{}",
        parent_step(full, root_dir(root), ""),
        creating(&build_create_symlink_command(target, full))
    )
}

/// 親を辿るときに比べる root_dir。終わりの "/" を除く（書き込み先のパスの祖先と同じ形にする）
fn root_dir(root: &str) -> &str {
    match root.trim_end_matches('/') {
        "" => "/",
        trimmed => trimmed,
    }
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

#[cfg(all(test, unix))]
#[path = "change_scripts_space_tests.rs"]
mod space_tests;

#[cfg(all(test, unix))]
#[path = "change_scripts_permission_tests.rs"]
mod permission_tests;
