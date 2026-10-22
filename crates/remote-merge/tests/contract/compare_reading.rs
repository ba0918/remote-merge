#![expect(
    unsafe_code,
    reason = "テストは権限・FIFO・時刻を直接扱うため libc を呼ぶ"
)]
#![cfg(unix)]
//! 比べる流れが、読み込みの失敗をどの種類も判定できないとし、ないことを状態からだけ決め、ハッシュに
//! 大きさの上限を当てず中身の読み込みにだけ当て、その他の種類を読まないこと（docs/ir/compare/flow.md の
//! REQ-compare-003、docs/ir/compare/reading.md の REQ-compare-010〜012・014、
//! docs/ir/compare/judgement.md の REQ-compare-009）の契約テスト。左はローカル、右は調べる経路。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge::runtime::CompareScope;
use remote_merge_core::compare::{Follow, Method};
use remote_merge_core::op_failure::SizeLimit;

use super::common::make_fifo;
use super::compare_support::{DiffUses, comparison_text, request};
use super::flow_support::{Child, FlowRoutes, ROUTES, child};

fn test_name(name: &str) -> String {
    format!("compare_reading::{name}")
}

/// root でないときだけ、権限で読めない状態を作れる
fn is_root() -> bool {
    // SAFETY: geteuid は引数を取らず、常に成功する
    unsafe { libc::geteuid() == 0 }
}

/// 子のプロセスで、状態を集めた後に求めの名前の変更をしてから、中身で比べた結果を答える
pub(super) fn answer(mut child: Child) {
    let sides = child.sides(false);
    let mut asked = request(CompareScope::Whole, Method::Content, Follow::No);
    let right = child.roots.right.clone();
    let between: Box<dyn Fn()> = match child.request.as_str() {
        "as it is" => Box::new(|| {}),
        "remove a.txt" => Box::new(move || fs::remove_file(right.join("a.txt")).unwrap()),
        "make d a file" => Box::new(move || {
            fs::remove_dir_all(right.join("d")).unwrap();
            fs::write(right.join("d"), "x\n").unwrap();
        }),
        "make a.txt a fifo" => Box::new(move || {
            fs::remove_file(right.join("a.txt")).unwrap();
            make_fifo(&right.join("a.txt"));
        }),
        "limit to 8 bytes" => {
            asked.content_limit = SizeLimit::Bytes(8);
            Box::new(|| {})
        }
        other => panic!("{other}"),
    };
    let text = child
        .runtime
        .collect_compare_states(&sides, &asked)
        .and_then(|collected| {
            between();
            child
                .runtime
                .compare_collected(&sides, collected, &asked, &DiffUses)
        });
    let text = comparison_text(text);
    child.answer(&text);
}

pub(super) fn write_both(
    left: &std::path::Path,
    right: &std::path::Path,
    path: &str,
    l: &str,
    r: &str,
) {
    for (root, content) in [(left, l), (right, r)] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, content).unwrap();
    }
}

// @kotowari[REQ-compare-003, REQ-compare-012, EX-compare-003, EX-compare-014]
#[test]
fn ex_compare_003_a_file_removed_before_it_was_read_is_undecidable_on_both_routes() {
    let name = "ex_compare_003_a_file_removed_before_it_was_read_is_undecidable_on_both_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes =
        FlowRoutes::new(|left, right| write_both(left, right, "a.txt", "alpha\n", "bravo\n"));

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "remove a.txt");
        assert_eq!(
            text,
            "a.txt left-right=undecidable(read failed: right: no such file or directory)\n\
             asked left hash=[\"a.txt\"] read=[]\n\
             asked right hash=[\"a.txt\"] read=[]\n",
            "{route:?}"
        );
    }
}

// @kotowari[REQ-compare-012, REQ-compare-014, REQ-paths-021, EX-compare-014]
#[test]
fn ex_compare_014_each_kind_of_read_failure_is_undecidable_on_both_routes() {
    let name = "ex_compare_014_each_kind_of_read_failure_is_undecidable_on_both_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        write_both(left, right, "a.txt", "alpha\n", "bravo\n");
        write_both(left, right, "d/b.txt", "alpha\n", "bravo\n");
        write_both(left, right, "locked.txt", "alpha\n", "bravo\n");
        fs::set_permissions(right.join("locked.txt"), fs::Permissions::from_mode(0o000)).unwrap();
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "make d a file");
        let locked = if is_root() {
            "locked.txt left-right=differs".to_string()
        } else {
            "locked.txt left-right=undecidable(read failed: right: permission denied)".to_string()
        };
        assert!(
            text.contains("d/b.txt left-right=undecidable(read failed: right: not a directory)\n"),
            "{route:?}: {text}"
        );
        assert!(text.contains(&format!("{locked}\n")), "{route:?}: {text}");
        let fifo = routes.run(route, &test_name(name), "make a.txt a fifo");
        assert!(
            fifo.contains("a.txt left-right=undecidable(read failed: right: not a regular file)\n"),
            "{route:?}: {fifo}"
        );
    }
}

// @kotowari[REQ-compare-014, EX-compare-016]
#[test]
fn ex_compare_016_the_reason_names_both_sides_that_could_not_be_read_on_both_routes() {
    let name = "ex_compare_016_the_reason_names_both_sides_that_could_not_be_read_on_both_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    if is_root() {
        return;
    }
    let routes = FlowRoutes::new(|left, right| {
        write_both(left, right, "d/a.txt", "alpha\n", "bravo\n");
        fs::set_permissions(left.join("d/a.txt"), fs::Permissions::from_mode(0o000)).unwrap();
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "make d a file");
        assert!(
            text.contains(
                "d/a.txt left-right=undecidable(read failed: left: permission denied; right: not a directory)\n"
            ),
            "{route:?}: {text}"
        );
    }
}
