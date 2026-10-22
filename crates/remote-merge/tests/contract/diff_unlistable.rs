#![cfg(unix)]
//! diff が、子の一覧を取れなかったディレクトリをファイルとしてもない側としても比べずに errors に出すこと
//! （docs/ir/cli/directory-paths.md の REQ-cli-096）の契約テスト。

use std::fs;

use remote_merge_core::test_tree::PermissionLocks;

use super::agent_support::{AgentFixture, Setup};
use super::common::stdout_json;
use super::server_faults::Faults;

/// 読めないようにしたディレクトリ。テストが途中で失敗しても、一時ディレクトリを消す前に戻す
fn unreadable(path: &std::path::Path) -> PermissionLocks {
    fs::create_dir_all(path).unwrap();
    let locks = PermissionLocks::new();
    locks.lock(path, 0o000);
    locks
}

/// リモートの経路（エージェント）
fn routes() -> [(&'static str, Setup); 1] {
    [("agent", Setup::default())]
}

// @kotowari[REQ-cli-096, EX-cli-095]
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_directory_is_reported_in_errors_and_not_compared() {
    if !super::common::permissions_apply() {
        return;
    }
    for (route, setup) in routes() {
        let fixture = AgentFixture::new(setup, Faults::default()).await;
        fixture.put("secret/a.txt", Some("local\n"), None);
        let _secret = unreadable(&fixture.remote().join("secret"));

        let output = fixture.run(&["diff", "secret"]);

        assert_eq!(output.status.code(), Some(2), "{route}: {output:?}");
        let result = stdout_json(&output);
        let files = result["files"]
            .as_array()
            .unwrap_or_else(|| panic!("{route}: {result}"));
        assert!(files.is_empty(), "{route}: {result}");
        let errors = result["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 1, "{route}: {result}");
        assert_eq!(errors[0]["path"], "secret", "{route}: {result}");
        assert!(
            !errors[0]["reason"].as_str().unwrap().is_empty(),
            "{route}: {result}"
        );
    }
}

// @kotowari[EX-cli-096]
#[tokio::test(flavor = "multi_thread")]
async fn a_regular_file_is_compared_as_a_file_without_errors() {
    for (route, setup) in routes() {
        let fixture = AgentFixture::new(setup, Faults::default()).await;
        fixture.put("a.txt", Some("local\n"), Some("remote\n"));

        let output = fixture.run(&["diff", "a.txt"]);

        assert_eq!(output.status.code(), Some(1), "{route}: {output:?}");
        let result = stdout_json(&output);
        assert_eq!(result["files"][0]["path"], "a.txt", "{route}: {result}");
        assert!(
            !result["files"][0]["hunks"].as_array().unwrap().is_empty(),
            "{route}: {result}"
        );
        let errors = result["errors"].as_array().map_or(0, Vec::len);
        assert_eq!(errors, 0, "{route}: {result}");
    }
}
