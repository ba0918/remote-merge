//! 走査の規則の駆動部。起点と辿るリンクを問いにして集め方にまとめて渡し（C7）、答えを
//! [`Walk`] で一覧にする。root_dir の確認は、どの起点でも最初の問いの一つとして一度だけ行う。

use super::exclude::Exclude;
use super::outcome::{Listing, ScanError, ScanFailure, ScanRequest, ScanScope};
use super::raw::{Answer, Collector, Depth, DirId, Question, RawKind, Unreached};
use super::start_paths::{start_paths, StartPath};
use super::walk::{PendingLink, Walk};

pub fn scan(
    collector: &mut dyn Collector,
    exclude: &Exclude,
    request: &ScanRequest,
) -> Result<Listing, ScanError> {
    let (whole, starts) = match &request.scope {
        ScanScope::Whole => (true, Vec::new()),
        ScanScope::Starts(starts) => start_paths(starts, exclude),
        ScanScope::Children(path) => {
            return super::children::scan_children(collector, exclude, path, request.max_entries)
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
        if start.with_self {
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
            walk.take_link(link, answer)?;
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
        Err(_) => Err(ScanFailure::RootUnreadable),
        Ok(entry) if entry.kind == RawKind::Directory => Ok(entry.id),
        Ok(_) => Err(ScanFailure::RootUnreadable),
    }
}

/// 起点に届かなかった理由を、ないなら Ok に、ほかは誤りにする
pub(super) fn unreached(path: &[u8], why: Unreached) -> Result<(), ScanError> {
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
    // 値そのものが symlink なら、同じ問いの答えでそのリンク先を決める
    match walk.take_leaf(path, entry, vec![root], counter)? {
        Some(link) => walk.take_link(link, answer),
        None => Ok(()),
    }
}

/// 指定したディレクトリの下だけを載せる（TBL-scan-001）。起点がディレクトリ（かディレクトリへの
/// リンク）なら、空でも分かるようにそれ自身を器のディレクトリとして載せる
fn take_below(
    walk: &mut Walk,
    start: &StartPath,
    answer: &Answer,
    root: DirId,
) -> Result<(), ScanError> {
    let path = start.path.as_slice();
    let entry = match &answer.entry {
        Err(_) if start.excluded => return Ok(()),
        Err(why) => return unreached(path, *why),
        Ok(entry) => entry,
    };
    let is_dir = match entry.kind {
        RawKind::Directory => true,
        RawKind::Symlink { .. } => {
            if answer.link != Some(super::raw::LinkTarget::Directory) {
                if start.excluded {
                    return Ok(());
                }
                let link = PendingLink::new(path.to_vec(), vec![root], start.group);
                return walk.take_link(link, answer);
            }
            true
        }
        _ => false,
    };
    if !is_dir {
        return Ok(());
    }
    let tree = answer.subtree.as_ref();
    walk.list_container(path, tree.map_or(entry, |tree| &tree.start))?;
    match (start.descend, tree) {
        (false, _) => Ok(()),
        (true, Some(tree)) => walk.take_subtree(path, tree, &[root], start.group),
        (true, None) => Err(missing_subtree()),
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
