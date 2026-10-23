#![warn(clippy::wildcard_enum_match_arm)]
#![warn(clippy::unwrap_used)]
#![deny(clippy::print_stdout, clippy::print_stderr)]

// バイト列のパス・デバイス番号と inode 番号・権限のビット・リンクを辿らずに開くこと・symlink の作成を
// 前提にしていて、Unix 以外の代わりの道は持たない
#[cfg(not(unix))]
compile_error!("remote-merge builds only on Unix (Linux and macOS)");

pub mod change_check;
pub mod compare;
pub mod content;
pub mod diff;
pub mod error;
pub mod filter;
pub mod machine_ops;
pub mod op_failure;
pub mod path_resolution;
pub mod relative_path;
pub mod route_choice;
pub mod scan;
pub mod sha256;
#[cfg(all(unix, any(test, feature = "test-utils")))]
pub mod test_tree;
pub mod unreadable_name;
