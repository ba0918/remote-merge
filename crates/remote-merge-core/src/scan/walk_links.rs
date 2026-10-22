//! 待たせた symlink のリンク先を、問いの答えから決める規則（REQ-scan-002・018）と、ディレクトリを
//! 載せる規則
use super::outcome::{LinkPoint, ListedKind, ScanError, ScanFailure};
use super::parts::listed_entry;
use super::parts::{Listed, PendingLink, changed, shown};
use super::raw::RawEntry;
use super::raw::{Answer, LinkTarget};
use super::walk::Walk;

impl Walk<'_> {
    /// 待たせた symlink のリンク先を答えから決め、ディレクトリなら辿って下を載せる（REQ-scan-002・018）
    pub fn take_link(&mut self, link: &PendingLink, answer: &Answer) -> Result<(), ScanError> {
        self.point_link(link, answer)?;
        if answer.link != Some(LinkTarget::Directory) {
            return Ok(());
        }
        let Some(tree) = &answer.subtree else {
            return Err(changed(&link.path));
        };
        self.take_subtree(&link.path, tree, &link.ancestors, link.counter)
    }

    /// 待たせた symlink を辿らずに、リンク先の種類だけを答えから決める
    pub fn point_link(&mut self, link: &PendingLink, answer: &Answer) -> Result<(), ScanError> {
        match answer.link {
            Some(LinkTarget::Directory) => self.point(link, LinkPoint::Directory),
            Some(LinkTarget::Missing) => self.point(link, LinkPoint::Missing),
            Some(LinkTarget::Other) => self.point(link, LinkPoint::Other),
            Some(LinkTarget::NotDirectory) => {}
            Some(LinkTarget::Unresolvable) => return self.point_unresolvable(link),
            None => return Err(changed(&link.path)),
        }
        Ok(())
    }

    /// 辿れないリンクは、名前の形によらず止めずに、そうと残して下を辿らない（REQ-scan-018）。一覧に
    /// 載せたものには印を付け、名前を読めないもの（読めない名前として報告済み）は辿れない名前に足す。
    /// 指定したディレクトリの起点だけは、下を集める起点なので走査を止める
    fn point_unresolvable(&mut self, link: &PendingLink) -> Result<(), ScanError> {
        match link.listed {
            Listed::Start => Err(ScanFailure::UnresolvableLink {
                path: shown(&link.path),
            }
            .into()),
            Listed::UnreadableName => {
                self.unresolvable_names.push(shown(&link.path));
                Ok(())
            }
            Listed::At(_) => {
                self.point(link, LinkPoint::Unresolvable);
                Ok(())
            }
        }
    }

    /// 一覧に載せた symlink のリンク先の種類を決める
    fn point(&mut self, link: &PendingLink, point: LinkPoint) {
        if let Listed::At(index) = link.listed
            && let ListedKind::Symlink { points_to, .. } = &mut self.entries[index].kind
        {
            *points_to = point;
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
