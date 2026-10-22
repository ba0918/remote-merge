//! 走査の規則の駆動部。起点と辿るリンクを問いにして集め方にまとめて渡し（C7）、答えを
//! `Walk` で一覧にする。root_dir の確認は、どの起点でも最初の問いの一つとして一度だけ行う。

use super::exclude::Exclude;
use super::outcome::{Listing, ScanError, ScanFailure, ScanRequest, ScanScope};
use super::raw::{Answer, Collector, Depth, DirId, Question, RawKind, Unreached};
use super::start_paths::start_paths;
use super::start_take::{take_below, take_itself, take_value};
use super::walk::Walk;

pub fn scan(
    collector: &mut dyn Collector,
    exclude: &Exclude,
    request: &ScanRequest,
) -> Result<Listing, ScanError> {
    let (whole, starts) = match &request.scope {
        ScanScope::Whole => (true, Vec::new()),
        ScanScope::Starts(starts) => start_paths(starts, exclude),
        ScanScope::Children(path) => {
            return super::children::scan_children(collector, exclude, path, request.max_entries);
        }
    };
    let counters = starts.iter().map(|s| s.group + 1).max().unwrap_or(1);
    let mut questions = vec![Question {
        path: Vec::new(),
        depth: if whole { Depth::All } else { Depth::Itself },
    }];
    questions.extend(starts.iter().map(|start| Question {
        path: start.path.clone(),
        depth: if start.descend {
            Depth::All
        } else {
            Depth::Itself
        },
    }));
    let answers = ask(collector, &questions, exclude)?;
    let root_id = root_identity(&answers[0])?;
    let mut walk = Walk::new(exclude, request.max_entries, counters);
    if whole {
        let tree = answers[0].subtree.as_ref().ok_or_else(missing_subtree)?;
        walk.take_subtree(b"", tree, &[], 0)?;
    }
    for (start, answer) in starts.iter().zip(&answers[1..]) {
        if start.with_self && !start.descend {
            take_itself(&mut walk, &start.path, answer, root_id, start.group)?;
        } else if start.with_self {
            take_value(&mut walk, &start.path, answer, root_id, start.group)?;
        } else {
            take_below(&mut walk, start, answer, root_id)?;
        }
    }
    while !walk.pending.is_empty() && !walk.stopped() {
        let pending = std::mem::take(&mut walk.pending);
        let questions: Vec<Question> = pending
            .iter()
            .map(|link| Question {
                path: link.path.clone(),
                depth: Depth::All,
            })
            .collect();
        let answers = ask(collector, &questions, exclude)?;
        for (link, answer) in pending.into_iter().zip(&answers) {
            walk.take_link(&link, answer)?;
        }
    }
    Ok(walk.finish())
}

pub(super) fn ask(
    collector: &mut dyn Collector,
    questions: &[Question],
    exclude: &Exclude,
) -> Result<Vec<Answer>, ScanError> {
    let answers = collector.collect(questions, exclude)?;
    if answers.len() != questions.len() {
        return Err(ScanFailure::InconsistentAnswer {
            detail: format!(
                "{} answers for {} questions",
                answers.len(),
                questions.len()
            ),
        }
        .into());
    }
    Ok(answers)
}

pub(super) fn missing_subtree() -> ScanError {
    ScanFailure::InconsistentAnswer {
        detail: "no entries below a directory".into(),
    }
    .into()
}

/// root_dir がないか読めなければ止める（REQ-scan-015）
pub(super) fn root_identity(answer: &Answer) -> Result<DirId, ScanFailure> {
    match &answer.entry {
        Err(Unreached::Missing) => Err(ScanFailure::RootMissing),
        Err(Unreached::NotDirectory) => Err(ScanFailure::RootNotDirectory),
        Err(Unreached::Unreadable(cause)) => Err(ScanFailure::RootUnreadable { cause: *cause }),
        Ok(entry) if entry.kind == RawKind::Directory => Ok(entry.id),
        Ok(_) => Err(ScanFailure::RootNotDirectory),
    }
}

/// 起点に届かなかった理由を、ないなら Ok に、ほかは誤りにする
pub(super) fn unreached_start(path: &[u8], why: Unreached) -> Result<(), ScanError> {
    let path = String::from_utf8_lossy(path).into_owned();
    match why {
        Unreached::Missing => Ok(()),
        Unreached::NotDirectory => Err(ScanFailure::StartBlocked { path }.into()),
        Unreached::Unreadable(cause) => Err(ScanFailure::StartUnreadable { path, cause }.into()),
    }
}

#[cfg(test)]
#[path = "drive_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "drive_limit_tests.rs"]
mod limit_tests;

#[cfg(test)]
#[path = "drive_name_tests.rs"]
mod name_tests;

#[cfg(test)]
#[path = "drive_include_tests.rs"]
mod include_tests;

#[cfg(test)]
#[path = "drive_scope_tests.rs"]
mod scope_tests;

#[cfg(test)]
#[path = "drive_changed_tests.rs"]
mod changed_tests;

#[cfg(test)]
#[path = "drive_kind_tests.rs"]
mod kind_tests;

#[cfg(test)]
#[path = "drive_itself_tests.rs"]
mod itself_tests;
