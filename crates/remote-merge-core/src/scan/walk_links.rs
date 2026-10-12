//! 待たせた symlink のリンク先を、問いの答えから決める規則（REQ-scan-002・018）
use super::outcome::{ListedKind, ScanError, ScanFailure};
use super::parts::{changed, shown, PendingLink};
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
}
