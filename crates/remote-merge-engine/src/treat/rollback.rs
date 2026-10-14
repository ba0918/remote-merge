//! rollback の書き戻しの扱い（docs/ir/backup/rollback-path.md の REQ-backup-003・010・027・030〜033、
//! docs/ir/compare/reading.md の TBL-compare-005 の rollback の行）。戻す先の状態は比べる流れの集め方
//! で集めたもの（role は target）、場所は経路のパスの確認の答えで、記録と比べて決める

use std::path::Path;

use remote_merge_core::compare::{Role, SideState, Undecidable};
use remote_merge_core::path_resolution::{ExistingKind, ResolvedPath};

/// 親のディレクトリがなくなったファイルのスキップの理由（REQ-backup-031）
pub const PARENT_GONE: &str = "parent directory no longer exists";
/// 置き換えた symlink の記録のスキップの理由（REQ-backup-032）
pub const LINK_NOT_SUPPORTED: &str = "symlink restore not supported";
/// 記録と違う場所に解決されるパスのスキップの理由（REQ-backup-033）
pub const CHANGED_LOCATION: &str = "path now resolves to a different location";
/// merge の後に第三者が変えた symlink のスキップの理由（REQ-backup-033）
pub const LINK_CHANGED: &str = "symlink changed after merge";
/// リンク先を辿れないパスの failed の error の頭（REQ-backup-003）
const UNRESOLVABLE: &str = "cannot resolve path: ";

/// 戻す記録の種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// 通常ファイルの中身（書き戻すか作り直す）。`real_path` は記録したときの実パス
    File { real_path: String },
    /// symlink 同士の更新。`expected_target` は merge が書いたリンク文字列、`real_parent` は親の実パス
    LinkUpdate {
        expected_target: String,
        real_parent: String,
    },
    /// symlink を通常ファイルに置き換えたか消した記録
    ReplacedLink,
}

/// 戻す先のパスについて集めた事実
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetFacts {
    /// パスかその親を確かめられない（その原因）
    Unresolvable(String),
    Resolved {
        /// 比べる流れの集め方で集めた状態
        state: SideState,
        /// パスの確認の答え
        entry: ResolvedPath,
        /// 親のパスの確認の答え
        parent: ResolvedPath,
    },
}

/// 一つのパスの扱い
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreEntry {
    Restore,
    Skip(String),
    Fail(String),
}

/// パスごとに決めた扱い（セッション全体を止めるものを含む）
enum Judged {
    Entry(RestoreEntry),
    /// セッションのどのファイルも書き戻さないスキップ（REQ-backup-003・010）
    Blocks(&'static str),
    /// リンク先を辿れない（セッションのすべてを failed にする。REQ-backup-003）
    Unresolvable(String),
}

/// セッションのパスの順に、書き戻す・スキップ・失敗を決める
pub fn rollback_plan(paths: Vec<(String, Recorded, TargetFacts)>) -> Vec<(String, RestoreEntry)> {
    let judged: Vec<(String, Judged)> = paths
        .into_iter()
        .map(|(path, recorded, facts)| {
            let judged = judge(&recorded, &facts);
            (path, judged)
        })
        .collect();
    let unresolvable = judged.iter().find_map(|(_, judged)| match judged {
        Judged::Unresolvable(cause) => Some(cause.clone()),
        _ => None,
    });
    let blocking = judged.iter().find_map(|(_, judged)| match judged {
        Judged::Blocks(reason) => Some(*reason),
        _ => None,
    });
    judged
        .into_iter()
        .map(|(path, judged)| {
            let entry = match (judged, &unresolvable, blocking) {
                (Judged::Unresolvable(cause), _, _) => {
                    RestoreEntry::Fail(format!("{UNRESOLVABLE}{cause}"))
                }
                // ほかのパスを辿れないときは、そのパスの原因を示す（REQ-backup-003）
                (_, Some(cause), _) => RestoreEntry::Fail(format!("{UNRESOLVABLE}{cause}")),
                (Judged::Blocks(reason), None, _) => RestoreEntry::Skip(reason.to_string()),
                (Judged::Entry(RestoreEntry::Restore), None, Some(reason)) => {
                    RestoreEntry::Skip(reason.to_string())
                }
                (Judged::Entry(entry), None, _) => entry,
            };
            (path, entry)
        })
        .collect()
}

fn judge(recorded: &Recorded, facts: &TargetFacts) -> Judged {
    let (state, entry, parent) = match (recorded, facts) {
        (Recorded::ReplacedLink, _) => {
            return Judged::Entry(RestoreEntry::Skip(LINK_NOT_SUPPORTED.to_string()));
        }
        (_, TargetFacts::Unresolvable(cause)) => return Judged::Unresolvable(cause.clone()),
        (
            _,
            TargetFacts::Resolved {
                state,
                entry,
                parent,
            },
        ) => (state, entry, parent),
    };
    match recorded {
        Recorded::File { real_path } => judge_file(real_path, state, entry),
        Recorded::LinkUpdate {
            expected_target,
            real_parent,
        } => judge_link(expected_target, real_parent, state, parent),
        Recorded::ReplacedLink => Judged::Entry(RestoreEntry::Skip(LINK_NOT_SUPPORTED.to_string())),
    }
}

fn judge_file(recorded_path: &str, state: &SideState, entry: &ResolvedPath) -> Judged {
    match state {
        SideState::Other | SideState::Directory => Judged::Entry(RestoreEntry::Fail(
            Undecidable::NotRegularFile(vec![Role::Target]).to_string(),
        )),
        SideState::UnreadableName => {
            Judged::Entry(RestoreEntry::Fail(Undecidable::UnreadableName.to_string()))
        }
        SideState::Symlink { .. } => Judged::Blocks(CHANGED_LOCATION),
        SideState::File { .. } => match entry {
            ResolvedPath::Exists {
                real_path,
                kind: ExistingKind::File,
            } if Path::new(real_path) == Path::new(recorded_path) => {
                Judged::Entry(RestoreEntry::Restore)
            }
            _ => Judged::Blocks(CHANGED_LOCATION),
        },
        SideState::Absent => match entry {
            ResolvedPath::Missing {
                parent_exists: false,
                ..
            }
            | ResolvedPath::AncestorNotDirectory { .. } => {
                Judged::Entry(RestoreEntry::Skip(PARENT_GONE.to_string()))
            }
            ResolvedPath::Missing { real_parent, .. }
                if Path::new(recorded_path).parent() == Some(Path::new(real_parent)) =>
            {
                Judged::Entry(RestoreEntry::Restore)
            }
            _ => Judged::Blocks(CHANGED_LOCATION),
        },
    }
}

fn judge_link(
    expected_target: &str,
    recorded_parent: &str,
    state: &SideState,
    parent: &ResolvedPath,
) -> Judged {
    match state {
        SideState::Symlink { target, .. } if target.as_slice() == expected_target.as_bytes() => {
            match parent {
                ResolvedPath::Exists { real_path, .. }
                | ResolvedPath::Symlink { real_path, .. }
                    if Path::new(real_path) == Path::new(recorded_parent) =>
                {
                    Judged::Entry(RestoreEntry::Restore)
                }
                _ => Judged::Blocks(CHANGED_LOCATION),
            }
        }
        _ => Judged::Blocks(LINK_CHANGED),
    }
}

#[cfg(test)]
#[path = "rollback_tests.rs"]
mod tests;
