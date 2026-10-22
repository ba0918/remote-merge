#![cfg(unix)]
//! 読み取りの `接続断` を同じ経路で一度だけやり直し、変更の `接続断` を繰り返さないこと
//! （docs/ir/ssh/reconnect.md・docs/ir/ssh/sudo.md）の契約テスト。

use super::agent_support::{AgentFixture, Setup, json};
use super::server_faults::{AgentCut, CutPoint, CutScope, Faults, Repeat};

fn agent_cut(request: &'static str, repeat: Repeat) -> Faults {
    Faults {
        agent_cut: Some(AgentCut {
            request: Some(request),
            occurrence: 1,
            point: CutPoint::BeforeResponse,
            scope: CutScope::Agent,
            repeat,
        }),
        ..Faults::default()
    }
}

/// remote の `paths` の更新時刻を local と違う時刻にする。サイズが同じファイルを、更新時刻で
/// 同じと決めずに中身で比べさせる（docs/ir/cli/status.md の TBL-cli-001）
fn with_remote_times_apart(fixture: &AgentFixture, paths: &[&str]) {
    let apart = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    for path in paths {
        std::fs::File::options()
            .write(true)
            .open(fixture.remote().join(path))
            .unwrap()
            .set_modified(apart)
            .unwrap();
    }
}

// @kotowari[EX-ssh-019]
#[tokio::test(flavor = "multi_thread")]
async fn a_scan_cut_once_is_scanned_again_after_restarting_the_agent_and_compared() {
    let fixture = AgentFixture::new(Setup::default(), agent_cut("ListTree", Repeat::Once)).await;
    fixture.put("example.txt", Some("same\n"), Some("same\n"));
    fixture.put("only-remote.txt", None, Some("remote\n"));

    let output = fixture.run(&["status"]);

    // 違いがあるときの終了コード 1 で、やり直した走査の一覧で比べている
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        json(&output).to_string().contains("only-remote.txt"),
        "{output:?}"
    );
    assert_eq!(fixture.server.faults().agent_starts(), 2);
    let scans = fixture
        .server
        .faults()
        .agent_requests()
        .into_iter()
        .filter(|request| request == "ListTree")
        .count();
    assert_eq!(scans, 2);
}

// @kotowari[REQ-ssh-003, EX-ssh-006]
#[tokio::test(flavor = "multi_thread")]
async fn a_scan_cut_again_after_the_retry_stops_the_command_with_exit_code_2() {
    let fixture = AgentFixture::new(Setup::default(), agent_cut("ListTree", Repeat::Every)).await;
    fixture.put("example.txt", Some("local\n"), Some("remote\n"));

    let output = fixture.run(&["diff", "example.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    // 比べる流れは名前で指定したパスも走査で集めるので、止めた誤りは走査の失敗を示す
    let result = json(&output);
    assert!(result["error"].as_str().is_some(), "{result}");
    assert!(!result.to_string().contains("\"hunks\""), "{result}");
    // 一度だけやり直し、三度目は尋ねない
    assert_eq!(fixture.server.faults().agent_starts(), 2);
    let scans = fixture
        .server
        .faults()
        .agent_requests()
        .into_iter()
        .filter(|request| request == "ListTree")
        .count();
    assert_eq!(scans, 2);
}

// @kotowari[REQ-ssh-003, EX-ssh-005]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_read_cut_once_restarts_the_agent_and_reads_again() {
    // status はエージェントのハッシュで中身を比べる
    let fixture =
        AgentFixture::new(Setup::default(), agent_cut("HashContents", Repeat::Once)).await;
    fixture.put("example.txt", Some("local\n"), Some("remot\n"));
    fixture.put("same.txt", Some("same\n"), Some("same\n"));
    with_remote_times_apart(&fixture, &["example.txt", "same.txt"]);

    let output = fixture.run(&["status"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let files = json(&output)["files"].to_string();
    assert!(
        files.contains("example.txt") && files.contains("modified"),
        "{files}"
    );
    assert!(!files.contains("same.txt"), "{files}");
    assert_eq!(fixture.server.faults().agent_starts(), 2);
    let hashes = fixture
        .server
        .faults()
        .agent_requests()
        .into_iter()
        .filter(|request| request == "HashContents")
        .count();
    assert_eq!(hashes, 2);
}

// @kotowari[REQ-ssh-004, EX-ssh-007]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_write_without_a_reply_is_not_written_again() {
    let fixture = AgentFixture::new(Setup::default(), agent_cut("WriteFile", Repeat::Once)).await;
    fixture.put("example.txt", Some("local updated\n"), Some("old remote\n"));

    let output = fixture.run(&["merge", "example.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let writes = fixture
        .server
        .faults()
        .agent_requests()
        .into_iter()
        .filter(|request| request == "WriteFile")
        .count();
    assert_eq!(writes, 1);
}

// @kotowari[REQ-ssh-009]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_read_cut_every_time_on_a_sudo_server_stops_without_an_unprivileged_read() {
    let mut faults = agent_cut("ReadFiles", Repeat::Every);
    faults.fake_sudo = true;
    let fixture = AgentFixture::new(
        Setup {
            sudo: true,
            ..Setup::default()
        },
        faults,
    )
    .await;
    fixture.put("example.txt", Some("local\n"), Some("remote\n"));

    let output = fixture.run(&["diff", "example.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    // remote の root_dir に触れたコマンド（エージェントの起動）はどれも sudo を通した
    let on_remote = fixture.remote_commands();
    assert!(!on_remote.is_empty(), "{output:?}");
    assert!(
        on_remote.iter().all(|command| command.starts_with("sudo ")),
        "{on_remote:#?}"
    );
}

// @kotowari[REQ-ssh-003, REQ-ssh-009]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_read_whose_ssh_session_is_cut_once_on_a_sudo_server_reconnects_and_reads_again() {
    // SSH の session ごと切れると、起動し直しの前の sudo の確かめも切れた session で失敗する。
    // その失敗は sudo の設定の誤りではないので、再接続してから起動し直す
    let mut faults = agent_cut("HashContents", Repeat::Once);
    if let Some(cut) = faults.agent_cut.as_mut() {
        cut.scope = CutScope::Session;
    }
    faults.fake_sudo = true;
    let fixture = AgentFixture::new(
        Setup {
            sudo: true,
            ..Setup::default()
        },
        faults,
    )
    .await;
    fixture.put("example.txt", Some("local\n"), Some("remot\n"));
    fixture.put("same.txt", Some("same\n"), Some("same\n"));
    with_remote_times_apart(&fixture, &["example.txt", "same.txt"]);

    let output = fixture.run(&["status"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let files = json(&output)["files"].to_string();
    assert!(
        files.contains("example.txt") && files.contains("modified"),
        "{files}"
    );
    assert!(!files.contains("same.txt"), "{files}");
    assert_eq!(fixture.server.faults().agent_starts(), 2);
}
