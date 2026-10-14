#![cfg(unix)]
//! 書き込み先の root_dir がないとき、ファイル一つの merge が走査の時点でエラーで止まり、何も
//! 書かないこと（docs/ir/scan/listing.md の REQ-scan-015）の契約テスト。

use std::fs;

use super::agent_support::{AgentFixture, Setup};
use super::server_faults::Faults;

/// 書き込み先の root_dir を消して、`path` を merge した出力
async fn merge_into_a_missing_root_dir(
    setup: Setup,
    path: &str,
) -> (AgentFixture, std::process::Output) {
    let fixture = AgentFixture::new(setup, Faults::default()).await;
    fixture.put(path, Some("new\n"), None);
    fs::remove_dir_all(fixture.remote()).unwrap();
    let output = fixture.run(&["merge", path]);
    (fixture, output)
}

fn assert_stopped_without_writing(fixture: &AgentFixture, output: &std::process::Output) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Remote root_dir not found: fixture:"),
        "{output:?}"
    );
    assert!(!fixture.remote().exists(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Ok(result) = serde_json::from_str::<serde_json::Value>(&stdout) {
        assert_eq!(
            result["merged"].as_array().map_or(0, Vec::len),
            0,
            "{result}"
        );
    }
}

// @kotowari[REQ-scan-015]
#[tokio::test(flavor = "multi_thread")]
async fn a_file_under_a_directory_is_not_merged_into_a_missing_root_dir_over_the_agent() {
    for path in ["sub/f.txt", "./sub/f.txt"] {
        let (fixture, output) = merge_into_a_missing_root_dir(Setup::default(), path).await;
        assert_stopped_without_writing(&fixture, &output);
    }
}

// @kotowari[REQ-scan-015]
#[tokio::test(flavor = "multi_thread")]
async fn a_file_under_a_directory_is_not_merged_into_a_missing_local_root_dir() {
    for path in ["sub/f.txt", "./sub/f.txt"] {
        let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
        fixture.put(path, None, Some("new\n"));
        fs::remove_dir_all(fixture.local()).unwrap();
        let output =
            fixture.run_with_sides(&["merge", path], &["--left", "fixture", "--right", "local"]);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(!fixture.local().exists(), "{output:?}");
    }
}
