#![cfg(unix)]
//! パスの引数に絶対パスか ".." の要素があるときに、コマンドが経路によらず引数の段階で止まる契約テスト
//! （docs/ir/cli/diff-output.md の REQ-cli-078、docs/ir/merge/symlink.md の REQ-merge-007）。
//!
//! 実行ファイルを試験 SSH サーバに対して起動し、エージェントを有効にした経路と、書き込み先が SSH の
//! 経路を確かめる。

use std::fs;

use super::scan_listing_cli::AgentFixture;

// @kotowari[REQ-cli-078]
#[tokio::test(flavor = "multi_thread")]
async fn an_absolute_path_stops_diff_before_any_command_reaches_the_agent_server() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("a.txt"), root.to_string_lossy().as_bytes()).unwrap();
    }

    let (output, commands) =
        fixture.diff_with_agent_enabled(&local_root, &remote_root, &["a.txt", "/etc/passwd"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("absolute subpath not allowed: /etc/passwd"),
        "{output:?}"
    );
    assert!(commands.is_empty(), "{commands:?}");
}
