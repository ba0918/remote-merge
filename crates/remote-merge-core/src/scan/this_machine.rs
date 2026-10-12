//! このマシンの生の項目を集める（ローカルとエージェントの経路が使う。C6）。何を載せるかは
//! 決めず、exclude に当たるディレクトリに降りないことだけを規則の [`Exclude`] で決める
//! （REQ-config-044）。
use std::path::{Path, PathBuf};

use super::exclude::Exclude;
use super::machine_fs::{bytes_of, entry_at, link_target, path_of, raw_entry, unreached};
use super::parts::join;
use super::raw::{
    Answer, CollectError, Collector, LinkTarget, Question, RawKind, ReadCause, Subtree,
    UnreadableDir,
};

pub struct MachineCollector {
    root: PathBuf,
}

impl MachineCollector {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn answer(&self, question: &Question, exclude: &Exclude) -> Answer {
        if question.path.is_empty() {
            return self.answer_root(question.descend, exclude);
        }
        let full = self.root.join(path_of(&question.path));
        let entry = entry_at(&full, Vec::new());
        let link = match &entry {
            Ok(entry) if matches!(entry.kind, RawKind::Symlink { .. }) => Some(link_target(&full)),
            _ => None,
        };
        let is_dir = match (&entry, &link) {
            (Ok(entry), None) => entry.kind == RawKind::Directory,
            (_, link) => *link == Some(LinkTarget::Directory),
        };
        let subtree =
            (question.descend && is_dir).then(|| collect_below(&full, &question.path, exclude));
        Answer {
            entry,
            link,
            subtree,
        }
    }

    /// root_dir はリンクを辿って読む（REQ-scan-007）
    fn answer_root(&self, descend: bool, exclude: &Exclude) -> Answer {
        let entry = std::fs::metadata(&self.root)
            .map(|meta| raw_entry(Vec::new(), &meta, None))
            .map_err(|e| unreached(&e));
        let is_dir = matches!(&entry, Ok(entry) if entry.kind == RawKind::Directory);
        let subtree = (descend && is_dir).then(|| collect_below(&self.root, b"", exclude));
        Answer {
            entry,
            link: None,
            subtree,
        }
    }
}

impl Collector for MachineCollector {
    fn collect(
        &mut self,
        questions: &[Question],
        exclude: &Exclude,
    ) -> Result<Vec<Answer>, CollectError> {
        Ok(questions.iter().map(|q| self.answer(q, exclude)).collect())
    }
}

/// リンクを辿ったディレクトリ `dir` の下を、中の symlink を辿らずに集める。`logical` は
/// root_dir からの `dir` のパスで、exclude を当てるのに使う
fn collect_below(dir: &Path, logical: &[u8], exclude: &Exclude) -> Subtree {
    let mut tree = Subtree {
        start: match std::fs::metadata(dir) {
            Ok(meta) => raw_entry(Vec::new(), &meta, None),
            Err(error) => {
                return unreadable_start(dir, &error);
            }
        },
        entries: Vec::new(),
        unreadable: Vec::new(),
        unreadable_entries: Vec::new(),
        changed: Vec::new(),
    };
    let mut stack: Vec<Vec<u8>> = vec![Vec::new()];
    while let Some(rel) = stack.pop() {
        let read = std::fs::read_dir(dir.join(path_of(&rel)));
        let children = match read {
            Ok(children) => children,
            Err(error) if went_away(&error) => {
                tree.changed.push(rel);
                continue;
            }
            Err(error) => {
                tree.unreadable
                    .push(UnreadableDir::new(rel, cause_of(&error)));
                continue;
            }
        };
        for child in children {
            let child = match child {
                Ok(child) => child,
                Err(error) => {
                    tree.unreadable
                        .push(UnreadableDir::new(rel.clone(), cause_of(&error)));
                    break;
                }
            };
            let child_rel = join(&rel, &bytes_of(&child.file_name()));
            if exclude.excludes(&join(logical, &child_rel)) {
                continue;
            }
            // 情報を読めない項目は、その項目自身のパスで答える（どう扱うかは規則が決める）
            match std::fs::symlink_metadata(child.path()) {
                Err(error) => {
                    tree.unreadable_entries
                        .push(UnreadableDir::new(child_rel, cause_of(&error)));
                }
                Ok(_) => {
                    let Ok(entry) = entry_at(&child.path(), child_rel.clone()) else {
                        tree.unreadable_entries
                            .push(UnreadableDir::new(child_rel, ReadCause::Other));
                        continue;
                    };
                    if entry.kind == RawKind::Directory {
                        stack.push(child_rel);
                    }
                    tree.entries.push(entry);
                }
            }
        }
    }
    tree
}

/// 起点を辿れたのに読めない（リンク先が変わった）。もうないかディレクトリでないなら変わった
/// もの、そうでなければ起点そのものを読めないものとする
fn unreadable_start(dir: &Path, error: &std::io::Error) -> Subtree {
    let start = entry_at(dir, Vec::new()).unwrap_or_else(|_| super::raw::RawEntry {
        path: Vec::new(),
        kind: RawKind::Directory,
        size: 0,
        mtime: super::raw::RawTime::Unreadable,
        mode: 0,
        id: super::raw::DirId { dev: 0, ino: 0 },
    });
    let (unreadable, changed) = if went_away(error) {
        (Vec::new(), vec![Vec::new()])
    } else {
        (
            vec![UnreadableDir::new(Vec::new(), ReadCause::Other)],
            Vec::new(),
        )
    };
    Subtree {
        start,
        entries: Vec::new(),
        unreadable,
        unreadable_entries: Vec::new(),
        changed,
    }
}

/// 読みに行ったディレクトリが、もうないかディレクトリでなくなっていた（走査の途中で変わった）
fn went_away(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

/// ディレクトリを読めなかった理由
fn cause_of(error: &std::io::Error) -> ReadCause {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        ReadCause::PermissionDenied
    } else {
        ReadCause::Other
    }
}

#[cfg(all(test, unix))]
#[path = "this_machine_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "this_machine_changed_tests.rs"]
mod changed_tests;
