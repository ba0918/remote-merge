//! 集めた生の項目を一覧に載せる規則。exclude・件数・UTF-8 でない名前・循環・辿るか・読めない
//! ときに止めるかをここで決める（docs/ir/scan/ と docs/ir/config/filters.md）。

use std::collections::HashMap;

use super::exclude::Exclude;
use super::outcome::{ListedEntry, ListedKind, Listing, ScanError, ScanFailure};
pub(super) use super::parts::PendingLink;
use super::parts::{join, listed_entry, parent_ids, shown, unreadable_directory};
use super::raw::{DirId, RawEntry, RawKind, ReadCause, Subtree};

pub(super) struct Walk<'a> {
    exclude: &'a Exclude,
    max_entries: usize,
    counts: Vec<usize>,
    pub(super) entries: Vec<ListedEntry>,
    unreadable_names: Vec<String>,
    pub truncated: bool,
    /// 循環に出会ったパス（不完全な一覧。REQ-scan-002）
    pub cycle: Option<String>,
    pub pending: Vec<PendingLink>,
}

impl<'a> Walk<'a> {
    pub fn new(exclude: &'a Exclude, max_entries: usize, counters: usize) -> Self {
        Self {
            exclude,
            max_entries,
            counts: vec![0; counters.max(1)],
            entries: Vec::new(),
            unreadable_names: Vec::new(),
            truncated: false,
            cycle: None,
            pending: Vec::new(),
        }
    }

    /// 件数の上限か循環で、これより先を集めない
    pub fn stopped(&self) -> bool {
        self.truncated || self.cycle.is_some()
    }

    pub fn finish(mut self) -> Listing {
        self.entries.sort_by(|a, b| a.path.cmp(&b.path));
        self.unreadable_names.sort();
        Listing {
            entries: self.entries,
            unreadable_names: self.unreadable_names,
            truncated: self.truncated,
            cycle: self.cycle,
        }
    }

    /// 件数の上限の内なら数えて true。超えたら打ち切りにして false（REQ-scan-010・004）
    fn count(&mut self, counter: usize) -> bool {
        self.counts[counter] += 1;
        if self.counts[counter] > self.max_entries {
            self.truncated = true;
        }
        !self.truncated
    }

    /// 起点 `base` の下の項目を載せる。`ancestors` は `base` の親までのディレクトリの番号
    pub fn take_subtree(
        &mut self,
        base: &[u8],
        tree: &Subtree,
        ancestors: &[DirId],
        counter: usize,
    ) -> Result<(), ScanError> {
        if ancestors.contains(&tree.start.id) {
            self.cycle = Some(shown(base));
            return Ok(());
        }
        let unreadable: HashMap<&[u8], ReadCause> = tree
            .unreadable
            .iter()
            .map(|dir| (dir.path.as_slice(), dir.cause))
            .collect();
        if let Some(cause) = unreadable.get(&b""[..]) {
            return Err(unreadable_directory(base, *cause).into());
        }
        let mut dir_ids: HashMap<&[u8], DirId> = HashMap::from([(&b""[..], tree.start.id)]);
        for entry in &tree.entries {
            if entry.kind == RawKind::Directory {
                dir_ids.insert(&entry.path, entry.id);
            }
        }
        let mut sorted: Vec<&RawEntry> = tree.entries.iter().collect();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));
        for entry in sorted {
            if self.stopped() {
                return Ok(());
            }
            let path = join(base, &entry.path);
            if entry.path.is_empty() || self.exclude.excludes(&path) {
                continue;
            }
            if entry.kind == RawKind::Directory {
                // リンクを辿った先で、辿ってきた祖先のディレクトリにまた出会った（REQ-scan-002）
                if ancestors.contains(&entry.id) {
                    self.cycle = Some(shown(&path));
                    return Ok(());
                }
                if let Some(cause) = unreadable.get(entry.path.as_slice()) {
                    return Err(unreadable_directory(&path, *cause).into());
                }
                self.list_directory(&path, entry)?;
                continue;
            }
            let mut chain = ancestors.to_vec();
            chain.extend(parent_ids(&entry.path, &dir_ids));
            if let Some(link) = self.take_leaf(&path, entry, chain, counter)? {
                self.pending.push(link);
            }
        }
        Ok(())
    }

    /// 指定したディレクトリの起点を器のディレクトリとして載せる
    pub fn list_container(&mut self, path: &[u8], entry: &RawEntry) -> Result<(), ScanError> {
        self.list_directory(path, entry)
    }

    /// ディレクトリは数えずに載せる。名前が UTF-8 として読めなければ載せず、下の項目で報告する
    fn list_directory(&mut self, path: &[u8], entry: &RawEntry) -> Result<(), ScanError> {
        if let Ok(text) = std::str::from_utf8(path) {
            self.entries
                .push(listed_entry(text, entry, ListedKind::Directory)?);
        }
        Ok(())
    }

    /// ファイル・symlink・そのほかの項目を数えて載せる。symlink なら、辿るかを決めるために
    /// 待たせるリンクを返す（止まった後や件数の上限を超えたときは載せず、何も返さない）
    pub fn take_leaf(
        &mut self,
        path: &[u8],
        entry: &RawEntry,
        ancestors: Vec<DirId>,
        counter: usize,
    ) -> Result<Option<PendingLink>, ScanError> {
        if self.stopped() || !self.count(counter) {
            return Ok(None);
        }
        let target = match &entry.kind {
            RawKind::Symlink { target: None } => {
                return Err(ScanFailure::UnreadableLinkTarget { path: shown(path) }.into())
            }
            RawKind::Symlink {
                target: Some(target),
            } => Some(target),
            _ => None,
        };
        let text = std::str::from_utf8(path).ok();
        // UTF-8 として読めないリンク先は置換文字にして載せ、merge が変わっていないことを確かめ
        // られないものとして扱う（docs/ir/merge/concurrency.md の REQ-merge-033）
        let kind = match target {
            None => ListedKind::File,
            Some(target) => ListedKind::Symlink {
                target: String::from_utf8_lossy(target).into_owned(),
                is_dir: false,
            },
        };
        let listed = match text {
            Some(text) => {
                self.entries.push(listed_entry(text, entry, kind)?);
                Some(self.entries.len() - 1)
            }
            None => {
                self.unreadable_names.push(shown(path));
                None
            }
        };
        Ok(target.map(|_| PendingLink {
            path: path.to_vec(),
            ancestors,
            counter,
            listed,
        }))
    }
}
