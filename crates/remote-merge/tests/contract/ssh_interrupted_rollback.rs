#![cfg(unix)]
//! バックアップが無効な rollback の書き戻しの途中の `接続断` で、書き戻す前にまとめて取った
//! ハッシュ（SSH でハッシュのコマンドがなければ中身）を使って、そのファイルを確かめてから止める
//! こと（docs/ir/ssh/reconnect.md の REQ-ssh-022、docs/ir/backup/rollback-path.md の
//! REQ-backup-028）の契約テスト。
//!
//! failed の error は `ssh_interrupted_changes` と同じ語で見分ける。

use serde_json::Value;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::{AgentCut, CutPoint, CutScope, Faults, Repeat, SshCommand, SshCut};
use super::ssh_interrupted_changes::backup_sessions;

const NOT_WRITTEN: &str = "not written";
const UNKNOWN: &str = "result unknown";
const UNTOUCHED: &str = "not attempted";
const READ_FAILED: &str = "read failed";

/// "a.txt" と "b.txt" を merge で書き換えておき、バックアップを無効にした設定にする
async fn merged_without_backup(setup: Setup, faults: Faults) -> AgentFixture {
    let fixture = AgentFixture::new(setup, faults).await;
    fixture.put("a.txt", Some("merged a\n"), Some("original a\n"));
    fixture.put("b.txt", Some("merged b\n"), Some("original b\n"));
    let merge = fixture.run(&["merge", "a.txt", "b.txt"]);
    assert!(merge.status.success(), "{merge:?}");
    fixture.disable_backup();
    fixture
}

fn rollback(fixture: &AgentFixture) -> Value {
    let output = fixture.run(&["rollback", "--target", "fixture", "--force"]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    json(&output)
}

/// 先に書き戻すファイルは書いていない失敗、もう一つは手を付けていない失敗で、どちらも結果が
/// 分からない失敗ではなく、どちらも変わっていない
fn assert_first_not_written_and_second_untouched(fixture: &AgentFixture, result: &Value) {
    let failed = result["failed"].as_array().expect("failed");
    assert_eq!(failed.len(), 2, "{result}");
    let first = failed[0]["error"].as_str().unwrap();
    let second = failed[1]["error"].as_str().unwrap();
    assert!(
        first.contains(NOT_WRITTEN) && !first.contains(UNKNOWN) && !first.contains(UNTOUCHED),
        "{result}"
    );
    assert!(
        second.contains(UNTOUCHED) && !second.contains(UNKNOWN),
        "{result}"
    );
    for name in ["a.txt", "b.txt"] {
        assert_eq!(
            fixture.read_remote(name),
            format!("merged {}\n", &name[..1])
        );
    }
    // 書き戻す前に取ったものは保存しない（merge のバックアップだけが残る）
    assert_eq!(backup_sessions(fixture).len(), 1, "{result}");
}

fn hash_commands(fixture: &AgentFixture) -> usize {
    fixture
        .remote_commands()
        .iter()
        .filter(|command| SshCommand::Hash.matches(command))
        .count()
}

/// 書き戻しの書き込みのコマンドで、一度だけ SSH の session を切る
fn arm_cut_on_write(fixture: &AgentFixture) {
    fixture.server.faults().arm_ssh_cut(SshCut {
        command: SshCommand::Write,
        repeat: Repeat::Once,
    });
}

fn without_agent() -> Setup {
    Setup {
        agent_disabled: true,
        ..Setup::default()
    }
}

// @kotowari[EX-ssh-053]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_rollback_without_backup_cut_before_the_first_restore_reports_not_written() {
    let fixture = merged_without_backup(Setup::default(), Faults::default()).await;
    // 最初の書き戻しは、3 番目のパスの確認の直後
    fixture.server.faults().arm_agent_cut(AgentCut {
        request: Some("InspectPath"),
        occurrence: 3,
        point: CutPoint::AfterResponse,
        scope: CutScope::Agent,
        repeat: Repeat::Once,
    });
    let before = fixture.server.faults().agent_requests().len();

    let result = rollback(&fixture);

    assert_first_not_written_and_second_untouched(&fixture, &result);
    // 切れた通信路に送られたかもしれない書き込みを、つなぎ直した後に繰り返さない
    let requests = &fixture.server.faults().agent_requests()[before..];
    let writes = requests.iter().filter(|seen| *seen == "WriteFile").count();
    assert!(writes <= 1, "{requests:?}");
}

// @kotowari[EX-ssh-053]
#[tokio::test(flavor = "multi_thread")]
async fn an_ssh_rollback_without_backup_cut_on_the_first_restore_reports_not_written() {
    let fixture = merged_without_backup(without_agent(), Faults::default()).await;
    arm_cut_on_write(&fixture);

    let result = rollback(&fixture);

    assert_first_not_written_and_second_untouched(&fixture, &result);
    assert_eq!(fixture.server.faults().ssh_cuts(), 1);
    assert_eq!(hash_commands(&fixture), 1);
}

