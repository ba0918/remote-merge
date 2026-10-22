//! rollback が戻す先について集める事実。状態は比べる流れの集め方（名前で指定したパスの走査。role は
//! target）で集め、記録と比べる実パスはパスとその親の確認を経路に一度にまとめて尋ねる
//! （docs/ir/backup/rollback-path.md の REQ-backup-003・027、docs/ir/compare/flow.md の
//! REQ-compare-003）

use std::collections::BTreeMap;

use remote_merge_core::compare::{Follow, Method, Role, SideState, state_table};
use remote_merge_core::op_failure::SizeLimit;
use remote_merge_core::path_resolution::ResolveError;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_engine::treat::rollback::TargetFacts;

use super::compare_flow::{
    CompareRequest, CompareScope, EntryLimit, ExternalLinks, LostDestination, UnfollowableLinks,
};
use super::core::CoreRuntime;
use super::root_checked::{RootAbsent, is_root_missing};
use crate::side::Side;

impl CoreRuntime {
    /// 戻す先 `target` の `paths` の、パスとその親の確認の答えと状態（パスの順）。確認できないパスが
    /// あっても、ほかのパスの状態は集める（どのパスの事実も一つの問いの答えから決める）
    pub(super) fn rollback_facts(
        &mut self,
        target: &Side,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<TargetFacts>> {
        let mut asked: Vec<RelPath> = Vec::new();
        for path in paths {
            asked.push(path.as_rel().clone());
            asked.push(path.parent_and_name().0);
        }
        // root_dir もパスの深さによらず同じ問いで尋ね、なければ走査の root_dir の誤りと同じにする
        let under = match self.inspect_under_root(target, &asked, RootAbsent::MissingAtStart) {
            Ok(under) => under,
            // つなぎ直しても `接続断` なら、呼び出し側がどのパスも読み込みの失敗にする（REQ-ssh-003）
            Err(error) if super::connection::classify(&error).is_disconnection() => {
                return Err(error);
            }
            // root_dir がないときは、走査の root_dir の誤りとして止める（REQ-merge-044）
            Err(error) if is_root_missing(&error) => return Err(error),
            // 戻す先を辿れないときは、root_dir がないとせず、どのパスも "cannot resolve path: " の
            // 失敗にする（REQ-backup-003）
            Err(error) => {
                let cause = unresolved_cause(&error);
                return Ok(paths
                    .iter()
                    .map(|_| TargetFacts::Unresolvable(cause.clone()))
                    .collect());
            }
        };
        let mut answers = under.answers.into_iter();
        let mut resolved = Vec::new();
        for path in paths {
            let (Some(entry), Some(parent)) = (answers.next(), answers.next()) else {
                anyhow::bail!("the target paths were not all answered");
            };
            resolved.push(match (entry, parent) {
                (Ok(entry), Ok(parent)) => Ok((path.as_str().to_string(), entry, parent)),
                (Err(error), _) | (_, Err(error)) => Err(unresolved_cause(&error)),
            });
        }
        // 確かめられたパスだけを走査する（辿れない symlink の起点は走査を止めるので入れない）
        let names: Vec<String> = resolved
            .iter()
            .filter_map(|resolved| match resolved {
                Ok((name, _, _)) => Some(name.clone()),
                Err(_) => None,
            })
            .collect();
        let table = self.target_states(target, &names)?;
        Ok(resolved
            .into_iter()
            .map(|resolved| match resolved {
                Err(cause) => TargetFacts::Unresolvable(cause),
                Ok((name, entry, parent)) => TargetFacts::Resolved {
                    // 走査の一覧に載らなかった名前で指定したパスは、その側にない（REQ-compare-003）
                    state: table.get(&name).cloned().unwrap_or(SideState::Absent),
                    entry,
                    parent,
                },
            })
            .collect())
    }

    /// 名前で指定したパスを走査して集めた、戻す先の状態（role は target）
    fn target_states(
        &mut self,
        target: &Side,
        names: &[String],
    ) -> anyhow::Result<BTreeMap<String, SideState>> {
        if names.is_empty() {
            return Ok(BTreeMap::new());
        }
        let request = CompareRequest {
            scope: CompareScope::Entries(names.to_vec()),
            max_entries: self.config.max_scan_entries,
            entry_limit: EntryLimit::PerDirectory,
            counted_before: Default::default(),
            method: Method::Metadata,
            named_files: None,
            follow: Follow::No,
            external_links: ExternalLinks::Follow,
            content_limit: SizeLimit::from_force(false),
            unfollowable_links: UnfollowableLinks::Stop,
            lost_destination: LostDestination::StopCommand,
        };
        let (listing, _) = self.collect_side(target, &request, names, request.max_entries)?;
        request.unfollowable_links.check(&listing)?;
        if listing.truncated || listing.cycle.is_some() {
            anyhow::bail!("the paths of the session could not all be listed on the target");
        }
        let table = state_table(&BTreeMap::from([(Role::Target, listing)]), &BTreeMap::new());
        Ok(table
            .into_iter()
            .filter_map(|(path, states)| {
                let state = states.states.get(&Role::Target)?.clone();
                Some((path, state))
            })
            .collect())
    }
}

/// パスを確かめられなかった原因（"cannot resolve path: " に続く文言）
fn unresolved_cause(error: &anyhow::Error) -> String {
    match error.downcast_ref::<ResolveError>() {
        Some(error) => error.cause().to_string(),
        None => format!("{error:#}"),
    }
}

#[cfg(test)]
#[path = "rollback_facts_tests.rs"]
mod tests;
