//! このマシンの生の項目を集める（ローカルとエージェントの経路が使う。C6）。何を載せるかは
//! 決めず、exclude に当たるディレクトリに降りないことだけを規則の [`Exclude`] で決める
//! （REQ-config-044）。
use std::path::{Path, PathBuf};

use super::exclude::Exclude;
use super::machine_fs::{
    bytes_of, cause_of, entry_at, entry_with_metadata, link_target, path_of, raw_entry,
    root_went_away, went_away, why_unreached,
};
use super::parts::join;
use super::raw::{
    Answer, CollectError, Collector, Depth, LinkTarget, Question, RawKind, ReadCause, Subtree,
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
            return self.answer_root(question.depth, exclude);
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
        let subtree = (is_dir && question.depth != Depth::Itself)
            .then(|| collect_below(&full, &question.path, question.depth, exclude));
        Answer {
            entry,
            link,
            subtree,
        }
    }

    /// root_dir はリンクを辿って読む（REQ-scan-007）。ディレクトリなら入れるかも確かめ、入れなければ
    /// 読めない（検索の権限がなければ権限がない）と答える。SSH の経路の走査が root_dir に cd するのと
    /// 同じ確かめで、どの範囲の走査でも root_dir の誤りを同じにする（docs/decision/records/
    /// 2026-10-12-foundation-4.md の E42）
    fn answer_root(&self, depth: Depth, exclude: &Exclude) -> Answer {
        let entry = std::fs::metadata(&self.root)
            .and_then(|meta| {
                if meta.is_dir() {
                    std::fs::metadata(self.root.join("."))?;
                }
                Ok(meta)
            })
            .map(|meta| raw_entry(Vec::new(), &meta, None))
            .map_err(|e| why_unreached(&e));
        let is_dir = matches!(&entry, Ok(entry) if entry.kind == RawKind::Directory);
        let subtree = (is_dir && depth != Depth::Itself)
            .then(|| collect_below(&self.root, b"", depth, exclude));
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
        let was_dir = self.root.is_dir();
        let answers = questions.iter().map(|q| self.answer(q, exclude)).collect();
        if root_went_away(was_dir, &self.root) {
            return Err(CollectError::RootGone);
        }
        Ok(answers)
    }
}

/// リンクを辿ったディレクトリ `dir` の下を、中の symlink を辿らずに集める。`logical` は
/// root_dir からの `dir` のパスで、exclude を当てるのに使う。`depth` が `Children` なら子の
/// ディレクトリは読まない
fn collect_below(dir: &Path, logical: &[u8], depth: Depth, exclude: &Exclude) -> Subtree {
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
            let child_path = child.path();
            match std::fs::symlink_metadata(&child_path) {
                Err(error) => child_unreadable(&mut tree, child_rel, &error),
                Ok(meta) => {
                    let entry = entry_with_metadata(&child_path, child_rel.clone(), &meta);
                    if entry.kind == RawKind::Directory && depth == Depth::All {
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

/// 情報を読めなかった子の答え。一覧に出たのにもうないなら、走査の途中で変わったもの
fn child_unreadable(tree: &mut Subtree, child_rel: Vec<u8>, error: &std::io::Error) {
    if error.kind() == std::io::ErrorKind::NotFound {
        tree.changed.push(child_rel);
        return;
    }
    tree.unreadable_entries
        .push(UnreadableDir::new(child_rel, cause_of(error)));
}

#[cfg(all(test, unix))]
#[path = "this_machine_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "this_machine_changed_tests.rs"]
mod changed_tests;
