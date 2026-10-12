//! 走査のスクリプトの標準エラーに find が出した診断を、読めなかったディレクトリとして読む
use std::collections::HashMap;

use remote_merge_core::scan::raw::{Answer, RawKind, ReadCause, UnreadableDir};

use crate::scan_fields::diagnosed_path;
use crate::scan_output::printed_start;
use crate::scan_script::{ScriptQuestion, MARKER};

/// find の標準エラーの各行を、読めなかったディレクトリとして答えに入れる。項目そのものを読め
/// なかった行は、それを入れているディレクトリを読めなかったものにする。それ以外の行は誤り
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
    for line in stderr[begin..]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let path = diagnosed_path(line);
        let path = path.as_deref();
        let parent = path.and_then(|path| {
            path.iter()
                .rposition(|byte| *byte == b'/')
                .map(|at| &path[..at])
        });
        let Some((index, rel)) = path
            .and_then(|path| directories.get(path))
            .or_else(|| parent.and_then(|parent| directories.get(parent)))
        else {
            return Err(format!("find reported: {}", String::from_utf8_lossy(line)));
        };
        let cause = if line.ends_with(b"Permission denied") {
            ReadCause::PermissionDenied
        } else {
            ReadCause::Other
        };
        if let Some(tree) = &mut answers[*index].subtree {
            tree.unreadable.push(UnreadableDir::new(rel.clone(), cause));
        }
        attributed += 1;
    }
    Ok(attributed)
}
