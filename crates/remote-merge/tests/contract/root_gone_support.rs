//! root_dir ごと消えたときの契約テスト（merge_root_gone・sync_root_gone）の道具

use std::fs;

use serde_json::Value;

pub use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::Faults;
pub use super::server_faults::SshCommand;
use super::server_removal::Removal;
pub use super::server_removal::When;

pub const UNTOUCHED: &str = "not attempted";

pub fn routes() -> [Setup; 2] {
    [
        Setup::default(),
        Setup {
            agent_disabled: true,
            ..Setup::default()
        },
    ]
}

/// local と remote に置いた fixture。remote の root_dir は必ず作る
pub async fn fixture_with(
    setup: Setup,
    files: &[(&str, Option<&str>, Option<&str>)],
) -> AgentFixture {
    let fixture = AgentFixture::new(setup, Faults::default()).await;
    fs::create_dir_all(fixture.remote()).unwrap();
    fs::create_dir_all(fixture.local()).unwrap();
    for (name, local, remote) in files {
        fixture.put(name, *local, *remote);
    }
    fixture
}

/// `request`（SSH では `command`）の `occurrence` 回目の `when` に remote の root_dir を消す
pub fn remove_root_at(
    fixture: &AgentFixture,
    request: &'static str,
    command: SshCommand,
    occurrence: usize,
    when: When,
) {
    fixture.server.faults().removal.arm(Removal {
        agent_request: request,
        ssh_command: command,
        occurrence,
        when,
        dir: fixture.remote(),
    });
}

/// `failed` の中の `path` の error
pub fn failure<'a>(failed: &'a Value, path: &str) -> &'a str {
    failed
        .as_array()
        .unwrap_or_else(|| panic!("failed missing: {failed}"))
        .iter()
        .find(|entry| entry["path"] == path)
        .unwrap_or_else(|| panic!("{path} is not in failed: {failed}"))["error"]
        .as_str()
        .unwrap()
}

pub fn paths(entries: &Value) -> Vec<&str> {
    entries
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| entry["path"].as_str().unwrap())
                .collect()
        })
        .unwrap_or_default()
}

/// `gone` で root_dir がない失敗になり、`untouched` は行わず、root_dir がない文言は一度だけ出て、
/// root_dir は作り直さない
pub fn assert_stopped(
    fixture: &AgentFixture,
    output: &std::process::Output,
    failed: &Value,
    gone: &str,
    untouched: &str,
) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let message = format!("root_dir is gone: fixture:{}", fixture.remote().display());
    assert!(failure(failed, gone).contains(&message), "{failed}");
    let rest = failure(failed, untouched);
    assert!(
        rest.contains(UNTOUCHED) && !rest.contains(&message),
        "{rest}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.matches(&message).count(), 1, "{stdout}");
    assert!(!fixture.remote().exists(), "{output:?}");
}

pub const NEW: [(&str, Option<&str>, Option<&str>); 3] = [
    ("a.txt", Some("local\n"), None),
    ("b.txt", Some("local\n"), None),
    ("c.txt", Some("local\n"), None),
];

pub const EXISTING: [(&str, Option<&str>, Option<&str>); 3] = [
    ("a.txt", Some("local\n"), Some("remote old\n")),
    ("b.txt", Some("local\n"), Some("remote old\n")),
    ("c.txt", Some("local\n"), Some("remote old\n")),
];
