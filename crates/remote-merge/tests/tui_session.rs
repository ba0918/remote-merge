#![cfg(unix)]

#[path = "common/tui_session.rs"]
mod tui_session;

use std::process::Command;
use std::time::{Duration, Instant};
use tui_session::DrainingSession;

#[test]
fn state_wait_observes_completed_json_after_missing_and_incomplete_output() {
    let temp = tempfile::TempDir::new().unwrap();
    let state_path = temp.path().join("state.json");
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "stty -echo; printf READY; read -r key; sleep 0.05; printf '{' > \"$1\"; sleep 0.05; printf '{\"ready\":true}' > \"$1.tmp\"; mv \"$1.tmp\" \"$1\"; printf COMPLETE; read -r key",
        "json-producer",
    ]);
    command.arg(&state_path);
    let mut session = DrainingSession::spawn(command);
    session.expect("READY");
    session.send("produce\n");
    let state = tui_session::wait_for_state(&state_path, "ready", |state| state["ready"] == true);
    assert_eq!(state["ready"], true);
    session.expect("COMPLETE");
    session.send("quit\n");
    session.expect_exit();
}

#[test]
fn output_is_consumed_while_the_test_waits_for_child_progress_and_remains_matchable() {
    let temp = tempfile::TempDir::new().unwrap();
    let progress = temp.path().join("progress");
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "stty -echo; printf READY; read -r key; head -c 1048576 /dev/zero | tr '\\000' x; printf OUTPUT_COMPLETE; touch \"$1\"; read -r key; printf CANCELLED; read -r key",
        "pty-producer",
    ]);
    command.arg(&progress);
    let mut session = DrainingSession::spawn(command);
    session.expect("READY");
    session.send("produce\n");

    let deadline = Instant::now() + Duration::from_secs(5);
    while !progress.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        progress.exists(),
        "child output blocked before recording progress"
    );
    session.expect("OUTPUT_COMPLETE");
    session.send("cancel\n");
    session.expect("CANCELLED");
    session.send("quit\n");
    session.expect_exit();
}
