#![cfg(unix)]
//! sync の書き込み先で、書き込み先を変えた後で失敗したファイル（中身は書いたが権限を変えられなかった、
//! symlink を置き換えるために古いリンクを消した後に作れなかった）しかないとき、そのファイルを failed に
//! 出し、書き込み先の status を "partial" にすること（docs/ir/cli/sync.md の TBL-cli-007）の契約テスト

use super::agent_support::{AgentFixture, Setup, json};
use super::server_faults::Faults;

async fn refusing(request: &'static str) -> AgentFixture {
    AgentFixture::new(
        Setup::default(),
        Faults {
            refuse_agent: Some(request),
            ..Faults::default()
        },
    )
    .await
}

fn assert_partial_with_the_failed(output: &std::process::Output, path: &str) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let target = &json(output)["targets"][0];
    assert_eq!(target["status"], "partial", "{target}");
    assert_eq!(target["failed"][0]["path"], path, "{target}");
    assert!(target["merged"].as_array().unwrap().is_empty(), "{target}");
}

// @kotowari[TBL-cli-007, REQ-cli-041]
#[tokio::test(flavor = "multi_thread")]
async fn a_target_whose_only_file_was_written_without_its_permissions_is_partial() {
    let fixture = refusing("Chmod").await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["sync", "a.txt", "--with-permissions", "--force"]);

    assert_partial_with_the_failed(&output, "a.txt");
    assert_eq!(fixture.read_remote("a.txt"), "local\n");
}

// @kotowari[TBL-cli-007, REQ-cli-041]
#[tokio::test(flavor = "multi_thread")]
async fn a_target_whose_only_link_was_removed_but_not_created_is_partial() {
    let fixture = refusing("Symlink").await;
    std::fs::create_dir_all(fixture.local()).unwrap();
    std::fs::create_dir_all(fixture.remote()).unwrap();
    std::os::unix::fs::symlink("new", fixture.local().join("link")).unwrap();
    std::os::unix::fs::symlink("old", fixture.remote().join("link")).unwrap();

    let output = fixture.run(&["sync", "link", "--force"]);

    assert_partial_with_the_failed(&output, "link");
}
