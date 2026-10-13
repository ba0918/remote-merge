//! 0 以外で終わった SSH の書き込みと symlink の作成の標準エラーを、失敗の種類にする
//! （docs/ir/paths/operations.md の REQ-paths-020）

use remote_merge_core::op_failure::{from_tool_message, OpFailure, Tool};

use crate::change_steps::{without_dd_transfer_lines, EXISTS, NO_TOOL, PARENT, ROOT_GONE};

/// 0 以外で終わった書き込みか symlink の作成の標準エラーを、失敗の種類にする。目印は行の始まりに
/// あるときだけ読む（道具の文言に含まれるパスの中の同じ文字列と取り違えない）
pub fn failure(stderr: &[u8]) -> OpFailure {
    let (rest, left) = left_parents(stderr);
    let failure = kind_of(&without_dd_transfer_lines(&rest));
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
            Some(b" ls") => return OpFailure::ToolMissing(Tool::LsDirectory),
            _ => {}
        }
        // 新しいファイルを作ろうとしたときに何かが現れていた。ローカルの経路の EEXIST と同じ文言にする
        // （シェルの noclobber の文言は bash と dash・BusyBox で違う）
        if line == EXISTS.as_bytes() {
            return OpFailure::Io("File exists".into());
        }
        if line == ROOT_GONE.as_bytes() {
            return OpFailure::RootGone;
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
