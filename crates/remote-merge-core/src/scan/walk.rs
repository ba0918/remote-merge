//! 集めた生の項目を一覧に載せる規則。exclude・件数・UTF-8 でない名前・循環・辿るか・読めない
//! ときに止めるかをここで決める（docs/ir/scan/ と docs/ir/config/filters.md）。

use std::collections::HashMap;

use super::exclude::Exclude;
use super::outcome::{ListedEntry, Listing, ScanError, ScanFailure};
pub(super) use super::parts::PendingLink;
use super::parts::{
    changed, in_path_order, join, parent_ids, parent_of, shown, unreadable_directory, Item,
};
use super::raw::{DirId, RawKind, ReadCause, Subtree};

pub(super) struct Walk<'a> {
    exclude: &'a Exclude,
    max_entries: usize,
    counts: Vec<usize>,
    pub(super) entries: Vec<ListedEntry>,
    pub(super) unreadable_names: Vec<String>,
    pub(super) unresolvable_names: Vec<String>,
    pub truncated: bool,
    /// 循環に出会ったパス（不完全な一覧。REQ-scan-002）
    pub cycle: Option<String>,
    pub pending: Vec<PendingLink>,
    /// ディレクトリへの symlink を辿って下を載せるか。子の一覧ではリンク先がディレクトリかだけを
    /// 決め、辿らない
    pub follow_links: bool,
}

impl<'a> Walk<'a> {
    pub fn new(exclude: &'a Exclude, max_entries: usize, counters: usize) -> Self {
        Self {
            exclude,
            max_entries,
            counts: vec![0; counters.max(1)],
            entries: Vec::new(),
            unreadable_names: Vec::new(),
            unresolvable_names: Vec::new(),
            truncated: false,
            cycle: None,
            pending: Vec::new(),
            follow_links: true,
        }
    }

    /// 件数の上限か循環で、これより先を集めない
    pub fn stopped(&self) -> bool {
        self.truncated || self.cycle.is_some()
    }

    pub fn finish(mut self) -> Listing {
        self.entries.sort_by(|a, b| a.path.cmp(&b.path));
        self.unreadable_names.sort();
        self.unresolvable_names.sort();
        Listing {
            entries: self.entries,
            unreadable_names: self.unreadable_names,
            truncated: self.truncated,
            cycle: self.cycle,
            unresolvable_names: self.unresolvable_names,
            counted: self.counts.iter().sum(),
        }
    }

    /// 件数の上限の内なら数えて true。超えたら打ち切りにして false（REQ-scan-010・004）
    pub(super) fn count(&mut self, counter: usize) -> bool {
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
        // 一覧に出た後で、ない・ディレクトリでない・中身が合わないと分かった項目は、読めない
        // のではなく走査の途中で変わったもの（REQ-scan-017）
        if tree.changed.iter().any(Vec::is_empty) {
            if base.is_empty() {
                return Err(ScanFailure::RootChanged.into());
            }
            return Err(changed(base));
        }
        if let Some(cause) = unreadable.get(&b""[..]) {
            return Err(unreadable_directory(base, *cause).into());
        }
        let mut dir_ids: HashMap<&[u8], DirId> = HashMap::from([(&b""[..], tree.start.id)]);
        for entry in &tree.entries {
            if entry.kind == RawKind::Directory {
                dir_ids.insert(&entry.path, entry.id);
            }
        }
        // 件数の上限や循環で止まった位置より後ろのものには当たらない
        for (rel, item) in in_path_order(tree) {
            if self.stopped() {
                return Ok(());
            }
            let path = join(base, rel);
            let entry = match item {
                Item::Entry(entry) => entry,
                // exclude に当たる項目は見ない（REQ-config-044）
                _ if self.exclude.excludes(&path) => continue,
                Item::Changed => return Err(changed(&path)),
                // 情報を読めなかった項目は、それを入れているディレクトリを読めなかったものに
                // する（REQ-scan-014）
                Item::Unread(cause) => {
                    return Err(unreadable_directory(&join(base, parent_of(rel)), cause).into())
                }
            };
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
}
