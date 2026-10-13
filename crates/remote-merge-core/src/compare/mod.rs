//! 比べる流れの判定の規則（docs/ir/compare/）。どれも純粋な関数で、経路にもコマンドにも依らない。
//! 状態の組の判定（judge）、中身の判定と読み込みの失敗の扱い（contents）、比べ方の選び方と symlink の
//! 組の合わせ方（method）、変更の直前の判定（recheck）を一つずつ置く。流れの組み立ては、側ごとの
//! 一覧から状態の表を作り（table）、組ごとに判定して中身を求める側を決め（pairs）、読んだハッシュと
//! 中身で決め直す（settle）三つの純粋な段を、経路の事実を集める段の間に挟む。

mod comparison;
mod contents;
mod judge;
mod method;
mod pairs;
mod recheck;
mod settle;
mod starts;
mod state;
mod table;
mod verdict;

pub use comparison::{has_content, Incomplete, NotObserved, PathComparison, Uses};
pub use contents::{
    judge_hashes, read_failure_handling, read_outcome, ContentJudgement, FailureHandling,
    ReadOutcome,
};
pub use judge::judge_states;
pub use method::{choose_method, combine_link_pair, Command, Method, Selection};
pub use pairs::{hash_roles, judge_pairs, Follow, Pair, PairJudgement};
pub use recheck::{check_before_change, ChangeCheck, Observed};
pub use settle::{apply_reads, settle_pairs, PairVerdict};
pub use starts::{blocked_starts, links_outside, outside_on_the_way, paths_to_check};
pub use state::{followed_state, Mtime, SideState};
pub use table::{state_table, PathStates};
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

#[cfg(test)]
#[path = "table_tests.rs"]
mod table_tests;

#[cfg(test)]
#[path = "pairs_tests.rs"]
mod pairs_tests;

#[cfg(test)]
#[path = "comparison_tests.rs"]
mod comparison_tests;

#[cfg(test)]
#[path = "settle_tests.rs"]
mod settle_tests;
