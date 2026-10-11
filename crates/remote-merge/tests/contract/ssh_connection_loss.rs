#![cfg(unix)]
//! 読み取りの `接続断` を同じ経路で一度だけやり直し、変更の `接続断` を繰り返さないこと
//! （docs/ir/ssh/reconnect.md・docs/ir/ssh/sudo.md）の契約テスト。

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::{AgentCut, CutPoint, CutScope, Faults, Repeat, SshCommand, SshCut};

/// SSH の偽サーバが受けた、remote の root_dir のファイルの中身を読むか書くコマンド
fn ssh_reads_and_writes(fixture: &AgentFixture) -> Vec<String> {
    fixture
        .remote_commands()
        .into_iter()
        .filter(|command| SshCommand::Read.matches(command) || SshCommand::Write.matches(command))
        .collect()
}

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

// @kotowari[EX-ssh-019]
#[tokio::test(flavor = "multi_thread")]
async fn a_scan_cut_once_is_scanned_again_after_reconnecting_and_compared() {
    let fixture = AgentFixture::new(
        Setup {
            agent_disabled: true,
            ..Setup::default()
        },
        Faults {
            ssh_cut: Some(SshCut {
                command: SshCommand::Scan,
                repeat: Repeat::Once,
            }),
            ..Faults::default()
        },
    )
    .await;
    fixture.put("example.txt", Some("same\n"), Some("same\n"));
    fixture.put("only-remote.txt", None, Some("remote\n"));

    let output = fixture.run(&["status"]);

    // 違いがあるときの終了コード 1 で、やり直した走査の一覧で比べている
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        json(&output).to_string().contains("only-remote.txt"),
        "{output:?}"
    );
    assert_eq!(fixture.server.faults().ssh_cuts(), 1);
    let scans = fixture
        .server
        .commands()
        .iter()
        .filter(|command| SshCommand::Scan.matches(command))
        .count();
    assert_eq!(scans, 2);
}

// @kotowari[REQ-ssh-003]
#[tokio::test(flavor = "multi_thread")]
async fn a_read_cut_again_after_the_retry_stops_the_command_with_exit_code_2() {
    let fixture = AgentFixture::new(
        Setup {
            agent_disabled: true,
            ..Setup::default()
        },
        Faults {
            ssh_cut: Some(SshCut {
                command: SshCommand::ListChildren,
                repeat: Repeat::Every,
            }),
            ..Faults::default()
        },
    )
    .await;
    fixture.put("example.txt", Some("local\n"), Some("remote\n"));

    let output = fixture.run(&["diff", "example.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output).to_string();
    assert!(result.contains("example.txt"), "{result}");
    assert!(!result.contains("\"hunks\""), "{result}");
    assert_eq!(fixture.server.faults().ssh_cuts(), 2);
}

// @kotowari[REQ-ssh-023, EX-ssh-041]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_read_cut_once_restarts_the_agent_and_reads_again_without_ssh() {
    // status はエージェントのハッシュで中身を比べる
    let fixture = AgentFixture::new(Setup::default(), agent_cut("HashFiles", Repeat::Once)).await;
    fixture.put("example.txt", Some("local\n"), Some("remot\n"));
    fixture.put("same.txt", Some("same\n"), Some("same\n"));

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
        .filter(|request| request == "HashFiles")
        .count();
    assert_eq!(hashes, 2);
    let over_ssh = ssh_reads_and_writes(&fixture);
    assert!(over_ssh.is_empty(), "{over_ssh:#?}");
}

// @kotowari[REQ-ssh-004]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_write_without_a_reply_is_not_written_again_over_ssh() {
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
    let over_ssh = ssh_reads_and_writes(&fixture);
    assert!(over_ssh.is_empty(), "{over_ssh:#?}");
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
    let over_ssh = ssh_reads_and_writes(&fixture);
    assert!(over_ssh.is_empty(), "{over_ssh:#?}");
}

// @kotowari[REQ-ssh-023, REQ-ssh-009]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_read_whose_ssh_session_is_cut_once_on_a_sudo_server_reconnects_and_reads_again() {
    // SSH の session ごと切れると、起動し直しの前の sudo の確かめも切れた session で失敗する。
    // その失敗は sudo の設定の誤りではないので、再接続してから起動し直す
    let mut faults = agent_cut("HashFiles", Repeat::Once);
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

    let output = fixture.run(&["status"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let files = json(&output)["files"].to_string();
    assert!(
        files.contains("example.txt") && files.contains("modified"),
        "{files}"
    );
    assert!(!files.contains("same.txt"), "{files}");
    assert_eq!(fixture.server.faults().agent_starts(), 2);
    let over_ssh = ssh_reads_and_writes(&fixture);
    assert!(over_ssh.is_empty(), "{over_ssh:#?}");
}
