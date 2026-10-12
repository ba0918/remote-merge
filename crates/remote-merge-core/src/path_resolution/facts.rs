//! 尋ねて集めたパスの要素の事実から、[`resolve_path`] と同じ手順で辿る（docs/decision/records/
//! 2026-10-12-foundation-3.md の D5）。
//!
//! 事実の足りない要素に出会ったら、その要素を「検索できるディレクトリ」と仮定して最後まで辿り、
//! 手順が尋ねた事実のないパスを "/" に近い順に並べて返す。経路はそれをまとめて尋ね、答えを足して
//! もう一度辿る。仮定が外れるのは symlink のリンク文字列を差し込むときだけなので、尋ねる回数は
//! 辿る symlink の段の数に一を足したものまでになる。

use std::cell::RefCell;
use std::collections::HashMap;

use super::{resolve_path, EntryKind, PathOps, ResolveError, ResolvedPath};

/// パスの一つの要素の事実（リンクは辿らない）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathFact {
    Directory {
        /// 実行する利用者が検索できるか
        searchable: bool,
    },
    /// symlink と、読んだリンク文字列（読む道具がなければその誤り）
    Symlink(Result<Vec<u8>, ResolveError>),
    RegularFile,
    Other,
    Missing,
    /// あるかないかを決められない（ないと分かる文言でない理由で調べられなかった）
    Unreadable,
}

/// 実パスごとの事実
#[derive(Debug, Clone, Default)]
pub struct FactBook(HashMap<String, PathFact>);

impl FactBook {
    pub fn insert(&mut self, path: String, fact: PathFact) {
        self.0.insert(path, fact);
    }

    pub fn get(&self, path: &str) -> Option<&PathFact> {
        self.0.get(path)
    }
}

/// 次にすること
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// 辿り終えた
    Done(Result<ResolvedPath, ResolveError>),
    /// これらのパスの事実を "/" に近い順に尋ねる
    Ask(Vec<String>),
}

/// 事実のない要素を検索できるディレクトリと仮定し、仮定したパスを覚える基本操作
struct Assuming<'a> {
    facts: &'a FactBook,
    wanted: RefCell<Vec<String>>,
}

impl Assuming<'_> {
    fn want(&self, path: &str) {
        let mut wanted = self.wanted.borrow_mut();
        if !wanted.iter().any(|known| known == path) {
            wanted.push(path.to_string());
        }
    }
}

impl PathOps for Assuming<'_> {
    fn entry_kind(&self, path: &str) -> Result<EntryKind, ResolveError> {
        Ok(match self.facts.get(path) {
            Some(PathFact::Directory { .. }) => EntryKind::Directory,
            Some(PathFact::Symlink(_)) => EntryKind::Symlink,
            Some(PathFact::RegularFile) => EntryKind::RegularFile,
            Some(PathFact::Other) => EntryKind::Other,
            Some(PathFact::Missing) => EntryKind::Missing,
            Some(PathFact::Unreadable) => return Err(ResolveError::CannotRead),
            None => {
                self.want(path);
                EntryKind::Directory
            }
        })
    }

    fn can_search(&self, directory: &str) -> Result<bool, ResolveError> {
        match self.facts.get(directory) {
            Some(PathFact::Directory { searchable }) => Ok(*searchable),
            // 辿る手順はディレクトリだと分かった要素の中だけを検索する
            Some(_) => Ok(true),
            None => {
                self.want(directory);
                Ok(true)
            }
        }
    }

    fn read_link(&self, path: &str) -> Result<Vec<u8>, ResolveError> {
        match self.facts.get(path) {
            Some(PathFact::Symlink(target)) => target.clone(),
            _ => Err(ResolveError::CannotRead),
        }
    }
}

/// 絶対パスの root_dir の下の `rel` を、`facts` で辿る
pub fn next_step(root: &str, rel: &str, facts: &FactBook) -> Step {
    let ops = Assuming {
        facts,
        wanted: RefCell::new(Vec::new()),
    };
    let result = resolve_path(root, rel, &ops);
    let wanted = ops.wanted.into_inner();
    if wanted.is_empty() {
        Step::Done(result)
    } else {
        Step::Ask(wanted)
    }
}

#[cfg(test)]
#[path = "facts_tests.rs"]
mod tests;
