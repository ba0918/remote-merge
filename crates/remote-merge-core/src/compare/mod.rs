//! 比べる流れの判定の規則（docs/ir/compare/）。どれも純粋な関数で、経路にもコマンドにも依らない。
//! 状態の組の判定（judge）、中身の判定と読み込みの失敗の扱い（contents）、比べ方の選び方と symlink の
//! 組の合わせ方（method）、変更の直前の判定（recheck）を一つずつ置く。

mod contents;
mod judge;
mod method;
mod recheck;
mod state;
mod verdict;

pub use contents::{
    judge_hashes, read_failure_handling, read_outcome, ContentJudgement, FailureHandling,
    ReadOutcome,
};
pub use judge::judge_states;
pub use method::{choose_method, combine_link_pair, Command, Method, Selection};
pub use recheck::{check_before_change, ChangeCheck, Observed};
pub use state::{followed_state, Mtime, SideState};
pub use verdict::{Judgement, Present, Role, SideFailure, Undecidable, Verdict};

#[cfg(test)]
#[path = "judge_tests.rs"]
mod judge_tests;

#[cfg(test)]
#[path = "judge_file_tests.rs"]
mod judge_file_tests;

#[cfg(test)]
#[path = "contents_tests.rs"]
mod contents_tests;

#[cfg(test)]
#[path = "method_tests.rs"]
mod method_tests;

#[cfg(test)]
#[path = "recheck_tests.rs"]
mod recheck_tests;
