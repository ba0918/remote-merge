use super::content::{Prepared, View, empty_text};
use remote_merge_core::{
    content::{Content, Interpretation, interpret},
    diff::engine::{HunkDirection, PairScript},
};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkMerge {
    NoChanges,
    Merged { content: String, total: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkError {
    Binary,
    NotRegular,
    OutOfRange { index: usize, total: usize },
    Conflict,
    Invalid(String),
}

pub fn merge_prepared(
    view: &View<'_>,
    selected: &[usize],
    reference: bool,
) -> Result<Option<(Vec<u8>, usize)>, HunkError> {
    let left = view
        .left
        .as_ref()
        .map_err(|reason| HunkError::Invalid(reason.clone()))?;
    let right = view
        .right
        .as_ref()
        .map_err(|reason| HunkError::Invalid(reason.clone()))?;
    if matches!(left, Prepared::Binary(_)) || matches!(right, Prepared::Binary(_)) {
        return Err(HunkError::Binary);
    }
    if matches!(left, Prepared::NonFile | Prepared::Unavailable)
        || matches!(right, Prepared::NonFile | Prepared::Unavailable)
    {
        return Err(HunkError::NotRegular);
    }
    if reference {
        view.reference.as_ref().map_err(|reason| {
            HunkError::Invalid(format!("three-way comparison incomplete: {reason}"))
        })?;
    }
    let empty = empty_text();
    let left_text = left.text_or_empty(&empty).ok_or(HunkError::NotRegular)?;
    let right_text = right.text_or_empty(&empty).ok_or(HunkError::NotRegular)?;
    if !matches!(left, Prepared::Absent)
        && !matches!(right, Prepared::Absent)
        && left_text.encoding() != right_text.encoding()
    {
        return Err(HunkError::Invalid(
            "Hunk merge requires the same content encoding on both sides".into(),
        ));
    }
    if reference && matches!(&view.reference, Ok(Prepared::Unavailable)) {
        return Err(HunkError::Invalid("three-way comparison incomplete".into()));
    }
    if reference && !view.conflicts().is_empty() {
        return Err(HunkError::Conflict);
    }
    let script = PairScript::new(left_text, right_text);
    let Some(total) = script.hunk_count() else {
        return Ok(None);
    };
    if let Some(index) = selected.iter().find(|index| **index >= total) {
        return Err(HunkError::OutOfRange {
            index: *index,
            total,
        });
    }
    let mut content = script.apply(
        &selected.iter().copied().collect::<HashSet<_>>(),
        HunkDirection::LeftToRight,
    );
    if matches!(right, Prepared::Absent) {
        let mut with_bom = left_text.bom().to_vec();
        with_bom.append(&mut content);
        content = with_bom;
    }
    Ok(Some((content, total)))
}

pub fn merge_selected_hunks(
    left: &str,
    right: &str,
    reference: Option<&[u8]>,
    selected: &[usize],
) -> Result<HunkMerge, HunkError> {
    let decode = |bytes| match interpret(bytes, Interpretation::Unspecified) {
        Ok(Content::Text(text)) => Ok(Prepared::Text(text)),
        Ok(Content::Binary) => Ok(Prepared::Binary(bytes)),
        Err(error) => Err(error.to_string()),
    };
    let view = View {
        left: decode(left.as_bytes()),
        right: decode(right.as_bytes()),
        reference: reference.map_or(Ok(Prepared::Unavailable), decode),
    };
    match merge_prepared(&view, selected, reference.is_some())? {
        None => Ok(HunkMerge::NoChanges),
        Some((content, total)) => Ok(HunkMerge::Merged {
            content: String::from_utf8(content).expect("UTF-8 spans form UTF-8 output"),
            total,
        }),
    }
}

#[cfg(test)]
#[path = "hunk_merge_tests.rs"]
mod tests;
