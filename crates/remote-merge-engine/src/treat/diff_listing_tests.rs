//! 比べた結果から diff の files・errors・summary を作る規則（REQ-cli-053・055・026・089・090、
//! TBL-compare-005）の単体テスト
use std::collections::{BTreeMap, BTreeSet};

use remote_merge_core::compare::{
    PathComparison, Role, SideFailure, SideState, Undecidable, Verdict,
};
use remote_merge_core::op_failure::OpFailure;

use super::super::content::Overrides;
use super::super::diff_file::Sources;
use super::super::test_support::{compared_with, file, link, pair};
use super::*;
use crate::service::types::SourceInfo;

/// 内容の設定と指定がないときの diff の出力
pub(super) fn diff_listing(
    paths: &BTreeMap<String, PathComparison>,
    sources: &Sources,
    limits: Limits,
    coverage: &Coverage,
) -> MultiDiffOutput {
    let config = Default::default();
    let context = Context {
        config: &config,
        sides: Default::default(),
        overrides: Overrides::default(),
    };
    diff_listing_with_content(paths, sources, limits, coverage, &context)
}

pub(super) fn sources(with_ref: bool) -> Sources {
    let source = |label: &str| SourceInfo {
        label: label.into(),
        root: label.into(),
    };
    Sources {
        left: source("l"),
        right: source("r"),
        reference: with_ref.then(|| source("ref")),
    }
}

fn files_pair(verdict: Verdict) -> PathComparison {
    compared_with(
        &[(Role::Left, file()), (Role::Right, file())],
        vec![pair(Role::Left, Role::Right, verdict)],
        &[(Role::Left, b"l\n"), (Role::Right, b"r\n")],
    )
}

pub(super) fn limits(max_files: usize) -> Limits {
    Limits {
        max_lines: None,
        max_files,
    }
}

pub(super) fn whole() -> Coverage<'static> {
    Coverage {
        named: &[],
        incomplete: BTreeSet::new(),
    }
}

pub(super) fn paths(entries: Vec<(&str, PathComparison)>) -> BTreeMap<String, PathComparison> {
    entries
        .into_iter()
        .map(|(path, comparison)| (path.to_string(), comparison))
        .collect()
}

fn denied(role: Role) -> Verdict {
    Verdict::Undecidable(Undecidable::ReadFailed(vec![SideFailure {
        role,
        failure: OpFailure::PermissionDenied,
    }]))
}

// @kotowari[REQ-cli-053, REQ-cli-089, TBL-compare-005]
#[test]
fn changed_files_are_listed_and_undecidable_ones_are_errors() {
    let directory = compared_with(
        &[
            (Role::Left, SideState::Directory),
            (Role::Right, SideState::Directory),
        ],
        vec![pair(Role::Left, Role::Right, Verdict::Same)],
        &[],
    );
    let listed = paths(vec![
        ("a", files_pair(Verdict::Differs)),
        ("b", files_pair(Verdict::Same)),
        ("c", files_pair(denied(Role::Right))),
        ("d", directory),
    ]);

    let output = diff_listing(&listed, &sources(false), limits(100), &whole());

    let shown: Vec<&str> = output.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(shown, ["a"]);
    assert_eq!(output.errors.len(), 1);
    assert_eq!(output.errors[0].path, "c");
    assert_eq!(
        output.errors[0].reason,
        "read failed: right: permission denied"
    );
    assert_eq!(output.summary.files_with_changes, 1);
    assert_eq!(output.summary.scanned_files, 3);
    assert!(!output.truncated && output.changed_files_total.is_none());
}

// @kotowari[REQ-cli-055]
#[test]
fn files_over_max_files_are_not_shown_and_the_total_is_kept() {
    let listed = paths(vec![
        ("a", files_pair(Verdict::Differs)),
        ("b", files_pair(Verdict::Differs)),
        ("c", files_pair(Verdict::Differs)),
    ]);

    let output = diff_listing(&listed, &sources(true), limits(2), &whole());
    assert_eq!(output.files.len(), 2);
    assert!(output.truncated);
    assert_eq!(output.changed_files_total, Some(3));
    assert_eq!(output.summary.files_with_changes, 2);

    let all = diff_listing(&listed, &sources(true), limits(0), &whole());
    assert_eq!(all.files.len(), 3);
    assert!(!all.truncated);
}

// @kotowari[REQ-cli-026, REQ-cli-024]
#[test]
fn a_link_outside_root_dir_is_shown_and_reported_as_not_compared() {
    let mut outside = compared_with(
        &[(Role::Left, link("/x")), (Role::Right, link("/x"))],
        vec![pair(Role::Left, Role::Right, Verdict::Same)],
        &[],
    );
    outside.states.outside.insert(Role::Left);

    let output = diff_listing(
        &paths(vec![("l", outside)]),
        &sources(false),
        limits(100),
        &whole(),
    );

    assert_eq!(output.files.len(), 1);
    assert_eq!(output.summary.files_with_changes, 0);
    assert_eq!(output.errors[0].path, "l");
    let reason = &output.errors[0].reason;
    assert!(
        reason.contains("outside root_dir") && reason.contains("not compared"),
        "{reason}"
    );
}

// @kotowari[REQ-cli-090, REQ-compare-004]
#[test]
fn an_undecidable_ref_pair_keeps_the_file_and_is_an_error() {
    let mut comparison = files_pair(Verdict::Differs);
    comparison.states.states.insert(Role::Ref, file());
    comparison
        .pairs
        .push(pair(Role::Left, Role::Ref, denied(Role::Ref)));
    comparison
        .pairs
        .push(pair(Role::Right, Role::Ref, denied(Role::Ref)));

    let output = diff_listing(
        &paths(vec![("a", comparison)]),
        &sources(true),
        limits(100),
        &whole(),
    );

    assert_eq!(output.files.len(), 1);
    assert!(output.files[0].ref_.is_some() && output.files[0].ref_hunks.is_none());
    assert_eq!(output.errors.len(), 1);
    assert_eq!(
        output.errors[0].reason,
        "read failed: ref: permission denied"
    );
}
