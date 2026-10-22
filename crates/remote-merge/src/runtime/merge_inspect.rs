//! merge と sync の変更の前に、書き込み先についてまとめて尋ねること。書き込み先のパスとその親と、
//! 書き込み先がないパスの祖先（書き込みが作るディレクトリ）の確認と、バックアップのための書き込み先の
//! 中身の読み込みを、それぞれ経路に一度に尋ねる

use std::collections::BTreeMap;

use remote_merge_core::compare::{PathComparison, Role, SideState};
use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPathError};

use super::core::CoreRuntime;
use super::merge_apply::{ApplyOptions, ChangeKind, PlannedChange};
use super::root_checked::{DistinctPaths, RootAbsent};
use crate::side::Side;

/// 書き込み先のパスを確かめた結果
pub(super) struct Inspected {
    pub entry: ResolvedPath,
    /// 親の答え。確かめられなければその誤りの文
    pub parent: Result<ResolvedPath, String>,
    /// 書く前になかった祖先（書き込みが作るディレクトリ。root_dir に近い順）
    pub missing_ancestors: Vec<EntryPath>,
}

/// バックアップのために読んだ書き込み先の中身
pub(super) type BackupContent = Result<Vec<u8>, OpFailure>;

impl CoreRuntime {
    /// 書き込み先のパスとその親と、書き込み先がないパスの祖先を、root_dir と一緒に経路にまとめて一度に
    /// 尋ねる。同じパスは一度だけ尋ね、答えを変更ごとに分ける。確かめられない書き込み先は、その誤りの文
    /// にする。比べた後なので、root_dir がなければなくなったものとしてその側を止める（REQ-merge-044）
    pub(super) fn inspect_destinations(
        &mut self,
        target: &Side,
        changes: &[PlannedChange<'_>],
    ) -> anyhow::Result<Vec<Result<Inspected, String>>> {
        let mut asked = DistinctPaths::default();
        let mut layout = Vec::new();
        for change in changes {
            let entry = EntryPath::parse(change.path)?;
            let at_entry = asked.add(entry.as_rel().clone());
            let at_parent = asked.add(entry.parent_and_name().0);
            let ancestors: Vec<(EntryPath, usize)> = if destination_absent(change.comparison) {
                entry
                    .parent_entries()
                    .into_iter()
                    .map(|ancestor| {
                        let at = asked.add(ancestor.as_rel().clone());
                        (ancestor, at)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            layout.push((at_entry, at_parent, ancestors));
        }
        let answers: Vec<Result<ResolvedPath, String>> = self
            .inspect_under_root(target, asked.paths(), RootAbsent::GoneAfterScan)?
            .answers
            .into_iter()
            .map(|answer| answer.map_err(|error| format!("{error:#}")))
            .collect();
        Ok(layout
            .into_iter()
            .map(|(at_entry, at_parent, ancestors)| {
                let missing_ancestors = ancestors
                    .into_iter()
                    .filter(|(_, at)| matches!(answers[*at], Ok(ResolvedPath::Missing { .. })))
                    .map(|(ancestor, _)| ancestor)
                    .collect();
                answers[at_entry].clone().map(|entry| Inspected {
                    entry,
                    parent: answers[at_parent].clone(),
                    missing_ancestors,
                })
            })
            .collect())
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
