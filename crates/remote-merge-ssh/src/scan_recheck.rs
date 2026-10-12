//! find が子を一つも出さなかったのに、確かめで読めて検索できると答えたディレクトリに、本当に
//! 空かをもう一度まとめて尋ねる（docs/decision/records/2026-10-12-foundation-2.md の C14）。
//!
//! findutils 4.4.2 は検索できないディレクトリの子を知らせずに出さないので、find が通った後で
//! 確かめの前に検索できるようになると、子のあるディレクトリが空に見える。もう一度尋ねて子が
//! あれば、走査の途中で変わったものとして答える。find の版では分けない。

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::raw::{join_relative, Answer, Subtree};

use crate::scan_checks::{check_script, relative, split_stderr};
use crate::scan_diagnostics::DirCheck;
use crate::scan_script::{assign, root_setup, split_by_length, wrapped_len, MARKER};

/// もう一度尋ねるディレクトリ。`answer` は問いの番号、`rel` は問いのパスからの相対パス、
/// `printed` は find に渡した形のパス
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyDir {
    pub answer: usize,
    pub rel: Vec<u8>,
    pub printed: Vec<u8>,
}

/// 読めて検索できると確かめたのに、find が子を一つも出さなかったディレクトリ（相対パスと
/// find に渡した形のパス）
pub fn empty_dirs(tree: &Subtree, checks: &[DirCheck], start: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let parents: HashSet<&[u8]> = tree
        .entries
        .iter()
        .map(|entry| entry.path.as_slice())
        .chain(
            tree.unreadable_entries
                .iter()
                .map(|item| item.path.as_slice()),
        )
        .map(parent_of)
        .collect();
    checks
        .iter()
        .filter(|check| check.readable && check.searchable && !check.gone)
        .filter_map(|check| {
            let rel = relative(&check.printed, start)?;
            (!parents.contains(rel.as_slice())).then(|| (rel, check.printed.clone()))
        })
        .collect()
}

fn parent_of(path: &[u8]) -> &[u8] {
    path.iter()
        .rposition(|byte| *byte == b'/')
        .map_or(&path[..0], |at| &path[..at])
}

/// root_dir `root` に移り、`printed` のディレクトリそれぞれの子の名前を、確かめと同じ形で
/// 標準エラーに出すスクリプト
pub fn recheck_script(root: &[u8], printed: &[&[u8]]) -> String {
    let mut script =
        format!("LC_ALL=C; export LC_ALL\nprintf '{MARKER}\\000'\nprintf '{MARKER}\\n' >&2\n");
    script.push_str(&check_script(true));
    script.push_str(&root_setup(root));
    let args: Vec<(String, String)> = printed
        .iter()
        .enumerate()
        .map(|(index, path)| dir_arg(index, path))
        .collect();
    for (setup, _) in &args {
        script.push_str(setup);
    }
    script.push_str("printf '\\000@QS\\000' >&2; sh -c \"$c\" sh");
    for (_, arg) in &args {
        script.push_str(" \\\n ");
        script.push_str(arg);
    }
    script.push_str(concat!(
        "\n",
        "printf '@R\\000%s\\000' \"$?\"\n",
        "printf '@END\\000'\n",
    ));
    script
}

/// 一つのディレクトリを sh に渡す引数と、その前に置く文。表示できる ASCII で "'" を含まない
/// パスはそのまま引用し、ほかは変数に入れてから渡す（ディレクトリごとにコマンド置換で
/// プロセスを起こすと、数千のディレクトリで秒の単位で遅くなる）
fn dir_arg(index: usize, path: &[u8]) -> (String, String) {
    let plain = path
        .iter()
        .all(|byte| (b' '..=b'~').contains(byte) && *byte != b'\'');
    if plain {
        return (
            String::new(),
            format!("'{}'", String::from_utf8_lossy(path)),
        );
    }
    let name = format!("a{index}");
    (assign(&name, path), format!("\"${name}\""))
}

fn dir_line(index: usize, path: &[u8]) -> String {
    let (setup, arg) = dir_arg(index, path);
    format!("{setup} \\\n {arg}")
}

/// もう一度尋ねるディレクトリを、コマンド行が `limit` バイトに収まるまとまりに分ける
pub fn split_recheck(root: &[u8], printed: &[&[u8]], limit: usize) -> Vec<Range<usize>> {
    let base = wrapped_len(&recheck_script(root, &[]));
    let lines = printed
        .iter()
        .enumerate()
        .map(|(index, path)| dir_line(index, path));
    split_by_length(base, lines, limit)
}

/// もう一度尋ねた答えを読む。root_dir にもう入れなければ `None`
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
    if tokens.next() != Some(b"@END") {
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

#[cfg(all(test, unix))]
#[path = "scan_recheck_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "scan_agree_empty_tests.rs"]
mod agree_empty_tests;

#[cfg(all(test, unix))]
#[path = "scan_recheck_script_tests.rs"]
mod script_tests;
