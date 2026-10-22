#![warn(clippy::wildcard_enum_match_arm)]
#![warn(clippy::unwrap_used)]
#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
pub mod backup;
pub mod backup_store;
pub mod child_listing;
pub mod local_io;
pub mod scan_result;
pub mod service;
pub mod side;
pub mod treat;
