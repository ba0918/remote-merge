#![cfg(unix)]
//! diff が名前で指定したディレクトリの配下を、全体の走査と同じ走査で展開すること
//! （docs/ir/scan/listing.md の REQ-scan-020、docs/ir/cli/directory-paths.md の REQ-cli-096 と
//! EX-cli-097）を、比べる片側をローカルとエージェントの二つの経路にして確かめる契約テスト。

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;

use serde_json::Value;

use super::common::stdout_json;
use super::scan_missing_root::{ROUTES, Sides};

fn paths_in(json: &Value, key: &str) -> Vec<String> {
    json[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["path"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

// @kotowari[REQ-scan-020, REQ-scan-012]
#[tokio::test(flavor = "multi_thread")]
async fn a_name_that_is_not_utf8_below_a_named_directory_is_reported_as_unreadable() {
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides
            .fixture
            .put("dir/ok.txt", Some("left\n"), Some("right\n"));
        fs::write(
            sides
                .tested_root()
                .join("dir")
                .join(OsStr::from_bytes(b"bad\xff.txt")),
            "bad\n",
        )
        .unwrap();

        let output = sides.run_into_tested(&["diff", "dir"]);

        let result = stdout_json(&output);
        let files = paths_in(&result, "files");
        assert_eq!(files, ["dir/ok.txt"], "{route:?}: {result}");
        assert_eq!(
            paths_in(&result, "errors"),
            ["dir/bad\\xff.txt"],
            "{route:?}: {result}"
        );
        assert_eq!(output.status.code(), Some(2), "{route:?}: {output:?}");
    }
}

// @kotowari[REQ-cli-096, EX-cli-097]
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_directory_below_a_named_directory_reports_the_whole_name() {
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides
            .fixture
            .put("dir/a.txt", Some("left\n"), Some("right\n"));
        sides.fixture.put("b.txt", Some("left\n"), Some("right\n"));
        let secret = sides.tested_root().join("dir/secret");
        fs::create_dir_all(&secret).unwrap();
        fs::write(secret.join("hidden.txt"), "x\n").unwrap();
        let locks = remote_merge_core::test_tree::PermissionLocks::new();
        locks.lock(&secret, 0o000);
        if fs::read_dir(&secret).is_ok() {
            eprintln!("note: running as root; unreadable directories cannot be made");
            return;
        }

        let output = sides.run_into_tested(&["diff", "dir", "b.txt"]);
        drop(locks);

        let result = stdout_json(&output);
        assert_eq!(paths_in(&result, "files"), ["b.txt"], "{route:?}: {result}");
        let errors = result["errors"].as_array().cloned().unwrap_or_default();
        assert_eq!(errors.len(), 1, "{route:?}: {result}");
        assert_eq!(errors[0]["path"], "dir", "{route:?}: {result}");
        assert!(
            errors[0]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("dir/secret")),
            "{route:?}: {result}"
        );
        assert_eq!(output.status.code(), Some(2), "{route:?}: {output:?}");
    }
}

// @kotowari[REQ-cli-096, REQ-scan-018, EX-cli-098]
#[tokio::test(flavor = "multi_thread")]
async fn a_link_that_cannot_be_followed_below_a_named_directory_is_reported_alone() {
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides
            .fixture
            .put("dir/a.txt", Some("left\n"), Some("right\n"));
        sides.fixture.put("b.txt", Some("left\n"), Some("right\n"));
        std::os::unix::fs::symlink("loop", sides.tested_root().join("dir/loop")).unwrap();

        let output = sides.run_into_tested(&["diff", "dir", "b.txt"]);

        let result = stdout_json(&output);
        assert_eq!(
            paths_in(&result, "files"),
            ["b.txt", "dir/a.txt"],
            "{route:?}: {result}"
        );
        let errors = result["errors"].as_array().cloned().unwrap_or_default();
        assert_eq!(errors.len(), 1, "{route:?}: {result}");
        assert_eq!(errors[0]["path"], "dir/loop", "{route:?}: {result}");
        assert!(
            errors[0]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("cannot follow")),
            "{route:?}: {result}"
        );
        assert_eq!(output.status.code(), Some(2), "{route:?}: {output:?}");
    }
}
