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
        let output = fixture.run(&["rollback", "--target", "fixture", "--force"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        // 終わりの Shutdown は、コマンドが終わった後に偽サーバに届くことがあるので比べない
        let requests = fixture.server.faults().agent_requests()[before..].to_vec();
        sequences.push(
            requests
                .into_iter()
                .filter(|request| request != "Shutdown")
                .collect::<Vec<_>>(),
        );
    }

    assert_eq!(
        sequences[0],
        [
            "InspectPath",
            "InspectPath",
            "InspectPath",
            "InspectPath",
            "ReadFiles",
            "WriteFile",
            "InspectPath",
            "InspectPath",
            "ReadFiles",
            "WriteFile",
        ]
    );
    assert_eq!(
        sequences[1],
        [
            "InspectPath",
            "InspectPath",
            "HashFiles",
            "InspectPath",
            "WriteFile",
            "InspectPath",
            "WriteFile",
        ]
    );
}
