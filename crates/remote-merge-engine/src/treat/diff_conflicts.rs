use super::content::{Prepared, View};
use crate::service::types::ConflictArea;
use remote_merge_core::content::{Content, Interpretation, interpret};

#[derive(Debug, Clone, Copy)]
pub enum Body<'a> {
    Absent,
    Bytes(&'a [u8]),
}

pub fn conflicts(left: Body, right: Body, reference: Body) -> Vec<ConflictArea> {
    let decode = |body| match body {
        Body::Absent => Ok(Prepared::Absent),
        Body::Bytes(bytes) => match interpret(bytes, Interpretation::Unspecified) {
            Ok(Content::Text(text)) => Ok(Prepared::Text(text)),
            Ok(Content::Binary) => Ok(Prepared::Binary(bytes)),
            Err(error) => Err(error.to_string()),
        },
    };
    View {
        left: decode(left),
        right: decode(right),
        reference: decode(reference),
    }
    .conflicts()
}

#[cfg(test)]
#[path = "diff_conflicts_tests.rs"]
mod tests;
