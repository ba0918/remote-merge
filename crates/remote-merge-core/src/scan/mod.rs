//! 走査の規則（docs/ir/scan/ と docs/ir/config/filters.md・include.md）。
//!
//! 規則は一か所にあり、経路は [`raw::Collector`] として生の項目を集めるだけにする
//! （docs/decision/records/2026-10-12-foundation-2.md の C6）。

pub mod children;
pub mod drive;
pub mod exclude;
pub mod include;
mod machine_fs;
pub mod outcome;
mod parts;
pub mod raw;
mod start_paths;
mod start_take;
pub mod starts;
pub mod this_machine;
pub mod tree_build;
mod walk;
mod walk_links;

#[cfg(test)]
mod test_support;
