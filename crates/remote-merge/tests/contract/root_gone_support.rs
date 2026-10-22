//! root_dir ごと消えたときの契約テスト（merge_root_gone・sync_root_gone）の道具

use std::fs;

use serde_json::Value;

pub use super::agent_support::{AgentFixture, Setup, failure, paths};
use super::server_faults::Faults;
use super::server_removal::Removal;
pub use super::server_removal::When;

pub const UNTOUCHED: &str = "not attempted";

/// root_dir の二つのなくし方
pub const WAYS: [Gone; 2] = [Gone::Removed, Gone::ReplacedByFile];

/// local と remote に置いた fixture。remote の root_dir は必ず作る
pub async fn fixture_with(files: &[(&str, Option<&str>, Option<&str>)]) -> AgentFixture {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fs::create_dir_all(fixture.remote()).unwrap();
    fs::create_dir_all(fixture.local()).unwrap();
    for (name, local, remote) in files {
        fixture.put(name, *local, *remote);
    }
    fixture
}

/// root_dir のなくし方
#[derive(Debug, Clone, Copy)]
pub enum Gone {
    /// 中身ごと消す
    Removed,
    /// 中身ごと消して、同じパスに通常ファイルを置く
    ReplacedByFile,
}

/// エージェントの要求 `request` の `occurrence` 回目の `when` に remote の root_dir を消す
pub fn remove_root_at(
    fixture: &AgentFixture,
    request: &'static str,
    occurrence: usize,
    when: When,
) {
    gone_root_at(fixture, request, occurrence, when, Gone::Removed);
}

/// [`remove_root_at`] の、なくし方 `gone` を選ぶもの
pub fn gone_root_at(
    fixture: &AgentFixture,
    request: &'static str,
    occurrence: usize,
    when: When,
    gone: Gone,
) {
    fixture.server.faults().removal.arm(Removal {
        agent_request: request,
        occurrence,
        when,
        dir: fixture.remote(),
        replace_with_file: matches!(gone, Gone::ReplacedByFile),
    });
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
    assert!(!fixture.remote().is_dir(), "{output:?}");
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
