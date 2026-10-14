//! ファイル・symlink・そのほかの項目を一覧に載せる規則（件数・UTF-8 でない名前・その他の種類）
use super::outcome::{LinkPoint, ListedKind, ScanError, ScanFailure};
use super::parts::{Listed, PendingLink, listed_entry, shown};
use super::raw::{DirId, RawEntry, RawKind};
use super::walk::Walk;

impl Walk<'_> {
    /// ファイル・symlink・そのほかの項目を数えて載せる。symlink なら、辿るかを決めるために
    /// 待たせるリンクを返す（止まった後や件数の上限を超えたときは載せず、何も返さない）
    pub fn take_leaf(
        &mut self,
        path: &[u8],
        entry: &RawEntry,
        ancestors: Vec<DirId>,
        counter: usize,
    ) -> Result<Option<PendingLink>, ScanError> {
        self.take_leaf_counted(path, entry, ancestors, Some(counter))
    }

    /// [`take_leaf`](Self::take_leaf) の、件数に数える組 `counter` を選ぶもの（None なら数えない）
    pub fn take_leaf_counted(
        &mut self,
        path: &[u8],
        entry: &RawEntry,
        ancestors: Vec<DirId>,
        counter: Option<usize>,
    ) -> Result<Option<PendingLink>, ScanError> {
        // 数えない器は、ディレクトリと同じく打ち切った後も載せる
        if counter.is_some_and(|counter| self.stopped() || !self.count(counter)) {
            return Ok(None);
        }
        let counter = counter.unwrap_or(0);
        let target = match &entry.kind {
            RawKind::Symlink { target: None } => {
                return Err(ScanFailure::UnreadableLinkTarget { path: shown(path) }.into());
            }
            RawKind::Symlink {
                target: Some(target),
            } => Some(target),
            RawKind::File | RawKind::Directory | RawKind::Other => None,
        };
        let text = std::str::from_utf8(path).ok();
        let kind = match (target, &entry.kind) {
            (Some(target), _) => ListedKind::Symlink {
                target: target.clone(),
                points_to: LinkPoint::NotDirectory,
            },
            // その他の種類は通常ファイルとせず、その種類のまま載せる（REQ-scan-021）
            (None, RawKind::Other) => ListedKind::Other,
            (None, _) => ListedKind::File,
        };
        let listed = match text {
            Some(text) => {
                self.entries.push(listed_entry(text, entry, kind)?);
                Listed::At(self.entries.len() - 1)
            }
            None => {
                self.unreadable_names.push(shown(path));
                Listed::UnreadableName
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
