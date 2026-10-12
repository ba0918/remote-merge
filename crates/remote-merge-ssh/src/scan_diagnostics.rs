//! 走査のスクリプトの標準エラーに find が出した診断を、読めなかったディレクトリとして読む
use std::collections::HashMap;

use remote_merge_core::scan::raw::{Answer, RawKind, ReadCause, UnreadableDir};

use crate::scan_fields::diagnosed_path;
use crate::scan_fields::printed_start;
use crate::scan_script::{ScriptQuestion, MARKER};

/// find の標準エラーの知らせを答えに入れる。ディレクトリを読めなかった知らせは読めなかった
/// ディレクトリに、項目そのものの情報を読めなかった知らせはその項目に入れる。それ以外は誤り
pub fn attribute(
    stderr: &[u8],
    questions: &[ScriptQuestion],
    answers: &mut [Answer],
) -> Result<usize, String> {
    let marker = [MARKER.as_bytes(), b"\n"].concat();
    let begin = stderr
        .windows(marker.len())
        .position(|window| window == marker)
        .map_or(stderr.len(), |at| at + marker.len());
    let mut directories: HashMap<Vec<u8>, (usize, Vec<u8>)> = HashMap::new();
    for (index, (question, answer)) in questions.iter().zip(answers.iter()).enumerate() {
        let Some(tree) = &answer.subtree else {
            continue;
        };
        let start = printed_start(question);
        directories.insert(start.clone(), (index, Vec::new()));
        for entry in tree.entries.iter().filter(|e| e.kind == RawKind::Directory) {
            directories.insert(
                [&start[..], b"/", &entry.path].concat(),
                (index, entry.path.clone()),
            );
        }
    }
    let mut attributed = 0;
    for message in messages(&stderr[begin..]) {
        let unexplained = || format!("find reported: {}", String::from_utf8_lossy(&message));
        let path = diagnosed_path(&message).ok_or_else(unexplained)?;
        let cause = if message.ends_with(b"Permission denied") {
            ReadCause::PermissionDenied
        } else {
            ReadCause::Other
        };
        if let Some((index, rel)) = directories.get(&path) {
            if let Some(tree) = &mut answers[*index].subtree {
                tree.unreadable.push(UnreadableDir::new(rel.clone(), cause));
            }
        } else {
            // 項目そのものの情報を読めなかった。exclude に当たるかは規則が決めるので、項目自身の
            // パスで答える
            let at = path
                .iter()
                .rposition(|byte| *byte == b'/')
                .ok_or_else(unexplained)?;
            let (index, parent) = directories.get(&path[..at]).ok_or_else(unexplained)?;
            let item = remote_merge_core::scan::raw::join_relative(parent, &path[at + 1..]);
            if let Some(tree) = &mut answers[*index].subtree {
                tree.unreadable_entries
                    .push(UnreadableDir::new(item, cause));
            }
        }
        attributed += 1;
    }
    Ok(attributed)
}

/// 標準エラーを find の知らせ一つずつに分ける。古い find（4.2.27）はパスを引用しないので、名前の
/// 中の改行で行が割れる。"find: " で始まらない行は前の知らせの続きとしてつなぐ
fn messages(text: &[u8]) -> Vec<Vec<u8>> {
    let mut messages: Vec<Vec<u8>> = Vec::new();
    let body = text.strip_suffix(b"\n").unwrap_or(text);
    if body.is_empty() {
        return messages;
    }
    for line in body.split(|byte| *byte == b'\n') {
        match messages.last_mut() {
            Some(last) if !line.starts_with(b"find: ") => {
                last.push(b'\n');
                last.extend_from_slice(line);
            }
            _ => messages.push(line.to_vec()),
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_whose_unquoted_path_holds_a_newline_is_kept_whole() {
        let text = b"find: ./cache/lo\ncked: Permission denied\nfind: './x': Permission denied\n";
        assert_eq!(
            messages(text),
            [
                b"find: ./cache/lo\ncked: Permission denied".to_vec(),
                b"find: './x': Permission denied".to_vec()
            ]
        );
        assert!(messages(b"").is_empty());
    }
}
