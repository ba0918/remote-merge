#![cfg(unix)]
//! status・diff・merge・sync が、その他の種類（FIFO・ソケット）を一覧から落とさず、判定できない
//! （理由は "not a regular file"）として errors・failed に出して終了コード 2 にし、書き込み先の
//! その他の種類も、読み込み元がその他の種類の書き込み先の通常ファイルも、--delete でも書き換えも
//! 消しもしないこと（docs/ir/compare/judgement.md の REQ-compare-009、docs/decision/records/
//! 2026-10-12-foundation-4.md の E8・E29）の CLI の契約テスト。試す側は三つの経路のそれぞれ。

use std::fs;
use std::process::Output;

use serde_json::Value;

use super::scan_missing_root::{Sides, ROUTES};
use super::scan_other_kinds::{make_fifo, make_socket};

const NOT_REGULAR: &str = "not a regular file";

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"))
}

/// `key` の配列に、`path` のパスで、理由に "not a regular file" を含む項目があるか
fn reported(output: &Output, key: &str, path: &str) -> bool {
    json(output)[key].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["path"] == path && item.to_string().contains(NOT_REGULAR))
    })
}

// @kotowari[REQ-compare-009, EX-compare-010]
#[tokio::test(flavor = "multi_thread")]
async fn ex_compare_010_status_and_diff_report_fifos_on_both_sides_and_exit_with_2_on_all_three_routes(
) {
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides.fixture.put("a.txt", Some("same\n"), Some("same\n"));
        for root in [sides.tested_root(), sides.other_root()] {
            make_fifo(&root.join("pipe"));
        }
        for command in [&["status"][..], &["diff", "."], &["diff", "pipe"]] {
            let output = sides.run_into_tested(command);
            assert_eq!(
                output.status.code(),
                Some(2),
                "{route:?} {command:?}: {output:?}"
            );
            assert!(
                reported(&output, "errors", "pipe"),
                "{route:?} {command:?}: {output:?}"
            );
        }
    }
}

// @kotowari[REQ-compare-009, EX-compare-011]
#[tokio::test(flavor = "multi_thread")]
async fn ex_compare_011_a_socket_only_on_the_destination_is_not_deleted_by_sync_on_all_three_routes(
) {
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides.fixture.put("a.txt", Some("same\n"), Some("same\n"));
        let socket = sides.tested_root().join("app.sock");
        make_socket(&socket);
        let output = sides.run_into_tested(&["sync", ".", "--delete", "--force"]);
        assert_eq!(output.status.code(), Some(2), "{route:?}: {output:?}");
        assert!(socket.symlink_metadata().is_ok(), "{route:?}: {output:?}");
        assert!(target_failed(&output, "app.sock"), "{route:?}: {output:?}");
    }
}

/// sync の書き込み先ごとの failed に、`path` のパスで理由に "not a regular file" を含む項目があるか
fn target_failed(output: &Output, path: &str) -> bool {
    json(output)["targets"].as_array().is_some_and(|targets| {
        targets.iter().any(|target| {
            target["failed"].as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item["path"] == path && item.to_string().contains(NOT_REGULAR))
            })
        })
    })
}

// @kotowari[REQ-compare-009]
#[tokio::test(flavor = "multi_thread")]
async fn merge_neither_replaces_a_file_from_a_fifo_nor_deletes_nor_replaces_a_fifo_on_all_three_routes(
) {
    for route in ROUTES {
        // 読み込み元（試す側）の FIFO と、書き込み先の同じパスの通常ファイル
        let sides = Sides::new(route).await;
        sides.fixture.put("d/a.txt", Some("same\n"), Some("same\n"));
        make_fifo(&sides.tested_root().join("d/x"));
        fs::write(sides.other_root().join("d/x"), "kept\n").unwrap();
        let output = sides.run_from_tested(&["merge", "d/", "--delete", "--force"]);
        assert_eq!(output.status.code(), Some(2), "{route:?}: {output:?}");
        assert_eq!(
            fs::read_to_string(sides.other_root().join("d/x")).unwrap(),
            "kept\n",
            "{route:?}: {output:?}"
        );
        assert!(reported(&output, "failed", "d/x"), "{route:?}: {output:?}");

        // 書き込み先（試す側）の FIFO。読み込み元に同じパスの通常ファイルがあるものと、ないもの
        let sides = Sides::new(route).await;
        sides.fixture.put("d/a.txt", Some("same\n"), Some("same\n"));
        fs::write(sides.other_root().join("d/x"), "new\n").unwrap();
        for name in ["d/x", "d/only"] {
            make_fifo(&sides.tested_root().join(name));
        }
        let output = sides.run_into_tested(&["merge", "d/", "--delete", "--force"]);
        assert_eq!(output.status.code(), Some(2), "{route:?}: {output:?}");
        for name in ["d/x", "d/only"] {
            let kind = sides.tested_root().join(name).symlink_metadata().unwrap();
            assert!(
                !kind.is_file() && !kind.is_dir(),
                "{route:?} {name}: {output:?}"
            );
            assert!(
                reported(&output, "failed", name),
                "{route:?} {name}: {output:?}"
            );
        }
    }
}
