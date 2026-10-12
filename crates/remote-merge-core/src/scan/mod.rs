//! 走査の規則（docs/ir/scan/ と docs/ir/config/filters.md・include.md）。
//!
//! 規則は一か所にあり、経路は [`raw::Collector`] として生の項目を集めるだけにする
//! （docs/decision/records/2026-10-12-foundation-2.md の C6）。

pub mod drive;
pub mod exclude;
pub mod include;
pub mod outcome;
mod parts;
pub mod raw;
pub mod tree_build;
mod walk;

#[cfg(test)]
mod test_support;
