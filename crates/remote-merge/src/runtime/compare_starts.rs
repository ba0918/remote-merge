//! 比べる流れが、名前で指定したパスとディレクトリ（起点）について集める事実。起点そのものの状態は
//! 一覧の器でなく起点そのものを集めて決め（docs/ir/compare/flow.md の REQ-compare-002）、途中が
//! ディレクトリでない起点はその側にないとし（REQ-compare-003）、途中が root_dir の外へ解決される
//! 起点の下は集めない（REQ-compare-004）

use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::include::is_inside;
use remote_merge_core::scan::outcome::{Listing, ScanFailure, ScanRequest, ScanScope, Start};

use super::core::CoreRuntime;
use crate::side::Side;

/// 起点のパスを、一覧のパスと比べられる形にする
fn normalized(path: &str) -> &str {
    path.trim_end_matches('/')
}

impl CoreRuntime {
    /// `scope` で走査する。名前で指定した起点 `named` のうち途中がディレクトリでないものは、その側に
    /// ないものとして外し、ほかの起点を走査し直す
    pub(super) fn scan_named(
        &mut self,
        side: &Side,
        mut scope: ScanScope,
        max_entries: usize,
        named: &[String],
    ) -> anyhow::Result<Listing> {
        loop {
            let request = ScanRequest {
                scope: scope.clone(),
                max_entries,
            };
            let error = match self.scan_listing(side, &request) {
                Ok(listing) => return Ok(listing),
                Err(error) => error,
            };
            let blocked = error.chain().find_map(|cause| match cause.downcast_ref() {
                Some(ScanFailure::StartBlocked { path }) => Some(normalized(path).to_string()),
                _ => None,
            });
            let (Some(blocked), ScanScope::Starts(starts)) = (blocked, &mut scope) else {
                return Err(error);
            };
            if !named.iter().any(|name| normalized(name) == blocked) {
                return Err(error);
            }
            let before = starts.len();
            starts.retain(|start| normalized(&start.path) != blocked);
            if starts.len() == before {
                return Err(error);
            }
            if starts.is_empty() {
                return Ok(Listing::default());
            }
        }
    }

    /// ディレクトリの下の一覧 `listing` の、起点 `dirs` を器として載せた項目を、起点そのものを集めた
    /// 項目に置き換える（ディレクトリへの symlink の起点は symlink として載る）
    pub(super) fn with_starts_themselves(
        &mut self,
        side: &Side,
        dirs: &[String],
        mut listing: Listing,
        max_entries: usize,
    ) -> anyhow::Result<Listing> {
        let starts = dirs
            .iter()
            .map(|dir| Start::itself(normalized(dir), 0))
            .collect();
        let themselves = self.scan_named(side, ScanScope::Starts(starts), max_entries, dirs)?;
        listing
            .entries
            .retain(|entry| !dirs.iter().any(|dir| normalized(dir) == entry.path));
        listing.entries.extend(themselves.entries);
        Ok(listing)
    }

    /// 起点 `named` の途中（起点そのものを含む）で、最も浅い root_dir の外へ解決される symlink のパス
    pub(super) fn starts_outside_root(
        &mut self,
        side: &Side,
        named: &[String],
    ) -> anyhow::Result<Vec<String>> {
        let mut io = super::target_io::for_side(side, self);
        let root = super::compare_checks::root_real_path(io.as_mut(), self)?;
        let mut outside: Vec<String> = Vec::new();
        for path in named {
            let mut prefix = String::new();
            for element in normalized(path).split('/').filter(|e| !e.is_empty()) {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(element);
                match io.inspect_path(self, &RelPath::parse(&prefix)?)? {
                    ResolvedPath::Symlink { real_path, .. } if !is_inside(&real_path, &root) => {
                        if !outside.contains(&prefix) {
                            outside.push(prefix.clone());
                        }
                        break;
                    }
                    ResolvedPath::Exists { .. } | ResolvedPath::Symlink { .. } => {}
                    ResolvedPath::Missing { .. } | ResolvedPath::AncestorNotDirectory { .. } => {
                        break
                    }
                }
            }
        }
        Ok(outside)
    }
}
