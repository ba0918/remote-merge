#![cfg(unix)]
//! 比べる流れが、symlink をリンク文字列で比べ、diff では辿った先の組も同じ表で判定し、参照先も左右と
//! 同じ集め方と判定で比べ、root_dir の外へ解決されるリンクの先を辿らないときは集めないこと
//! （docs/ir/compare/judgement.md の REQ-compare-008、docs/ir/compare/flow.md の REQ-compare-004）の
//! 契約テスト。左はローカル、右と参照先は調べる経路。

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;

use remote_merge::runtime::{CompareScope, ExternalLinks};
use remote_merge_core::compare::{Follow, Method};

use super::compare_support::{comparison_text, request, DiffUses, NothingUsed};
use super::flow_support::{child, Child, FlowRoutes, ROUTES};

fn test_name(name: &str) -> String {
    format!("compare_links::{name}")
}

/// 子のプロセスで、求めの名前の比べ方で比べた結果を答える
fn answer(mut child: Child) {
    let text = match child.request.as_str() {
        "diff and status" => {
            let sides = child.sides(false);
            let diff = request(CompareScope::Whole, Method::Content, Follow::Yes);
            let status = request(CompareScope::Whole, Method::Metadata, Follow::No);
            let diff = comparison_text(child.runtime.compare(&sides, &diff, &DiffUses));
            let status = comparison_text(child.runtime.compare(&sides, &status, &NothingUsed));
            format!("{diff}--\n{status}")
        }
        "diff with the reference without external links" => {
            let sides = child.sides(true);
            let mut diff = request(CompareScope::Whole, Method::Content, Follow::Yes);
            diff.external_links = ExternalLinks::Stop;
            comparison_text(child.runtime.compare(&sides, &diff, &DiffUses))
        }
        "status" => {
            let sides = child.sides(false);
            let status = request(CompareScope::Whole, Method::Metadata, Follow::No);
            comparison_text(child.runtime.compare(&sides, &status, &NothingUsed))
        }
        other => panic!("{other}"),
    };
    child.answer(&text);
}

// @kotowari[REQ-compare-008, REQ-compare-006]
#[test]
fn req_compare_008_links_are_compared_by_text_and_diff_also_compares_their_targets_on_all_three_routes(
) {
    let name = "req_compare_008_links_are_compared_by_text_and_diff_also_compares_their_targets_on_all_three_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        for (root, content, nanos) in [(left, "alpha\n", 1), (right, "bravo\n", 2)] {
            fs::write(root.join("t.txt"), content).unwrap();
            // メタデータで比べても中身で決めるよう、更新時刻を違えておく
            let mtime = std::time::UNIX_EPOCH + std::time::Duration::from_nanos(nanos);
            fs::File::options()
                .write(true)
                .open(root.join("t.txt"))
                .unwrap()
                .set_modified(mtime)
                .unwrap();
            symlink("t.txt", root.join("same-text")).unwrap();
            symlink("gone", root.join("dangling")).unwrap();
        }
        symlink("t.txt", left.join("other-text")).unwrap();
        symlink("x.txt", right.join("other-text")).unwrap();
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "diff and status");
        let diff = "dangling left-right=same(followed both-absent)\n\
             other-text left-right=differs(followed first-only)\n\
             same-text left-right=differs(followed differs)\n\
             t.txt left-right=differs\n\
             asked left hash=[\"same-text\", \"t.txt\"] read=[\"other-text\", \"same-text\", \"t.txt\"]\n\
             asked right hash=[\"same-text\", \"t.txt\"] read=[\"same-text\", \"t.txt\"]\n";
        let status = "dangling left-right=same\n\
             other-text left-right=differs\n\
             same-text left-right=same\n\
             t.txt left-right=differs\n\
             asked left hash=[\"t.txt\"] read=[]\n\
             asked right hash=[\"t.txt\"] read=[]\n";
        assert_eq!(text, format!("{diff}--\n{status}"), "{route:?}");
    }
}

// @kotowari[REQ-compare-004, EX-compare-004]
#[test]
fn ex_compare_004_the_content_outside_the_reference_root_dir_is_not_compared_on_all_three_routes() {
    let name = "ex_compare_004_the_content_outside_the_reference_root_dir_is_not_compared_on_all_three_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::with_roots(|roots| {
        let outside = roots.reference.parent().unwrap().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("a.txt"), "outside\n").unwrap();
        symlink(&outside, roots.reference.join("shared")).unwrap();
        for (root, content) in [(&roots.left, "left\n"), (&roots.right, "right\n")] {
            fs::create_dir(root.join("shared")).unwrap();
            fs::write(root.join("shared/a.txt"), content).unwrap();
        }
    });

    for route in ROUTES {
        let text = routes.run(
            route,
            &test_name(name),
            "diff with the reference without external links",
        );
        assert_eq!(
            text,
            "shared left-right=same left-ref=kind-differs right-ref=kind-differs outside:ref\n\
             shared/a.txt left-right=differs not-collected:ref\n\
             asked left hash=[] read=[\"shared/a.txt\"]\n\
             asked right hash=[] read=[\"shared/a.txt\"]\n",
            "{route:?}"
        );
    }
}

// @kotowari[REQ-compare-008]
#[test]
fn req_compare_008_link_texts_that_are_not_utf8_are_compared_as_bytes_on_all_three_routes() {
    let name =
        "req_compare_008_link_texts_that_are_not_utf8_are_compared_as_bytes_on_all_three_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        for root in [left, right] {
            symlink(OsStr::from_bytes(b"t\xfd"), root.join("same-bytes")).unwrap();
        }
        symlink(OsStr::from_bytes(b"t\xff"), left.join("other-bytes")).unwrap();
        symlink(OsStr::from_bytes(b"t\xfe"), right.join("other-bytes")).unwrap();
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "status");
        assert_eq!(
            text,
            "other-bytes left-right=differs\n\
             same-bytes left-right=same\n",
            "{route:?}"
        );
    }
}

// @kotowari[REQ-compare-009, REQ-compare-008]
#[test]
fn req_compare_009_links_whose_targets_are_of_another_kind_are_undecidable_before_reading_on_all_three_routes(
) {
    let name = "req_compare_009_links_whose_targets_are_of_another_kind_are_undecidable_before_reading_on_all_three_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        for root in [left, right] {
            super::scan_other_kinds::make_fifo(&root.join("pipe"));
            symlink("pipe", root.join("to-pipe")).unwrap();
        }
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "diff and status");
        let diff = text.split("--\n").next().unwrap();
        assert!(
            diff.contains("to-pipe left-right=undecidable(not a regular file)(followed undecidable(not a regular file))"),
            "{route:?}: {text}"
        );
        assert!(!text.contains("hash=[\"to-pipe\"]"), "{route:?}: {text}");
    }
}
