//! 変更のあるファイルを diff の項目にする（docs/ir/cli/symlink-diff.md の REQ-cli-020・024、binary.md の
//! REQ-cli-061、diff-output.md の REQ-cli-060、reference.md の REQ-cli-063・100・101、conflicts.md の
//! REQ-cli-097・099）。中身は比べる流れが読んだものだけを使う

use remote_merge_core::compare::{PathComparison, Role, SideState};
use remote_merge_core::diff::binary::compute_sha256;
use remote_merge_core::diff::engine::is_binary;

use super::diff::Change;
use super::diff_conflicts::{Body, conflicts};
use super::diff_hunks::shown_hunks;
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
/// symlink は変更がなくても項目にするので、そのときはない）
pub fn diff_file(
    path: &str,
    comparison: &PathComparison,
    change: Option<Change>,
    sources: &Sources,
    max_lines: Option<usize>,
) -> DiffOutput {
    let left = body(comparison, Role::Left);
    let right = body(comparison, Role::Right);
    let symlink = is_link(comparison, Role::Left) || is_link(comparison, Role::Right);
    let reference = shown_reference(comparison, sources);
    let texts = text_of(left).zip(text_of(right));
    let mut output = DiffOutput {
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
        one_sided: matches!(change, Some(Change::OneSided(_) | Change::KindDiffers)),
    };
    match texts {
        Some((left_text, right_text)) => {
            let shown = shown_hunks(left_text, right_text, max_lines);
            output.hunks = shown.hunks;
            output.truncated = shown.truncated;
        }
        None => {
            output.left_hash = hash(left);
            output.right_hash = hash(right);
        }
    }
    if reference.is_some() && !symlink && files_only(comparison) {
        with_reference(&mut output, comparison, (left, right), max_lines);
    }
    output
}

/// 参照先との差（左右と参照先がテキストなら左からの差、どれかがバイナリならハッシュ）と競合を足す。参照先が左右のファイルと
/// 種類の違うものか、読めなかったときは、"ref" だけにする
fn with_reference(
    output: &mut DiffOutput,
    comparison: &PathComparison,
    (left, right): (Body, Body),
    max_lines: Option<usize>,
) {
    let reference = match comparison.states.states.get(&Role::Ref) {
        Some(SideState::Absent) => Body::Absent,
        Some(SideState::File { .. }) => match body(comparison, Role::Ref) {
            Body::Bytes(bytes) => Body::Bytes(bytes),
            Body::Absent => return,
        },
        _ => return,
    };
    if let Body::Bytes(bytes) = reference {
        match (text_of(left), text_of(Body::Bytes(bytes))) {
            (Some(left_text), Some(ref_text)) if !output.binary => {
                let shown = shown_hunks(left_text, ref_text, max_lines);
                output.ref_hunks = Some(shown.hunks);
                output.truncated |= shown.truncated;
            }
            // 左右か参照先のどれかがバイナリなら、参照先のハッシュで違いを示す（REQ-cli-100）
            _ => output.ref_hash = Some(compute_sha256(bytes)),
        }
    }
    output.conflict_regions = conflicts(left, right, reference);
    output.conflict_count = output.conflict_regions.len();
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

/// 左右がどちらも通常ファイルか、ないか
fn files_only(comparison: &PathComparison) -> bool {
    [Role::Left, Role::Right].iter().all(|role| {
        matches!(
            comparison.states.states.get(role),
            Some(SideState::File { .. } | SideState::Absent)
        )
    })
}

/// 比べる流れが読んだ `role` の中身
fn body(comparison: &PathComparison, role: Role) -> Body<'_> {
    match comparison.contents.get(&role) {
        Some(Ok(bytes)) => Body::Bytes(bytes),
        Some(Err(_)) | None => Body::Absent,
    }
}

/// テキストとして示せる中身（ない側は空）。バイナリなら（REQ-cli-061）ない
fn text_of(body: Body<'_>) -> Option<&str> {
    match body {
        Body::Absent => Some(""),
        Body::Bytes(bytes) if is_binary(bytes) => None,
        Body::Bytes(bytes) => std::str::from_utf8(bytes).ok(),
    }
}

fn hash(body: Body<'_>) -> Option<String> {
    match body {
        Body::Bytes(bytes) => Some(compute_sha256(bytes)),
        Body::Absent => None,
    }
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
