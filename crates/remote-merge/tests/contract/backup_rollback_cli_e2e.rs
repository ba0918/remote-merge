#![cfg(unix)]
//! rollback コマンドの振る舞い（docs/ir/backup/rollback-cli.md）の契約テストのうち、
//! 隔離された SSH fixture に対して実行ファイルを起動して確かめるもの。

use std::fs;
use std::io::Write;
use std::process::{Output, Stdio};

use super::common::{assert_exit_error, assert_exit_success, CliEnv};

fn merge(env: &CliEnv, paths: &[&str]) {
    let output = env
        .cmd_with("merge")
        .args(paths)
        .args(["--left", "local", "--right", "develop", "--force"])
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&output);
}

fn newest_session_id(env: &CliEnv) -> String {
    let output = env
        .cmd_with("rollback")
        .args(["--list", "--format", "json", "--target", "develop"])
        .output()
        .expect("failed to execute rollback --list");
    assert_exit_success(&output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    json["sessions"][0]["session_id"]
        .as_str()
        .expect("no session listed")
        .to_owned()
}

/// --force も --dry-run もなしに rollback を起動し、確認に answer を答える。
fn rollback_answering(env: &CliEnv, answer: &str) -> Output {
    rollback_answering_with(env, answer, &[])
}

/// `rollback_answering` に `extra` の引数を足して起動する
fn rollback_answering_with(env: &CliEnv, answer: &str, extra: &[&str]) -> Output {
    let mut child = env
        .cmd_with("rollback")
        .args(["--target", "develop"])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rollback");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(answer.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

// @kotowari[REQ-backup-035]
#[test]
fn rollback_asks_before_restoring_and_restores_only_after_y_or_yes() {
    for answer in ["n\n", "\n", "y\n"] {
        let env = CliEnv::new(&[("file.txt", "merged\n")], &[("file.txt", "original\n")]);
        merge(&env, &["file.txt"]);
        let session_id = newest_session_id(&env);

        let output = rollback_answering(&env, answer);

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!(
                "Restore 1 file(s) from session {session_id} to develop? [y/N]"
            )),
            "{answer:?}: {stderr}"
        );
        assert_exit_success(&output);
        assert_eq!(
            stderr.contains("Aborted."),
            answer != "y\n",
            "{answer:?}: {stderr}"
        );
        let expected = if answer == "y\n" {
            "original\n"
        } else {
            "merged\n"
        };
        assert_eq!(
            fs::read_to_string(env.remote_dir.join("file.txt")).unwrap(),
            expected,
            "{answer:?}"
        );
    }
}

// @kotowari[REQ-backup-035]
#[test]
fn rollback_restores_after_yes() {
    let env = CliEnv::new(&[("file.txt", "merged\n")], &[("file.txt", "original\n")]);
    merge(&env, &["file.txt"]);

    let output = rollback_answering(&env, "yes\n");

    assert_exit_success(&output);
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("file.txt")).unwrap(),
        "original\n"
    );
}

// @kotowari[REQ-backup-034]
#[test]
fn rollback_restores_every_file_of_the_newest_session() {
    let env = CliEnv::new(
        &[("a.txt", "a-local\n"), ("b.txt", "b-local\n")],
        &[("a.txt", "a-original\n"), ("b.txt", "b-original\n")],
    );
    merge(&env, &["a.txt", "b.txt"]);

    let output = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--force"])
        .output()
        .expect("failed to execute rollback");

    assert_exit_success(&output);
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("a.txt")).unwrap(),
        "a-original\n"
    );
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("b.txt")).unwrap(),
        "b-original\n"
    );
}

// @kotowari[REQ-backup-034]
#[test]
fn rollback_without_force_restores_a_dotenv_file_after_yes() {
    let env = CliEnv::new(
        &[("file.txt", "merged\n"), (".env", "A=merged\n")],
        &[("file.txt", "original\n"), (".env", "A=original\n")],
    );
    merge(&env, &["file.txt", ".env"]);
    let session_id = newest_session_id(&env);

    let output = rollback_answering_with(&env, "y\n", &["--format", "json"]);

    assert_exit_success(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!(
            "Restore 2 file(s) from session {session_id} to develop? [y/N]"
        )),
        "{stderr}"
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut restored: Vec<&str> = json["restored"]
        .as_array()
        .unwrap_or_else(|| panic!("restored missing: {json}"))
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    restored.sort_unstable();
    assert_eq!(restored, [".env", "file.txt"], "{json}");
    assert_eq!(
        fs::read_to_string(env.remote_dir.join(".env")).unwrap(),
        "A=original\n"
    );
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("file.txt")).unwrap(),
        "original\n"
    );
}

// @kotowari[REQ-backup-034]
#[test]
fn rollback_restores_only_the_selected_older_session() {
    let env = CliEnv::new(
        &[("a.txt", "a-local\n"), ("b.txt", "b-local\n")],
        &[("a.txt", "a-original\n"), ("b.txt", "b-original\n")],
    );
    merge(&env, &["a.txt"]);
    let older = newest_session_id(&env);
    merge(&env, &["b.txt"]);
    assert_ne!(newest_session_id(&env), older);

    let output = env
        .cmd_with("rollback")
        .args(["--session", &older, "--target", "develop", "--force"])
        .output()
        .expect("failed to execute rollback --session");

    assert_exit_success(&output);
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("a.txt")).unwrap(),
        "a-original\n"
    );
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("b.txt")).unwrap(),
        "b-local\n"
    );
}

// @kotowari[REQ-backup-038]
#[test]
fn rollback_exits_with_2_when_the_session_is_not_found() {
    let env = CliEnv::new(&[("file.txt", "local\n")], &[("file.txt", "remote\n")]);

    let without_sessions = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--force"])
        .output()
        .expect("failed to execute rollback");
    merge(&env, &["file.txt"]);
    let unknown_session = env
        .cmd_with("rollback")
        .args([
            "--target",
            "develop",
            "--force",
            "--session",
            "20000101-000000",
        ])
        .output()
        .expect("failed to execute rollback --session");

    assert_exit_error(&without_sessions, 2);
    assert_exit_error(&unknown_session, 2);
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("file.txt")).unwrap(),
        "local\n"
    );
}
