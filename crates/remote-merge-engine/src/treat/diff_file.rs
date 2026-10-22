//! 変更のあるファイルを diff の項目にする（docs/ir/cli/symlink-diff.md の REQ-cli-020・024、binary.md の
//! REQ-cli-061、diff-output.md の REQ-cli-060、reference.md の REQ-cli-063・100・101、conflicts.md の
//! REQ-cli-097・099）。中身は比べる流れが読んだものだけを使う

use remote_merge_core::compare::{PathComparison, Role, SideState};
use remote_merge_core::diff::engine::PairScript;
use remote_merge_core::sha256::hex_of_bytes;

use super::content::{Prepared, View, empty_text};
use super::diff::Change;
use super::diff_hunks::shown_script;
use super::uses::sides_are_files_or_absent;
use crate::service::types::{DiffOutput, LinkTargets, SourceInfo};

/// 項目に載せる左右と参照先の名前と root_dir
#[derive(Debug, Clone)]
pub struct Sources {
    pub left: SourceInfo,
    pub right: SourceInfo,
    /// --ref の参照先（使わないときはない）
    pub reference: Option<SourceInfo>,
}

/// `path` の項目。`change` は判定による変更の種類（root_dir の外へ解決されて中身を比べなかった
/// symlink は変更がなくても項目にするので、そのときはない）。`view` は内容の設定で解釈した中身
pub(crate) fn prepared_file(
    path: &str,
    comparison: &PathComparison,
    change: Option<Change>,
    sources: &Sources,
    max_lines: Option<usize>,
    view: &View<'_>,
) -> DiffOutput {
    let empty = empty_text();
    let texts = view
        .left
        .as_ref()
        .ok()
        .and_then(|side| side.text_or_empty(&empty))
        .zip(
            view.right
                .as_ref()
                .ok()
                .and_then(|side| side.text_or_empty(&empty)),
        );
    let symlink = is_link(comparison, Role::Left) || is_link(comparison, Role::Right);
    let reference = shown_reference(comparison, sources);
    let mut output = DiffOutput {
        encodings: view.encodings(),
        byte_only_difference: false,
        path: path.to_string(),
        left: sources.left.clone(),
        right: sources.right.clone(),
        ref_: reference.cloned(),
        binary: texts.is_none(),
        symlink,
        link_targets: symlink.then(|| LinkTargets {
            left: link_target(comparison, Role::Left),
            right: link_target(comparison, Role::Right),
            ref_: reference.map(|_| link_target(comparison, Role::Ref)),
        }),
        truncated: false,
        hunks: Vec::new(),
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: (change == Some(Change::KindDiffers)).then(|| {
            format!(
                "type mismatch: {} vs {}",
                kind(comparison, Role::Left),
                kind(comparison, Role::Right)
            )
        }),
        conflict_count: 0,
        conflict_regions: Vec::new(),
    };
    match texts {
        Some((left_text, right_text)) => {
            let shown = shown_script(PairScript::new(left_text, right_text).display(), max_lines);
            output.hunks = shown.hunks;
            output.truncated = shown.truncated;
            if left_text.text() == right_text.text() && left_text.raw() != right_text.raw() {
                output.byte_only_difference = true;
                output.left_hash = Some(hex_of_bytes(left_text.raw()));
                output.right_hash = Some(hex_of_bytes(right_text.raw()));
                output.note =
                    Some("byte-only representation difference; decoded text is equal".into());
            }
        }
        None => {
            output.left_hash = view
                .left
                .as_ref()
                .ok()
                .and_then(Prepared::raw)
                .map(hex_of_bytes);
            output.right_hash = view
                .right
                .as_ref()
                .ok()
                .and_then(Prepared::raw)
                .map(hex_of_bytes);
        }
    }
    if reference.is_some() && !symlink && sides_are_files_or_absent(&comparison.states) {
        if let Ok(Prepared::Text(reference)) = &view.reference {
            if let Some((left, _)) = texts {
                let shown = shown_script(PairScript::new(left, reference).display(), max_lines);
                output.ref_hunks = Some(shown.hunks);
                output.truncated |= shown.truncated;
            } else {
                output.ref_hash = Some(hex_of_bytes(reference.raw()));
            }
        } else if let Ok(Prepared::Binary(reference)) = &view.reference {
            output.ref_hash = Some(hex_of_bytes(reference));
        }
        output.conflict_regions = view.conflicts();
        output.conflict_count = output.conflict_regions.len();
    }
    output
}

/// 参照先を項目に付けるか。参照先の状態を集め、どの側も root_dir の外へ解決されないときだけ付ける
fn shown_reference<'a>(
    comparison: &PathComparison,
    sources: &'a Sources,
) -> Option<&'a SourceInfo> {
    let states = &comparison.states;
    let collected = states.states.contains_key(&Role::Ref) && states.outside.is_empty();
    sources.reference.as_ref().filter(|_| collected)
}

fn is_link(comparison: &PathComparison, role: Role) -> bool {
    matches!(
        comparison.states.states.get(&role),
        Some(SideState::Symlink { .. })
    )
}

fn link_target(comparison: &PathComparison, role: Role) -> Option<String> {
    match comparison.states.states.get(&role) {
        Some(SideState::Symlink { target, .. }) => {
            Some(String::from_utf8_lossy(target).into_owned())
        }
        _ => None,
    }
}

/// note に出す、判定に使った種類
fn kind(comparison: &PathComparison, role: Role) -> &'static str {
    match comparison.states.states.get(&role) {
        Some(SideState::File { .. }) => "file",
        Some(SideState::Directory) => "directory",
        Some(SideState::Symlink { .. }) => "symlink",
        Some(SideState::Other) => "other",
        Some(SideState::UnreadableName) => "unreadable name",
        Some(SideState::Absent) | None => "missing",
    }
}

#[cfg(test)]
#[path = "diff_file_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "diff_file_ref_tests.rs"]
mod ref_tests;
