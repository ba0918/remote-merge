#![cfg(unix)]
//! 比べる流れが、ハッシュに大きさの上限を当てず中身の読み込みにだけ当て、その他の種類を読まないこと
//! （docs/ir/compare/reading.md の REQ-compare-010・011、docs/ir/compare/judgement.md の
//! REQ-compare-009）の契約テスト。左はローカル、右は調べる経路。

use super::compare_reading::{answer, write_both};
use super::flow_support::{FlowRoutes, ROUTES, child};
use super::scan_other_kinds::make_fifo;

// @kotowari[REQ-compare-010, REQ-compare-011, EX-compare-012, EX-compare-013]
#[test]
fn ex_compare_012_013_hashes_have_no_size_limit_and_only_contents_read_for_use_do_on_all_three_routes()
 {
    let name = "ex_compare_012_013_hashes_have_no_size_limit_and_only_contents_read_for_use_do_on_all_three_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        write_both(
            left,
            right,
            "same.bin",
            "0123456789abcdef",
            "0123456789abcdef",
        );
        write_both(
            left,
            right,
            "other.bin",
            "0123456789abcdef",
            "fedcba9876543210",
        );
    });

    for route in ROUTES {
        let text = routes.run(
            route,
            &format!("compare_reading_kinds::{name}"),
            "limit to 8 bytes",
        );
        assert_eq!(
            text,
            "other.bin left-right=undecidable(read failed: \
             left: file too large (16 bytes, the limit is 8 bytes; use --force to read it); \
             right: file too large (16 bytes, the limit is 8 bytes; use --force to read it))\n\
             same.bin left-right=same\n\
             asked left hash=[\"other.bin\", \"same.bin\"] read=[\"other.bin\"]\n\
             asked right hash=[\"other.bin\", \"same.bin\"] read=[\"other.bin\"]\n",
            "{route:?}"
        );
    }
}

// @kotowari[REQ-compare-009, REQ-scan-021]
#[test]
fn req_compare_009_another_kind_is_undecidable_without_being_read_on_all_three_routes() {
    let name = "req_compare_009_another_kind_is_undecidable_without_being_read_on_all_three_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        make_fifo(&left.join("pipe"));
        make_fifo(&right.join("pipe"));
    });

    for route in ROUTES {
        let text = routes.run(route, &format!("compare_reading_kinds::{name}"), "as it is");
        assert_eq!(
            text, "pipe left-right=undecidable(not a regular file: left; right)\n",
            "{route:?}"
        );
    }
}