// @kotowari[EX-ssh-053]
#[tokio::test(flavor = "multi_thread")]
async fn an_ssh_rollback_without_backup_on_a_server_without_hash_commands_reports_not_written() {
    let faults = Faults {
        without_hash_commands: true,
        ..Faults::default()
    };
    let fixture = merged_without_backup(without_agent(), faults).await;
    arm_cut_on_write(&fixture);
    let before = fixture.remote_commands().len();

    let result = rollback(&fixture);

    assert_first_not_written_and_second_untouched(&fixture, &result);
    assert_eq!(fixture.server.faults().ssh_cuts(), 1);
    // 代わりに、最初の書き戻しの前に書き戻す先の中身をまとめて一回だけ読む
    let reads = fixture.remote_commands()[before..]
        .iter()
        .take_while(|command| !SshCommand::Write.matches(command))
        .filter(|command| SshCommand::Read.matches(command))
        .count();
    assert_eq!(reads, 1);
}

// @kotowari[REQ-ssh-022]
#[tokio::test(flavor = "multi_thread")]
async fn only_a_rollback_without_backup_asks_the_agent_for_hashes_once_before_restoring() {
    let with_backup = AgentFixture::new(Setup::default(), Faults::default()).await;
    with_backup.put("a.txt", Some("merged a\n"), Some("original a\n"));
    with_backup.put("b.txt", Some("merged b\n"), Some("original b\n"));
    assert!(with_backup
        .run(&["merge", "a.txt", "b.txt"])
        .status
        .success());
    let without_backup = merged_without_backup(Setup::default(), Faults::default()).await;

    let mut sequences = Vec::new();
    for fixture in [&with_backup, &without_backup] {
        let before = fixture.server.faults().agent_requests().len();
        let starts = fixture.server.faults().agent_starts();
        let output = fixture.run(&["rollback", "--target", "fixture", "--force"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(fixture.server.faults().agent_starts(), starts + 1);
        // 終わりの Shutdown は、コマンドが終わった後に偽サーバに届くことがあるので比べない
        let requests = fixture.server.faults().agent_requests()[before..].to_vec();
        sequences.push(
            requests
                .into_iter()
                .filter(|request| request != "Shutdown")
                .collect::<Vec<_>>(),
        );
    }

    // バックアップが有効な rollback はハッシュを取らない。無効な rollback は、最初の書き戻しの前に
    // 一回だけまとめて取る。どちらも二つのファイルを一回ずつ書き戻す
    let count = |requests: &[String], kind: &str| {
        requests.iter().filter(|request| *request == kind).count()
    };
    assert_eq!(count(&sequences[0], "HashFiles"), 0, "{:?}", sequences[0]);
    assert_eq!(count(&sequences[1], "HashFiles"), 1, "{:?}", sequences[1]);
    let position = |kind: &str| sequences[1].iter().position(|request| request == kind);
    assert!(
        position("HashFiles") < position("WriteFile"),
        "{:?}",
        sequences[1]
    );
    for requests in &sequences {
        assert_eq!(count(requests, "WriteFile"), 2, "{requests:?}");
    }
}

/// 書き戻す先にない "b.txt" はまとめてハッシュを取る対象に入らない。まとめのハッシュが
/// やり直しても `接続断` なら、まとめに入っていた "a.txt" は理由付きの読み込みの失敗、"b.txt" は
/// 手を付けていない失敗で、どちらも書き戻さない
fn assert_batch_read_failed_and_rest_untouched(fixture: &AgentFixture, result: &Value) {
    let failed = result["failed"].as_array().expect("failed");
    assert_eq!(failed.len(), 2, "{result}");
    let error = |path: &str| {
        failed
            .iter()
            .find(|entry| entry["path"] == path)
            .unwrap_or_else(|| panic!("{path} is not in failed: {result}"))["error"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let batch = error("a.txt");
    assert!(
        batch.contains(READ_FAILED) && !batch.contains(UNTOUCHED),
        "{result}"
    );
    assert!(error("b.txt").contains(UNTOUCHED), "{result}");
    assert_eq!(fixture.read_remote("a.txt"), "merged a\n");
    assert!(!fixture.remote().join("b.txt").exists());
}

// @kotowari[REQ-ssh-003]
#[tokio::test(flavor = "multi_thread")]
async fn an_ssh_hash_batch_lost_again_after_reconnecting_fails_its_files_as_reads_and_leaves_the_rest_untouched(
) {
    let fixture = merged_without_backup(without_agent(), Faults::default()).await;
    std::fs::remove_file(fixture.remote().join("b.txt")).unwrap();
    fixture.server.faults().arm_ssh_cut(SshCut {
        command: SshCommand::Hash,
        repeat: Repeat::Every,
    });

    let result = rollback(&fixture);

    assert_batch_read_failed_and_rest_untouched(&fixture, &result);
    assert_eq!(fixture.server.faults().ssh_cuts(), 2);
}

// @kotowari[REQ-ssh-023]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_hash_batch_lost_again_after_restarting_fails_its_files_as_reads_and_leaves_the_rest_untouched(
) {
    let fixture = merged_without_backup(Setup::default(), Faults::default()).await;
    std::fs::remove_file(fixture.remote().join("b.txt")).unwrap();
    fixture.server.faults().arm_agent_cut(AgentCut {
        request: Some("HashFiles"),
        occurrence: 1,
        point: CutPoint::BeforeResponse,
        scope: CutScope::Agent,
        repeat: Repeat::Every,
    });

    let result = rollback(&fixture);

    assert_batch_read_failed_and_rest_untouched(&fixture, &result);
}
