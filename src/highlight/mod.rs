//! シンタックスハイライト機能。

pub mod cache;
pub mod convert;
pub mod engine;

pub use cache::HighlightCache;
pub use engine::{HighlightedFile, StyledSegment, SyntaxHighlighter};
