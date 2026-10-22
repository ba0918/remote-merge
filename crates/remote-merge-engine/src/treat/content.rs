use crate::{service::types::ConflictArea, side::Side};
use remote_merge_config::content::ContentConfig;
use remote_merge_core::{
    compare::{PathComparison, Role, SideState},
    content::{Content, Interpretation, Text, interpret},
    diff::conflict::text_conflicts,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default)]
pub struct Overrides {
    pub left: Option<Interpretation>,
    pub right: Option<Interpretation>,
    pub reference: Option<Interpretation>,
}

pub struct Context<'a> {
    pub config: &'a ContentConfig,
    pub sides: BTreeMap<Role, Side>,
    pub overrides: Overrides,
}

pub enum Prepared<'a> {
    Absent,
    NonFile,
    Unavailable,
    Binary(&'a [u8]),
    Text(Text<'a>),
}

impl Prepared<'_> {
    pub fn text_or_empty<'b>(&'b self, empty: &'b Text<'static>) -> Option<&'b Text<'b>> {
        match self {
            Self::Absent | Self::NonFile => Some(empty),
            Self::Text(text) => Some(text),
            Self::Unavailable | Self::Binary(_) => None,
        }
    }
    pub fn text(&self) -> Option<&Text<'_>> {
        match self {
            Self::Text(text) => Some(text),
            Self::Absent | Self::NonFile | Self::Unavailable | Self::Binary(_) => None,
        }
    }
    pub fn raw(&self) -> Option<&[u8]> {
        match self {
            Self::Text(text) => Some(text.raw()),
            Self::Binary(bytes) => Some(bytes),
            Self::Absent | Self::NonFile | Self::Unavailable => None,
        }
    }
}

pub struct View<'a> {
    pub left: Result<Prepared<'a>, String>,
    pub right: Result<Prepared<'a>, String>,
    pub reference: Result<Prepared<'a>, String>,
}

pub fn prepare<'a>(
    comparison: &'a PathComparison,
    path: &str,
    context: &Context<'_>,
    reference: bool,
) -> View<'a> {
    let side = |role| -> Result<Prepared<'a>, String> {
        if matches!(comparison.states.states.get(&role), Some(SideState::Absent)) {
            return Ok(Prepared::Absent);
        }
        if matches!(
            comparison.states.states.get(&role),
            Some(
                SideState::Directory
                    | SideState::Symlink {
                        points_to: remote_merge_core::scan::outcome::LinkPoint::Directory,
                        ..
                    }
            )
        ) {
            return Ok(Prepared::NonFile);
        }
        if !matches!(
            comparison.states.states.get(&role),
            Some(SideState::File { .. } | SideState::Symlink { .. })
        ) {
            return Ok(Prepared::Unavailable);
        }
        let Some(bytes) = comparison.contents.get(&role) else {
            return Ok(Prepared::Unavailable);
        };
        let bytes = bytes.as_ref().map_err(|failure| failure.to_string())?;
        let name = context.sides.get(&role).map_or("local", Side::display_name);
        let override_ = match role {
            Role::Left => context.overrides.left,
            Role::Right => context.overrides.right,
            Role::Ref => context.overrides.reference,
            Role::Target => None,
        };
        let specification = context.config.resolve(name, path, override_);
        match interpret(bytes, specification)
            .map_err(|error| format!("{role:?} ({name}): {error}"))?
        {
            Content::Binary => Ok(Prepared::Binary(bytes)),
            Content::Text(text) => Ok(Prepared::Text(text)),
        }
    };
    View {
        left: side(Role::Left),
        right: side(Role::Right),
        reference: if reference {
            side(Role::Ref)
        } else {
            Ok(Prepared::Unavailable)
        },
    }
}

impl View<'_> {
    pub fn conflicts(&self) -> Vec<ConflictArea> {
        let (Ok(left), Ok(right), Ok(reference)) = (&self.left, &self.right, &self.reference)
        else {
            return Vec::new();
        };
        if matches!(left, Prepared::Unavailable | Prepared::NonFile)
            || matches!(right, Prepared::Unavailable | Prepared::NonFile)
            || matches!(reference, Prepared::Unavailable | Prepared::NonFile)
        {
            return Vec::new();
        }
        if let (Prepared::Text(left), Prepared::Text(right), Prepared::Text(reference)) =
            (left, right, reference)
        {
            return text_conflicts(reference, left, right)
                .regions
                .into_iter()
                .map(ConflictArea::Lines)
                .collect();
        }
        let (left, right, reference) = (left.raw(), right.raw(), reference.raw());
        if left != reference && right != reference && left != right {
            vec![ConflictArea::whole_file()]
        } else {
            Vec::new()
        }
    }
    pub fn encodings(&self) -> BTreeMap<String, String> {
        [
            ("left", &self.left),
            ("right", &self.right),
            ("ref", &self.reference),
        ]
        .into_iter()
        .filter_map(|(role, prepared)| {
            prepared
                .as_ref()
                .ok()
                .and_then(Prepared::text)
                .map(|text| (role.to_owned(), text.encoding().name().to_owned()))
        })
        .collect()
    }
}

pub fn empty_text() -> Text<'static> {
    match interpret(b"", Interpretation::Unspecified) {
        Ok(Content::Text(text)) => text,
        Ok(Content::Binary) | Err(_) => unreachable!("empty UTF-8 is text"),
    }
}

#[cfg(test)]
#[path = "content_conflicts_tests.rs"]
mod conflicts_tests;
