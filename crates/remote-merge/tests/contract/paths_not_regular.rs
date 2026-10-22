#![cfg(unix)]
//! 書き手のいない FIFO の中身とハッシュを、二つの経路とも開いて待たずに "not a regular file" の
//! I/O の失敗にすること（docs/ir/paths/operations.md の REQ-paths-021）の契約テスト。

use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;

use remote_merge_core::op_failure::{OpFailure, SizeLimit};
use remote_merge_core::relative_path::EntryPath;

use super::flow_routes::CHILD_LIMIT;
use super::flow_support::{Child, FlowRoutes, ROUTES, child};
use super::scan_other_kinds::make_fifo;

const THIS_TEST: &str =
    "paths_not_regular::ex_paths_046_a_fifo_is_not_read_while_waiting_for_a_writer_on_both_routes";

fn kind<T>(results: anyhow::Result<Vec<(String, Result<T, OpFailure>)>>) -> String {
    match results {
        Ok(results) => results
            .into_iter()
            .map(|(path, result)| match result {
                Ok(_) => format!("{path}: ok\n"),
                Err(failure) => format!("{path}: {failure:?}\n"),
            })
            .collect(),
        Err(error) => format!("error: {error:#}\n"),
    }
}

/// 子のプロセスで、右の側の "pipe" の中身とハッシュを読んだ結果を答える
fn answer(mut child: Child) {
    assert_eq!(child.request, "read pipe");
    let right = child.right();
    let pipe = [EntryPath::parse("pipe").unwrap()];
    let read = child
        .runtime
        .read_contents(&right, &pipe, SizeLimit::Unlimited);
    let hash = child.runtime.hash_contents(&right, &pipe);
    let text = format!("read {}hash {}", kind(read), kind(hash));
    child.answer(&text);
}

/// FIFO を開いて待っている読み手がいれば、書き手を一瞬開いて閉じ、終わらせる
fn release(fifo: &Path) {
    // 読み手がいなければ開けずに終わる（ENXIO）。それは待っている読み手がいないこと
    if let Ok(writer) = fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(fifo)
    {
        drop(writer);
    }
}

// @kotowari[REQ-paths-021, EX-paths-046]
#[test]
fn ex_paths_046_a_fifo_is_not_read_while_waiting_for_a_writer_on_both_routes() {
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|_, right| make_fifo(&right.join("pipe")));

    for route in ROUTES {
        let fifo = routes.right(route).join("pipe");
        let answer = routes.run_within(route, THIS_TEST, "read pipe", CHILD_LIMIT, || {
            // 止まった読み手を何度か終わらせる（中身とハッシュの二つ）
            for _ in 0..4 {
                release(&fifo);
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let Some(answer) = answer else {
            panic!("{route:?}: reading the FIFO waited for a writer");
        };
        assert_eq!(
            answer,
            "read pipe: Io(\"not a regular file\")\nhash pipe: Io(\"not a regular file\")\n",
            "{route:?}"
        );
    }
}
