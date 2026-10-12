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
    let (scope, starts) = start_paths(&request.scope, exclude);
    let whole = matches!(scope, ScanScope::Whole);
    let counters = match scope {
        ScanScope::Below(_) => starts.len(),
        _ => 1,
    };
    let mut questions = vec![Question {
        path: Vec::new(),
        descend: whole,
    }];
    questions.extend(starts.iter().map(|path| Question {
        path: path.clone(),
        descend: true,
    }));
    let answers = ask(collector, &questions, exclude)?;
    let root_id = root_identity(&answers[0])?;
    let mut walk = Walk::new(exclude, request.max_entries, counters);
    if whole {
        let tree = answers[0].subtree.as_ref().ok_or_else(missing_subtree)?;
        walk.take_subtree(b"", tree, &[], 0)?;
    }
    for (index, (path, answer)) in starts.iter().zip(&answers[1..]).enumerate() {
        match scope {
            ScanScope::Below(_) => take_below(&mut walk, path, answer, root_id, index)?,
            _ => take_value(&mut walk, path, answer, root_id)?,
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
fn take_value(walk: &mut Walk, path: &[u8], answer: &Answer, root: DirId) -> Result<(), ScanError> {
    let entry = match &answer.entry {
        Err(why) => return unreached(path, *why),
        Ok(entry) => entry,
    };
    if entry.kind == RawKind::Directory {
        let tree = answer.subtree.as_ref().ok_or_else(missing_subtree)?;
        return walk.take_subtree(path, tree, &[root], 0);
    }
    walk.take_leaf(path, entry, vec![root], 0)?;
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

/// 起点のパスを整え、exclude に当たるものと、ほかの起点の下にあるものを除く。root_dir 全体を
/// 指すものがあれば全体の走査にする
fn start_paths(scope: &ScanScope, exclude: &Exclude) -> (ScanScope, Vec<Vec<u8>>) {
    let paths = match scope {
        ScanScope::Whole => return (ScanScope::Whole, Vec::new()),
        ScanScope::Values(paths) | ScanScope::Below(paths) => paths,
    };
    let mut normal: Vec<Vec<u8>> = paths.iter().map(|p| normalize(p).into_bytes()).collect();
    if normal.iter().any(Vec::is_empty) {
        return (ScanScope::Whole, Vec::new());
    }
    normal.sort();
    normal.dedup();
    let mut kept: Vec<Vec<u8>> = Vec::new();
    for path in normal {
        let nested = kept
            .iter()
            .any(|outer| path.starts_with(outer) && path.get(outer.len()) == Some(&b'/'));
        if !nested && !exclude.excludes(&path) {
            kept.push(path);
        }
    }
    (scope.clone(), kept)
}

fn normalize(path: &str) -> String {
    path.split('/')
        .filter(|element| !element.is_empty() && *element != ".")
        .collect::<Vec<_>>()
        .join("/")
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
