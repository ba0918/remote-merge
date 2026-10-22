//! merge と sync の変更の前に、書き込み先についてまとめて尋ねること。書き込み先のパスとその親と、
//! 書き込み先がないパスの祖先（書き込みが作るディレクトリ）の確認と、バックアップのための書き込み先の
//! 中身の読み込みを、それぞれ経路に一度に尋ねる

use std::collections::BTreeMap;

use remote_merge_core::compare::{PathComparison, Role, SideState};
use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath, RelPathError};

use super::core::CoreRuntime;
use super::merge_apply::{ApplyOptions, ChangeKind, PlannedChange};
use super::root_checked::RootAbsent;
use crate::side::Side;

/// 書き込み先のパスを確かめた結果
pub(super) struct Inspected {
    pub entry: ResolvedPath,
    pub parent: anyhow::Result<ResolvedPath>,
    /// 書く前になかった祖先（書き込みが作るディレクトリ。root_dir に近い順）
    pub missing_ancestors: Vec<EntryPath>,
}

/// バックアップのために読んだ書き込み先の中身
pub(super) type BackupContent = Result<Vec<u8>, OpFailure>;

impl CoreRuntime {
    /// 書き込み先のパスとその親と、書き込み先がないパスの祖先を、root_dir と一緒に経路にまとめて一度に
    /// 尋ねる。比べた後なので、root_dir がなければなくなったものとしてその側を止める（REQ-merge-044）
    pub(super) fn inspect_destinations(
        &mut self,
        target: &Side,
        changes: &[PlannedChange<'_>],
    ) -> anyhow::Result<Vec<anyhow::Result<Inspected>>> {
        let mut rels: Vec<RelPath> = Vec::new();
        let mut layout = Vec::new();
        for change in changes {
            let entry = EntryPath::parse(change.path)?;
            let ancestors = if destination_absent(change.comparison) {
                entry.parent_entries()
            } else {
                Vec::new()
            };
            rels.push(entry.as_rel().clone());
            rels.push(entry.parent_and_name().0);
            rels.extend(ancestors.iter().map(|ancestor| ancestor.as_rel().clone()));
            layout.push(ancestors);
        }
        let mut answers = self
            .inspect_under_root(target, &rels, RootAbsent::GoneAfterScan)?
            .answers
            .into_iter();
        let mut inspected = Vec::new();
        for ancestors in layout {
            let (Some(entry), Some(parent)) = (answers.next(), answers.next()) else {
                anyhow::bail!("the destination paths were not all answered");
            };
            let mut missing_ancestors = Vec::new();
            for ancestor in ancestors {
                match answers.next() {
                    Some(Ok(ResolvedPath::Missing { .. })) => missing_ancestors.push(ancestor),
                    Some(_) => {}
                    None => anyhow::bail!("the destination paths were not all answered"),
                }
            }
            inspected.push(entry.map(|entry| Inspected {
                entry,
                parent,
                missing_ancestors,
            }));
        }
        Ok(inspected)
    }

    /// バックアップする書き込み先の通常ファイルの中身を、まとめて読む
    pub(super) fn read_backup_contents(
        &mut self,
        target: &Side,
        paths: &[EntryPath],
        options: &ApplyOptions<'_>,
    ) -> anyhow::Result<BTreeMap<String, BackupContent>> {
        if paths.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut io = super::target_io::for_side(target, self);
        Ok(io
            .read_entries(self, paths, options.content_limit)?
            .into_iter()
            .collect())
    }
}

/// 書き換えるか消す書き込み先の通常ファイル（バックアップのために中身を読むもの。バックアップが無効か
/// --dry-run ならない）
pub(super) fn backed_up_paths(
    changes: &[PlannedChange<'_>],
    options: &ApplyOptions<'_>,
) -> Result<Vec<EntryPath>, RelPathError> {
    if options.session.is_none() || options.dry_run {
        return Ok(Vec::new());
    }
    changes
        .iter()
        .filter(|change| matches!(change.kind, ChangeKind::WriteFile | ChangeKind::Delete))
        .filter(|change| {
            matches!(
                change.comparison.states.states.get(&Role::Right),
                Some(SideState::File { .. })
            )
        })
        .map(|change| EntryPath::parse(change.path))
        .collect()
}

/// 比べたときに書き込み先がなかったか
fn destination_absent(comparison: &PathComparison) -> bool {
    matches!(
        comparison.states.states.get(&Role::Right),
        Some(SideState::Absent)
    )
}

#[cfg(all(test, unix))]
#[path = "merge_inspect_tests.rs"]
mod tests;
