//! 待たせた symlink のリンク先を、問いの答えから決める規則（REQ-scan-002・018）と、ディレクトリを
//! 載せる規則
use super::outcome::{ListedKind, ScanError, ScanFailure};
use super::parts::listed_entry;
use super::parts::{changed, shown, PendingLink};
use super::raw::RawEntry;
use super::raw::{Answer, LinkTarget};
use super::walk::Walk;

impl Walk<'_> {
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

    /// 指定したディレクトリの起点を器のディレクトリとして載せる
    pub fn list_container(&mut self, path: &[u8], entry: &RawEntry) -> Result<(), ScanError> {
        self.list_directory(path, entry)
    }

    /// ディレクトリは数えずに載せる。名前が UTF-8 として読めなければ載せず、下の項目で報告する
    pub(super) fn list_directory(
        &mut self,
        path: &[u8],
        entry: &RawEntry,
    ) -> Result<(), ScanError> {
        match std::str::from_utf8(path) {
            Ok(text) => self
                .entries
                .push(listed_entry(text, entry, ListedKind::Directory)?),
            // 名前が読めないディレクトリも、空で下に何もないときに黙って消えないよう報告する
            Err(_) => self.unreadable_names.push(shown(path)),
        }
        Ok(())
    }
}
