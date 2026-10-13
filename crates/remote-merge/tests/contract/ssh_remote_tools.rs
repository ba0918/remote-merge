#![cfg(unix)]
//! エージェントを使わないときのリモートの道具の有無の扱い（docs/ir/ssh/compatibility.md の
//! TBL-ssh-002）と、まとめた問いの接続断（docs/ir/ssh/reconnect.md の REQ-ssh-003）の契約テスト。
//!
//! SSH の経路が使う問いのスクリプトを、道具を絞った PATH の sh で動かして確かめる。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use remote_merge_core::op_failure::{OpFailure, Tool};
use remote_merge_core::relative_path::EntryPath;
use remote_merge_ssh::contents::{hash_all, read_all};

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::{Faults, Repeat, SshCommand, SshCut};

/// `tools` だけを置いた PATH。どれも /usr/bin か /bin にあるものを起動する。`fake` は
/// （名前, sh の本文）で置く偽の道具
fn bin_with(tools: &[&str], fake: &[(&str, &str)]) -> tempfile::TempDir {
    let bin = tempfile::tempdir().unwrap();
    let place = |name: &str, body: String| {
        let path = bin.path().join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    };
    for tool in tools {
        place(
            tool,
            format!(
                "for d in /usr/bin /bin; do [ -x \"$d/{tool}\" ] && exec \"$d/{tool}\" \"$@\"; done\nexit 127\n"
            ),
        );
    }
    for (name, body) in fake {
        place(name, body.to_string());
    }
    bin
}

/// `bin` だけを PATH にした sh でスクリプトを動かす
fn run_in(bin: &Path) -> impl FnMut(&str) -> Result<Vec<u8>, String> + '_ {
    move |script: &str| {
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .env("PATH", bin)
            .output()
            .map_err(|error| error.to_string())?;
        Ok(output.stdout)
    }
}

/// [`run_in`] の、`input` を標準入力に送るもの（中身の読み込みは区切りを標準入力で受ける）
fn fed_in(bin: &Path) -> impl FnMut(&str, &[u8]) -> Result<Vec<u8>, String> + '_ {
    move |script: &str, input: &[u8]| {
        use std::io::Write as _;
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .env("PATH", bin)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        let mut stdin = child.stdin.take().expect("stdin is piped");
        stdin.write_all(input).map_err(|error| error.to_string())?;
        drop(stdin);
        let output = child
            .wait_with_output()
            .map_err(|error| error.to_string())?;
        Ok(output.stdout)
    }
}

fn asked(root: &Path, rel: &str) -> Vec<(String, String)> {
    vec![(rel.to_string(), format!("{}/{rel}", root.display()))]
}

// @kotowari[REQ-ssh-025, EX-ssh-058]
#[test]
fn ex_ssh_058_shasum_gives_the_same_hash_as_the_local_route() {
    if !Path::new("/usr/bin/shasum").exists() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.txt"), "content\n").unwrap();
    let only_shasum = bin_with(&["cat", "shasum"], &[]);

    let hashes = hash_all(
        &asked(root.path(), "a.txt"),
        65536,
        run_in(only_shasum.path()),
    )
    .unwrap();

    let local =
        remote_merge_core::machine_ops::hash(root.path(), &EntryPath::parse("a.txt").unwrap());
    assert_eq!(hashes[0].1, Ok(Some(local.unwrap())));
}

// @kotowari[REQ-ssh-025]
#[test]
fn without_sha256sum_and_shasum_the_hash_is_left_to_the_content() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.txt"), "content\n").unwrap();
    let none = bin_with(&["cat"], &[]);

    let hashes = hash_all(&asked(root.path(), "a.txt"), 65536, run_in(none.path())).unwrap();

    assert_eq!(hashes[0].1, Ok(None));
}

// @kotowari[REQ-ssh-025, REQ-ssh-026]
#[test]
fn the_size_comes_from_ls_when_stat_refuses_c_and_is_a_missing_tool_without_both() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.txt"), "content\n").unwrap();
    let refusing_stat = ("stat", "echo \"stat: invalid option -- 'c'\" >&2\nexit 1\n");
    let with_ls = bin_with(&["cat", "ls", "awk"], &[refusing_stat]);
    let without_ls = bin_with(&["cat"], &[refusing_stat]);

    let by_ls = read_all(
        &asked(root.path(), "a.txt"),
        Some(100),
        "0123456789abcdef0123456789abcdef",
        65536,
        fed_in(with_ls.path()),
    )
    .unwrap();
    let neither = read_all(
        &asked(root.path(), "a.txt"),
        Some(100),
        "0123456789abcdef0123456789abcdef",
        65536,
        fed_in(without_ls.path()),
    )
    .unwrap();

    assert_eq!(by_ls[0].1, Ok(b"content\n".to_vec()));
    assert_eq!(neither[0].1, Err(OpFailure::ToolMissing(Tool::Stat)));
    let message = remote_merge_core::op_failure::OpError::new(
        remote_merge_core::op_failure::Operation::Read,
        "a.txt",
        neither[0].1.clone().unwrap_err(),
    )
    .to_string();
    assert!(message.contains("stat"), "{message}");
}

// 書き込み先の中身をまとめて読む問いが、やり直しても接続断なら、まとめに入っていたパスを
// すべて同じ理由の読み込みの失敗にし、どれも書かない
// @kotowari[REQ-ssh-003]
#[tokio::test(flavor = "multi_thread")]
async fn a_batch_read_lost_twice_fails_every_path_in_it_with_the_same_reason() {
    let fixture = AgentFixture::new(
        Setup {
            agent_disabled: true,
            ..Setup::default()
        },
        Faults {
            ssh_cut: Some(SshCut {
                command: SshCommand::Read,
                repeat: Repeat::Every,
            }),
            ..Faults::default()
        },
    )
    .await;
    fixture.put("a.txt", Some("left a\n"), Some("right a\n"));
    fixture.put("b.txt", Some("left b\n"), Some("right b\n"));

    let output = fixture.run(&["sync", "a.txt", "b.txt", "--force"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let failed: Vec<(String, String)> = parsed["targets"][0]["failed"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|entry| {
            (
                entry["path"].as_str().unwrap().to_string(),
                entry["error"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let paths: Vec<&str> = failed.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(paths, ["a.txt", "b.txt"], "{result}");
    assert_eq!(failed[0].1, failed[1].1, "{result}");
    assert_eq!(fixture.read_remote("a.txt"), "right a\n");
    assert_eq!(fixture.server.faults().ssh_cuts(), 2);
}
