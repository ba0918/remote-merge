//! 走査の規則の駆動部。起点と辿るリンクを問いにして集め方にまとめて渡し（C7）、答えを
//! [`Walk`] で一覧にする。root_dir の確認は、どの起点でも最初の問いの一つとして一度だけ行う。

use super::exclude::Exclude;
use super::outcome::{Listing, ScanError, ScanFailure, ScanRequest, ScanScope};
use super::raw::{Answer, Collector, DirId, Question, RawKind, Unreached};
use super::walk::{PendingLink, Walk};

pub fn scan(
    collector: &mut dyn Collector,
    exclude: &Exclude,
    request: &ScanRequest,
) -> Result<Listing, ScanError> {
    let (whole, starts) = start_paths(&request.scope, exclude);
    let counters = starts.iter().map(|s| s.group + 1).max().unwrap_or(1);
    let mut questions = vec![Question {
        path: Vec::new(),
        descend: whole,
    }];
    questions.extend(starts.iter().map(|start| Question {
        path: start.path.clone(),
        descend: true,
    }));
    let answers = ask(collector, &questions, exclude)?;
    let root_id = root_identity(&answers[0])?;
    let mut walk = Walk::new(exclude, request.max_entries, counters);
    if whole {
        let tree = answers[0].subtree.as_ref().ok_or_else(missing_subtree)?;
        walk.take_subtree(b"", tree, &[], 0)?;
    }
    for (start, answer) in starts.iter().zip(&answers[1..]) {
        if start.with_self {
            take_value(&mut walk, &start.path, answer, root_id, start.group)?;
        } else {
            take_below(&mut walk, &start.path, answer, root_id, start.group)?;
        }
    }
    while !walk.pending.is_empty() && !walk.truncated {
        let pending = std::mem::take(&mut walk.pending);
        let questions: Vec<Question> = pending
            .iter()
            .map(|link| Question {
                path: link.path.clone(),
                descend: true,
            })
            .collect();
        let answers = ask(collector, &questions, exclude)?;
        for (link, answer) in pending.into_iter().zip(&answers) {
            walk.take_link(link, answer)?;
        }
    }
    Ok(walk.finish())
}

fn ask(
    collector: &mut dyn Collector,
    questions: &[Question],
    exclude: &Exclude,
) -> Result<Vec<Answer>, ScanError> {
    let answers = collector.collect(questions, exclude)?;
    if answers.len() != questions.len() {
        return Err(ScanError::Collect(anyhow::anyhow!(
            "The scan returned {} answers for {} questions",
            answers.len(),
            questions.len()
        )));
    }
    Ok(answers)
}

fn missing_subtree() -> ScanError {
    ScanError::Collect(anyhow::anyhow!(
        "The scan returned no entries below a directory"
    ))
}

/// root_dir がないか読めなければ止める（REQ-scan-015）
fn root_identity(answer: &Answer) -> Result<DirId, ScanFailure> {
    match &answer.entry {
        Err(Unreached::Missing) => Err(ScanFailure::RootMissing),
        Err(_) => Err(ScanFailure::RootUnreadable),
        Ok(entry) if entry.kind == RawKind::Directory => Ok(entry.id),
        Ok(_) => Err(ScanFailure::RootUnreadable),
    }
}

/// 起点に届かなかった理由を、ないなら Ok に、ほかは誤りにする
fn unreached(path: &[u8], why: Unreached) -> Result<(), ScanError> {
    let path = String::from_utf8_lossy(path).into_owned();
    match why {
        Unreached::Missing => Ok(()),
        Unreached::NotDirectory => Err(ScanFailure::StartBlocked { path }.into()),
        Unreached::Unreadable => Err(ScanFailure::StartUnreadable { path }.into()),
    }
}

/// include の値そのものと、その下を載せる（docs/ir/config/include.md）
fn take_value(
    walk: &mut Walk,
    path: &[u8],
    answer: &Answer,
    root: DirId,
    counter: usize,
) -> Result<(), ScanError> {
    let entry = match &answer.entry {
        Err(why) => return unreached(path, *why),
        Ok(entry) => entry,
    };
    if entry.kind == RawKind::Directory {
        let tree = answer.subtree.as_ref().ok_or_else(missing_subtree)?;
        return walk.take_subtree(path, tree, &[root], counter);
    }
    walk.take_leaf(path, entry, vec![root], counter)?;
    match walk.pending.pop() {
        Some(link) => walk.take_link(link, answer),
        None => Ok(()),
    }
}

/// 指定したディレクトリの下だけを載せる（TBL-scan-001）
fn take_below(
    walk: &mut Walk,
    path: &[u8],
    answer: &Answer,
    root: DirId,
    counter: usize,
) -> Result<(), ScanError> {
    let entry = match &answer.entry {
        Err(why) => return unreached(path, *why),
        Ok(entry) => entry,
    };
    let is_dir = match entry.kind {
        RawKind::Directory => true,
        RawKind::Symlink { .. } => {
            let link = PendingLink::new(path.to_vec(), vec![root], counter);
            if answer.link != Some(super::raw::LinkTarget::Directory) {
                return walk.take_link(link, answer);
            }
            true
        }
        _ => false,
    };
    match (is_dir, &answer.subtree) {
        (false, _) => Ok(()),
        (true, Some(tree)) => walk.take_subtree(path, tree, &[root], counter),
        (true, None) => Err(missing_subtree()),
    }
}

/// 整えた起点
struct StartPath {
    path: Vec<u8>,
    with_self: bool,
    group: usize,
}

/// 起点のパスを整え、exclude に当たるものと、ほかの起点の下にあるものを除く（同じパスなら自身も
/// 載せる起点を残す）。root_dir 全体を指す起点があれば全体の走査にする。返す値の一つ目は全体の
/// 走査か
fn start_paths(scope: &ScanScope, exclude: &Exclude) -> (bool, Vec<StartPath>) {
    let starts = match scope {
        ScanScope::Whole => return (true, Vec::new()),
        ScanScope::Starts(starts) => starts,
    };
    let mut normal: Vec<StartPath> = starts
        .iter()
        .map(|start| StartPath {
            path: super::starts::normalize(&start.path).into_bytes(),
            with_self: start.with_self,
            group: start.group,
        })
        .collect();
    if normal.iter().any(|start| start.path.is_empty()) {
        return (true, Vec::new());
    }
    normal.sort_by(|a, b| a.path.cmp(&b.path).then(b.with_self.cmp(&a.with_self)));
    let mut kept: Vec<StartPath> = Vec::new();
    for start in normal {
        let covered = kept.iter().any(|outer| {
            outer.path == start.path
                || (start.path.starts_with(&outer.path)
                    && start.path.get(outer.path.len()) == Some(&b'/'))
        });
        if !covered && !exclude.excludes(&start.path) {
            kept.push(start);
        }
    }
    (false, kept)
}

#[cfg(test)]
#[path = "drive_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "drive_limit_tests.rs"]
mod limit_tests;

#[cfg(test)]
#[path = "drive_scope_tests.rs"]
mod scope_tests;
