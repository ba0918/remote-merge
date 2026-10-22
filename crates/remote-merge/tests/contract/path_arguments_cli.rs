#![cfg(unix)]
//! パスの引数に絶対パスか ".." の要素があるときに、コマンドが経路によらず引数の段階で止まる契約テスト
//! （docs/ir/cli/diff-output.md の REQ-cli-078、docs/ir/merge/symlink.md の REQ-merge-007）。
//!
//! 実行ファイルを試験 SSH サーバに対して起動し、エージェントを有効にした経路と、書き込み先が SSH の
//! 経路を確かめる。

use std::fs;

use super::cli_launch_support::AgentRootsFixture;

// @kotowari[REQ-cli-078]
#[tokio::test(flavor = "multi_thread")]
async fn an_absolute_path_stops_diff_before_any_command_reaches_the_agent_server() {
    let fixture = AgentRootsFixture::new().await;
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

// @kotowari[REQ-merge-007]
#[test]
fn an_absolute_path_stops_a_merge_into_an_ssh_target_before_writing() {
    let env = super::common::CliEnv::new_3way(
        &[("file.txt", "incoming\n")],
        &[("file.txt", "develop old\n"), ("extra.txt", "extra\n")],
        &[],
    );
    let outside = env.temp_root().join("outside.txt");
    fs::write(&outside, "outside\n").unwrap();
    let outside = outside.to_str().unwrap();

    let output = env
        .cmd_with("merge")
        .args(["file.txt", outside, "--delete", "--force"])
        .args(["--left", "local", "--right", "develop"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains(&format!("absolute subpath not allowed: {outside}")),
        "{output:?}"
    );
    let mut entries: Vec<String> = fs::read_dir(&env.remote_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    assert_eq!(entries, ["extra.txt", "file.txt"]);
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("file.txt")).unwrap(),
        "develop old\n"
    );
}
