//! 集めた生の項目を一覧に載せる規則。exclude・件数・UTF-8 でない名前・循環・辿るか・読めない
//! ときに止めるかをここで決める（docs/ir/scan/ と docs/ir/config/filters.md）。

use std::collections::{HashMap, HashSet};

use super::exclude::Exclude;
use super::outcome::{ListedEntry, ListedKind, Listing, ScanError, ScanFailure};
pub(super) use super::parts::PendingLink;
use super::parts::{changed, join, listed_entry, parent_ids, shown, unreadable_directory};
use super::raw::{Answer, DirId, LinkTarget, RawEntry, RawKind, Subtree};

pub(super) struct Walk<'a> {
    exclude: &'a Exclude,
    max_entries: usize,
    counts: Vec<usize>,
    entries: Vec<ListedEntry>,
    unreadable_names: Vec<String>,
    pub truncated: bool,
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
            pending: Vec::new(),
        }
    }

    pub fn finish(mut self) -> Listing {
        self.entries.sort_by(|a, b| a.path.cmp(&b.path));
        self.unreadable_names.sort();
        Listing {
            entries: self.entries,
            unreadable_names: self.unreadable_names,
            truncated: self.truncated,
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
            return Err(ScanFailure::Cycle { path: shown(base) }.into());
        }
        let unreadable: HashSet<&[u8]> = tree.unreadable.iter().map(Vec::as_slice).collect();
        if unreadable.contains(&b""[..]) {
            return Err(unreadable_directory(base).into());
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
            if self.truncated {
                return Ok(());
            }
            let path = join(base, &entry.path);
            if entry.path.is_empty() || self.exclude.excludes(&path) {
                continue;
            }
            if entry.kind == RawKind::Directory {
                if unreadable.contains(entry.path.as_slice()) {
                    return Err(unreadable_directory(&path).into());
                }
                self.list_directory(&path, entry)?;
                continue;
            }
            let mut chain = ancestors.to_vec();
            chain.extend(parent_ids(&entry.path, &dir_ids));
            self.take_leaf(&path, entry, chain, counter)?;
        }
        Ok(())
    }

    /// ディレクトリは数えずに載せる。名前が UTF-8 として読めなければ載せず、下の項目で報告する
    fn list_directory(&mut self, path: &[u8], entry: &RawEntry) -> Result<(), ScanError> {
        if let Ok(text) = std::str::from_utf8(path) {
            self.entries
                .push(listed_entry(text, entry, ListedKind::Directory)?);
        }
        Ok(())
    }

    /// ファイル・symlink・そのほかの項目を数えて載せる。symlink は辿るかを決めるまで待たせる
    pub fn take_leaf(
        &mut self,
        path: &[u8],
        entry: &RawEntry,
        ancestors: Vec<DirId>,
        counter: usize,
    ) -> Result<(), ScanError> {
        if !self.count(counter) {
            return Ok(());
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
        let kind = match target.map(|target| std::str::from_utf8(target)) {
            None => Some(ListedKind::File),
            Some(Ok(target)) => Some(ListedKind::Symlink {
                target: target.to_string(),
                is_dir: false,
            }),
            Some(Err(_)) => None,
        };
        let listed = match text.zip(kind) {
            Some((text, kind)) => {
                self.entries.push(listed_entry(text, entry, kind)?);
                Some(self.entries.len() - 1)
            }
            None => {
                self.unreadable_names.push(shown(path));
                None
            }
        };
        if target.is_some() {
            self.pending.push(PendingLink {
                path: path.to_vec(),
                ancestors,
                counter,
                listed,
            });
        }
        Ok(())
    }

    /// 待たせた symlink のリンク先を答えから決める（REQ-scan-002・018）
    pub fn take_link(&mut self, link: PendingLink, answer: &Answer) -> Result<(), ScanError> {
        match answer.link {
            Some(LinkTarget::Directory) => {
                let Some(tree) = &answer.subtree else {
                    return Err(changed(&link.path));
                };
                if let Some(index) = link.listed {
                    if let ListedKind::Symlink { is_dir, .. } = &mut self.entries[index].kind {
                        *is_dir = true;
                    }
                }
                self.take_subtree(&link.path, tree, &link.ancestors, link.counter)
            }
            Some(LinkTarget::NotDirectory | LinkTarget::Missing) => Ok(()),
            Some(LinkTarget::Unresolvable) => Err(ScanFailure::UnresolvableLink {
                path: shown(&link.path),
            }
            .into()),
            None => Err(changed(&link.path)),
        }
    }
}
