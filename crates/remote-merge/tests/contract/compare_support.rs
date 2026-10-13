#![cfg(unix)]
//! 比べる流れの契約テストの道具。子のプロセスで比べた結果を一つの文字列にし、コマンドの代わりに
//! 判定の扱いに使うもの（ハッシュと中身）を告げる

use std::collections::BTreeSet;

use remote_merge::runtime::{CompareRequest, CompareScope, Comparison, ExternalLinks};
use remote_merge_core::compare::{
    Follow, Method, PairJudgement, PairVerdict, PathStates, Present, Role, Uses, Verdict,
};
use remote_merge_core::op_failure::SizeLimit;

/// 何も使わない（status の判定だけ）
pub(super) struct NothingUsed;

impl Uses for NothingUsed {
    fn hashes(&self, _: &str, _: &PathStates, _: &[PairJudgement]) -> BTreeSet<Role> {
        BTreeSet::new()
    }
    fn contents(&self, _: &str, _: &PathStates, _: &[PairVerdict]) -> BTreeSet<Role> {
        BTreeSet::new()
    }
}

/// diff と同じく、左右の組が同じでなければ、左右の中身を読む
pub(super) struct DiffUses;

impl Uses for DiffUses {
    fn hashes(&self, _: &str, _: &PathStates, _: &[PairJudgement]) -> BTreeSet<Role> {
        BTreeSet::new()
    }
    fn contents(&self, _: &str, _: &PathStates, verdicts: &[PairVerdict]) -> BTreeSet<Role> {
        let differs = verdicts.iter().any(|pair| {
            pair.pair.first == Role::Left
                && pair.pair.second == Role::Right
                && matches!(
                    pair.verdict,
                    Verdict::Differs | Verdict::OneSided(_) | Verdict::KindDiffers
                )
        });
        if differs {
            BTreeSet::from([Role::Left, Role::Right])
        } else {
            BTreeSet::new()
        }
    }
}

/// merge・sync と同じく、書き込み先（右）の通常ファイルのハッシュを比べるときに取る
pub(super) struct DestinationHashed;

impl Uses for DestinationHashed {
    fn hashes(&self, _: &str, _: &PathStates, _: &[PairJudgement]) -> BTreeSet<Role> {
        BTreeSet::from([Role::Right])
    }
    fn contents(&self, _: &str, _: &PathStates, _: &[PairVerdict]) -> BTreeSet<Role> {
        BTreeSet::new()
    }
}

/// 比べ方の求め。中身の読み込みの上限は `limit`
pub(super) fn request(scope: CompareScope, method: Method, follow: Follow) -> CompareRequest {
    CompareRequest {
        scope,
        max_entries: 1000,
        method,
        follow,
        external_links: ExternalLinks::Follow,
        content_limit: SizeLimit::Unlimited,
    }
}

fn verdict_text(verdict: &Verdict) -> String {
    match verdict {
        Verdict::BothAbsent => "both-absent".into(),
        Verdict::Undecidable(why) => format!("undecidable({why})"),
        Verdict::OneSided(Present::First) => "first-only".into(),
        Verdict::OneSided(Present::Second) => "second-only".into(),
        Verdict::KindDiffers => "kind-differs".into(),
        Verdict::Same => "same".into(),
        Verdict::Differs => "differs".into(),
    }
}

fn role_text(role: Role) -> &'static str {
    match role {
        Role::Left => "left",
        Role::Right => "right",
        Role::Ref => "ref",
    }
}

/// パスごとの組の判定（"パス 左-右=判定 …"）と、側ごとに尋ねたもの（"asked 側 hash=… read=…"）
pub(super) fn comparison_text(comparison: anyhow::Result<Comparison>) -> String {
    let comparison = match comparison {
        Ok(comparison) => comparison,
        Err(error) => return format!("error: {error:#}\n"),
    };
    let mut text = String::new();
    for (path, compared) in &comparison.paths {
        text.push_str(path);
        for pair in &compared.pairs {
            text.push_str(&format!(
                " {}-{}={}",
                role_text(pair.pair.first),
                role_text(pair.pair.second),
                verdict_text(&pair.verdict)
            ));
            if let Some(followed) = &pair.followed {
                text.push_str(&format!("(followed {})", verdict_text(followed)));
            }
        }
        for role in &compared.states.outside {
            text.push_str(&format!(" outside:{}", role_text(*role)));
        }
        for role in &compared.states.not_collected {
            text.push_str(&format!(" not-collected:{}", role_text(*role)));
        }
        text.push('\n');
    }
    for (role, asked) in &comparison.asked {
        text.push_str(&format!(
            "asked {} hash={:?} read={:?}\n",
            role_text(*role),
            asked.hashed,
            asked.read
        ));
    }
    for incomplete in &comparison.incomplete {
        text.push_str(&format!("incomplete {incomplete:?}\n"));
    }
    text
}
