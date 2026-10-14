#![deny(clippy::print_stdout, clippy::print_stderr)]
pub mod agent;

pub(crate) use remote_merge_ssh as ssh;
