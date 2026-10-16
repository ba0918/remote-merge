#![warn(clippy::wildcard_enum_match_arm)]
#![warn(clippy::unwrap_used)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
pub mod compare;
pub mod content;
pub mod diff;
pub mod error;
pub mod filter;
pub mod machine_ops;
pub mod op_failure;
pub mod operations;
pub mod path_resolution;
pub mod relative_path;
pub mod route_choice;
pub mod scan;
#[cfg(all(unix, any(test, feature = "test-utils")))]
pub mod test_tree;
pub mod unreadable_name;
