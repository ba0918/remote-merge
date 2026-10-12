//! もう一度尋ねた答え（[`crate::scan_recheck`]）を読み、尋ねたディレクトリと突き合わせて入れる
use std::collections::{HashMap, HashSet};

use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::raw::{join_relative, Answer};

use crate::scan_checks::split_stderr;
use crate::scan_diagnostics::DirCheck;
use crate::scan_recheck::EmptyDir;
use crate::scan_script::MARKER;

/// もう一度尋ねた答えを読む。root_dir にもう入れなければ `None`（root_dir がなくなったと
/// 出したかは、先に [`crate::scan_script::root_gone`] で確かめる）
pub fn read_recheck(stdout: &[u8], stderr: &[u8]) -> Result<Option<Vec<DirCheck>>, String> {
    let marker = [MARKER.as_bytes(), b"\0"].concat();
    let begin = stdout
        .windows(marker.len())
        .position(|window| window == marker)
        .ok_or("The second scan output has no start marker")?;
    let mut tokens = stdout[begin + marker.len()..].split(|byte| *byte == 0);
    match tokens.next() {
        Some(b"@F") => return Ok(None),
        Some(b"@R") => {}
        _ => return Err("Unexpected second scan output".into()),
    }
    let status = tokens.next().unwrap_or_default();
    if tokens.next() != Some(b"@RT")
        || tokens.next() != Some(b"1")
        || tokens.next() != Some(b"@END")
    {
        return Err("The second scan output ended early".into());
    }
    if status != b"0" {
        return Err(format!(
            "The second directory check failed with status {}",
            String::from_utf8_lossy(status)
        ));
    }
    let marker = [MARKER.as_bytes(), b"\n"].concat();
    let after = stderr
        .windows(marker.len())
        .position(|window| window == marker)
        .map_or(&stderr[stderr.len()..], |at| &stderr[at + marker.len()..]);
    let parts = split_stderr(after)?;
    if !parts.diagnostics.is_empty() {
        return Err(format!(
            "The second directory check reported: {}",
            String::from_utf8_lossy(&parts.diagnostics)
        ));
    }
    let [group] = <[Vec<DirCheck>; 1]>::try_from(parts.groups)
        .map_err(|_| "The second directory check output is broken".to_string())?;
    Ok(Some(group))
}

/// もう一度尋ねた答えを入れる。子が見つかった・もう空と確かめられないディレクトリは変わった
/// ものにする。exclude に当たる子（find が刈り込んだもの）は数えない。`question_paths` は問いの
/// 番号ごとの root_dir からのパス
pub fn apply_recheck(
    answers: &mut [Answer],
    empty: &[EmptyDir],
    checks: &[DirCheck],
    question_paths: &[&[u8]],
    exclude: &Exclude,
) -> Result<(), String> {
    let mismatch = |what: &str, path: &[u8]| {
        format!(
            "The second directory check does not match the question ({what}): {}",
            String::from_utf8_lossy(path)
        )
    };
    let mut by_printed: HashMap<&[u8], &DirCheck> = HashMap::new();
    for check in checks {
        if by_printed.insert(&check.printed, check).is_some() {
            return Err(mismatch("twice", &check.printed));
        }
    }
    let asked: HashSet<&[u8]> = empty.iter().map(|dir| dir.printed.as_slice()).collect();
    if let Some(extra) = checks
        .iter()
        .find(|c| !asked.contains(c.printed.as_slice()))
    {
        return Err(mismatch("not asked", &extra.printed));
    }
    for dir in empty {
        let check = by_printed
            .get(dir.printed.as_slice())
            .ok_or_else(|| mismatch("missing", &dir.printed))?;
        let base = join_relative(question_paths[dir.answer], &dir.rel);
        let has_child = check
            .names
            .iter()
            .any(|name| !exclude.excludes(&join_relative(&base, name)));
        if check.gone || !check.readable || !check.searchable || has_child {
            answers[dir.answer]
                .subtree
                .as_mut()
                .ok_or_else(|| mismatch("no subtree", &dir.printed))?
                .changed
                .push(dir.rel.clone());
        }
    }
    Ok(())
}
