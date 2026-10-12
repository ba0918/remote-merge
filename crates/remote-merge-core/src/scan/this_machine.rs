//! このマシンの生の項目を集める（ローカルとエージェントの経路が使う。C6）。何を載せるかは
//! 決めず、exclude に当たるディレクトリに降りないことだけを規則の [`Exclude`] で決める
//! （REQ-config-044）。
use std::path::{Path, PathBuf};

use super::exclude::Exclude;
use super::machine_fs::{bytes_of, entry_at, link_target, path_of, raw_entry, unreached};
use super::parts::join;
use super::raw::{Answer, CollectError, Collector, LinkTarget, Question, RawKind, Subtree};

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
            Err(_) => {
                return unreadable_start(dir);
            }
        },
        entries: Vec::new(),
        unreadable: Vec::new(),
    };
    let mut stack: Vec<Vec<u8>> = vec![Vec::new()];
    while let Some(rel) = stack.pop() {
        let read = std::fs::read_dir(dir.join(path_of(&rel)));
        let Ok(children) = read else {
            tree.unreadable.push(rel);
            continue;
        };
        for child in children {
            let Ok(child) = child else {
                tree.unreadable.push(rel.clone());
                break;
            };
            let child_rel = join(&rel, &bytes_of(&child.file_name()));
            if exclude.excludes(&join(logical, &child_rel)) {
                continue;
            }
            match entry_at(&child.path(), child_rel.clone()) {
                Ok(entry) => {
                    if entry.kind == RawKind::Directory {
                        stack.push(child_rel);
                    }
                    tree.entries.push(entry);
                }
                Err(_) => tree.unreadable.push(rel.clone()),
            }
        }
    }
    tree
}

/// 起点を辿れたのに読めない（リンク先が変わった）。起点そのものを読めないものとする
fn unreadable_start(dir: &Path) -> Subtree {
    let start = entry_at(dir, Vec::new()).unwrap_or_else(|_| super::raw::RawEntry {
        path: Vec::new(),
        kind: RawKind::Directory,
        size: 0,
        mtime: super::raw::RawTime::Unreadable,
        mode: 0,
        id: super::raw::DirId { dev: 0, ino: 0 },
    });
    Subtree {
        start,
        entries: Vec::new(),
        unreadable: vec![Vec::new()],
    }
}

#[cfg(all(test, unix))]
#[path = "this_machine_tests.rs"]
mod tests;
