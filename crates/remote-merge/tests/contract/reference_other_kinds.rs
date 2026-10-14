#![cfg(unix)]
//! --ref の参照先の同じパスがその他の種類（FIFO）のとき、status と merge が参照先にないとせず、
//! 参照先との組を判定できない（理由は "not a regular file"）とし、左右の判定は残して終了コード 2 に
//! すること（docs/ir/compare/judgement.md の REQ-compare-009、docs/ir/compare/flow.md の
//! REQ-compare-004、docs/decision/records/2026-10-12-foundation-4.md の E7・E8）の契約テスト。
//! 左はローカル、右と参照先は調べる経路。

use std::fs;

use remote_merge::cli::diff::{execute_diff, DiffArgs};
use remote_merge::cli::merge::{execute_merge, MergeCommandOutput};
use remote_merge::cli::status::{execute_status, StatusArgs};
use remote_merge::service::output::format_json;
use serde_json::Value;

use super::backup_support::merge_args;
use super::flow_support::{child, Child, FlowRoutes, ROUTES};
use super::scan_other_kinds::make_fifo;

const THIS_TEST: &str = "reference_other_kinds::a_fifo_in_the_reference_is_undecidable_against_the_reference_and_not_missing_on_all_three_routes";

/// 子のプロセスで、参照先を付けた status と merge の JSON を "--" でつないで答える
fn answer(child: Child) {
    let status = execute_status(
        StatusArgs {
            left: Some("local".into()),
            right: Some("fixture".into()),
            ref_server: Some("reference".into()),
            format: "json".into(),
            summary: false,
            all: true,
            checksum: false,
            verbose: 0,
            max_entries: None,
        },
        child.config.clone(),
        child.targets.clone(),
    )
    .unwrap();
    let mut merge = merge_args("m.txt");
    merge.right = Some("fixture".into());
    merge.ref_server = Some("reference".into());
    let merged = execute_merge(merge, child.config.clone(), child.targets.clone()).unwrap();
    let MergeCommandOutput::Files(merged_output) = &merged.output else {
        panic!("expected the merged files");
    };
    let diff = DiffArgs {
        paths: vec!["m.txt".into()],
        left: Some("local".into()),
        right: Some("fixture".into()),
        ref_server: Some("reference".into()),
        format: "json".into(),
        max_lines: None,
        max_files: 100,
        force: false,
        follow_external_links: false,
        max_entries: None,
    };
    let (diffed, diff_code) =
        execute_diff(diff, child.config.clone(), child.targets.clone()).unwrap();
    let text = format!(
        "{}\n{}\n--\n{}\n{}\n--\n{}\n{}",
        status.exit_code,
        format_json(&status.output).unwrap(),
        merged.exit_code,
        format_json(merged_output).unwrap(),
        diff_code,
        format_json(&diffed).unwrap()
    );
    child.answer(&text);
}

fn reported_with_reason(json: &Value, key: &str, path: &str) -> bool {
    json[key].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["path"] == path && item.to_string().contains("not a regular file"))
    })
}

// @kotowari[REQ-compare-009, REQ-compare-004]
#[test]
fn a_fifo_in_the_reference_is_undecidable_against_the_reference_and_not_missing_on_all_three_routes(
) {
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::with_roots(|roots| {
        fs::write(roots.left.join("a.txt"), "same\n").unwrap();
        fs::write(roots.right.join("a.txt"), "same\n").unwrap();
        make_fifo(&roots.reference.join("a.txt"));
        fs::write(roots.left.join("m.txt"), "new content\n").unwrap();
        fs::write(roots.right.join("m.txt"), "old\n").unwrap();
        make_fifo(&roots.reference.join("m.txt"));
    });

    for route in ROUTES {
        let text = routes.run(route, THIS_TEST, "status and merge");
        let (status, rest) = text.split_once("\n--\n").unwrap();
        let (merge, diff) = rest.split_once("\n--\n").unwrap();
        let (status_code, status) = status.split_once('\n').unwrap();
        let (merge_code, merge) = merge.split_once('\n').unwrap();
        let status: Value = serde_json::from_str(status).unwrap();
        let merge: Value = serde_json::from_str(merge).unwrap();

        assert_eq!(status_code, "2", "{route:?}: {status}");
        assert!(
            reported_with_reason(&status, "errors", "a.txt"),
            "{route:?}: {status}"
        );
        let a = status["files"]
            .as_array()
            .and_then(|files| files.iter().find(|file| file["path"] == "a.txt"))
            .unwrap_or_else(|| panic!("{route:?}: a.txt missing: {status}"));
        assert_eq!(a["status"], "equal", "{route:?}: {status}");
        assert!(a.get("ref_badge").is_none(), "{route:?}: {status}");

        assert_eq!(merge_code, "2", "{route:?}: {merge}");
        assert!(
            reported_with_reason(&merge, "failed", "m.txt"),
            "{route:?}: {merge}"
        );
        assert_eq!(
            fs::read_to_string(routes.roots(route).right.join("m.txt")).unwrap(),
            "old\n",
            "{route:?}: {merge}"
        );

        let (diff_code, diff) = diff.split_once('\n').unwrap();
        let diff: Value = serde_json::from_str(diff).unwrap();
        assert_eq!(diff_code, "2", "{route:?}: {diff}");
        assert!(
            reported_with_reason(&diff, "errors", "m.txt"),
            "{route:?}: {diff}"
        );
    }
}
