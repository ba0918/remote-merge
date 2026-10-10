#![cfg(unix)]
//! `接続断` でその書き込み先への処理を止め、どこまで済んだかを failed に出すこと
//! （docs/ir/ssh/reconnect.md の REQ-ssh-022・REQ-ssh-003・REQ-ssh-023、
//! docs/ir/merge/permissions.md の REQ-merge-037、docs/ir/backup/failure.md の REQ-backup-017）の
//! 契約テスト。
//!
//! failed の error は文言を固定せず、区別すべきことを示す語で見分ける。
//! 書いていない: "not written"、書きかけ: "partially written"、消していない: "not deleted"、
//! 権限は変えていない: "permissions not changed"、結果が分からない: "result unknown"、
//! 手を付けていない: "not attempted"、読み込みに失敗した: "read failed"、中身は書いた: "content written"。
//!
//! 変更の要求をエージェントに渡す前に切る道具はないので、「要求が書かれる前に切れる」は、変更の
//! 直前の要求の応答を返した後にエージェントとの間を切って起こす。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use serde_json::Value;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::{AgentCut, CutPoint, CutScope, Faults, Repeat, SshCommand, SshCut};

const NOT_WRITTEN: &str = "not written";
const PARTIAL: &str = "partially written";
const NOT_DELETED: &str = "not deleted";
const PERMISSIONS_UNCHANGED: &str = "permissions not changed";
const UNKNOWN: &str = "result unknown";
const UNTOUCHED: &str = "not attempted";
const READ_FAILED: &str = "read failed";
const CONTENT_WRITTEN: &str = "content written";

fn cut(request: &'static str, occurrence: usize, point: CutPoint, repeat: Repeat) -> Faults {
    Faults {
        agent_cut: Some(AgentCut {
            request: Some(request),
            occurrence,
            point,
            scope: CutScope::Agent,
            repeat,
        }),
        ..Faults::default()
    }
}

/// `failed` の中の `path` の error
fn failure<'a>(failed: &'a Value, path: &str) -> &'a str {
    failed
        .as_array()
        .unwrap_or_else(|| panic!("failed missing: {failed}"))
        .iter()
        .find(|entry| entry["path"] == path)
        .unwrap_or_else(|| panic!("{path} is not in failed: {failed}"))["error"]
        .as_str()
        .unwrap()
}

fn paths(entries: &Value) -> Vec<&str> {
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

fn count(fixture: &AgentFixture, request: &str) -> usize {
    fixture
        .server
        .faults()
        .agent_requests()
        .iter()
        .filter(|seen| *seen == request)
        .count()
}

/// SSH の偽サーバが受けた、remote の root_dir のファイルの中身を書くコマンド
fn ssh_writes(fixture: &AgentFixture) -> Vec<String> {
    fixture
        .remote_commands()
        .into_iter()
        .filter(|command| SshCommand::Write.matches(command))
        .collect()
}

async fn three_files(faults: Faults) -> AgentFixture {
    let fixture = AgentFixture::new(Setup::default(), faults).await;
    for name in ["a.txt", "b.txt", "c.txt"] {
        fixture.put(name, Some("local\n"), Some("remote old\n"));
    }
    fixture
}

// @kotowari[EX-ssh-038]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_lost_before_writing_is_restarted_to_confirm_the_file_was_not_written() {
    // 一つのファイルの merge は 4 番目の読み込み（バックアップ）の直後に書き込む
    let fixture = AgentFixture::new(
        Setup::default(),
        cut("ReadFiles", 4, CutPoint::AfterResponse, Repeat::Once),
    )
    .await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let error = failure(&result["failed"], "a.txt");
    assert!(error.contains(NOT_WRITTEN), "{error}");
    assert_eq!(fixture.read_remote("a.txt"), "remote old\n");
    assert_eq!(fixture.server.faults().agent_starts(), 2);
    assert!(
        ssh_writes(&fixture).is_empty(),
        "{:#?}",
        ssh_writes(&fixture)
    );
}

// @kotowari[REQ-ssh-022, EX-ssh-039]
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_cut_before_the_second_write_reports_written_not_written_and_untouched_files() {
    // "b.txt" の書き込みは 6 番目の読み込み（"b.txt" のバックアップ）の直後。SSH の session ごと切る
    let mut faults = cut("ReadFiles", 6, CutPoint::AfterResponse, Repeat::Once);
    if let Some(cut) = faults.agent_cut.as_mut() {
        cut.scope = CutScope::Session;
    }
    let fixture = three_files(faults).await;

    let output = fixture.run(&["merge", "a.txt", "b.txt", "c.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    assert_eq!(paths(&result["merged"]), ["a.txt"], "{result}");
    let b = failure(&result["failed"], "b.txt");
    assert!(b.contains(NOT_WRITTEN) && !b.contains(UNTOUCHED), "{b}");
    assert!(
        failure(&result["failed"], "c.txt").contains(UNTOUCHED),
        "{result}"
    );
    assert_eq!(fixture.read_remote("a.txt"), "local\n");
    assert_eq!(fixture.read_remote("b.txt"), "remote old\n");
    assert_eq!(fixture.read_remote("c.txt"), "remote old\n");
    assert!(
        ssh_writes(&fixture).is_empty(),
        "{:#?}",
        ssh_writes(&fixture)
    );
}

// @kotowari[EX-ssh-040]
#[tokio::test(flavor = "multi_thread")]
async fn a_single_file_that_cannot_be_written_fails_alone_and_the_rest_are_written() {
    let fixture = three_files(Faults::default()).await;
    let b = fixture.remote().join("b.txt");
    fs::set_permissions(&b, fs::Permissions::from_mode(0o444)).unwrap();

    let output = fixture.run(&["merge", "a.txt", "b.txt", "c.txt"]);
    fs::set_permissions(&b, fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    assert_eq!(paths(&result["failed"]), ["b.txt"], "{result}");
    assert_eq!(fixture.read_remote("a.txt"), "local\n");
    assert_eq!(fixture.read_remote("b.txt"), "remote old\n");
    assert_eq!(fixture.read_remote("c.txt"), "local\n");
}

// @kotowari[EX-ssh-042]
#[tokio::test(flavor = "multi_thread")]
async fn a_read_lost_again_after_restarting_the_agent_stops_without_writing_or_switching_to_ssh() {
    let fixture = AgentFixture::new(
        Setup::default(),
        cut("ReadFiles", 1, CutPoint::BeforeResponse, Repeat::Every),
    )
    .await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));
    fixture.put("b.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt", "b.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(fixture.read_remote("a.txt"), "remote old\n");
    assert_eq!(fixture.read_remote("b.txt"), "remote old\n");
    // まとめて読む処理は一度だけやり直し、1 ファイルずつ読み直さない
    assert_eq!(count(&fixture, "ReadFiles"), 2);
    assert_eq!(fixture.server.faults().agent_starts(), 2);
    let over_ssh: Vec<String> = fixture
        .remote_commands()
        .into_iter()
        .filter(|command| SshCommand::Read.matches(command) || SshCommand::Write.matches(command))
        .collect();
    assert!(over_ssh.is_empty(), "{over_ssh:#?}");
}

// @kotowari[EX-ssh-043]
#[tokio::test(flavor = "multi_thread")]
async fn a_deletion_cut_before_it_is_requested_reports_not_deleted_and_leaves_the_rest_untouched() {
    // "x.txt" の削除は、最初の読み込み（"x.txt" のバックアップ）の直後
    let fixture = AgentFixture::new(
        Setup::default(),
        cut("ReadFiles", 1, CutPoint::AfterResponse, Repeat::Once),
    )
    .await;
    fixture.put("x.txt", None, Some("x\n"));
    fixture.put("y.txt", None, Some("y\n"));

    let output = fixture.run(&["sync", ".", "--delete", "--force"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let failed = &result["targets"][0]["failed"];
    assert!(failure(failed, "x.txt").contains(NOT_DELETED), "{result}");
    assert!(failure(failed, "y.txt").contains(UNTOUCHED), "{result}");
    assert_eq!(fixture.read_remote("x.txt"), "x\n");
    assert_eq!(fixture.read_remote("y.txt"), "y\n");
}

// @kotowari[EX-ssh-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_rollback_cut_before_the_first_restore_reports_not_written_and_an_untouched_file() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a.txt", Some("merged a\n"), Some("original a\n"));
    fixture.put("b.txt", Some("merged b\n"), Some("original b\n"));
    let merge = fixture.run(&["merge", "a.txt", "b.txt"]);
    assert!(merge.status.success(), "{merge:?}");
    // 最初の書き戻しは、最初の読み込み（書き戻す前の内容のバックアップ）の直後
    fixture.server.faults().arm_agent_cut(AgentCut {
        request: Some("ReadFiles"),
        occurrence: 1,
        point: CutPoint::AfterResponse,
        scope: CutScope::Agent,
        repeat: Repeat::Once,
    });

    let output = fixture.run(&["rollback", "--target", "fixture", "--force"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let failed = &result["failed"];
    let first = paths(failed)[0];
    let second = if first == "a.txt" { "b.txt" } else { "a.txt" };
    let error = failure(failed, first);
    assert!(
        error.contains(NOT_WRITTEN) && !error.contains(UNTOUCHED),
        "{result}"
    );
    assert!(failure(failed, second).contains(UNTOUCHED), "{result}");
    for name in ["a.txt", "b.txt"] {
        assert_eq!(
            fixture.read_remote(name),
            format!("merged {}\n", &name[..1])
        );
    }
}

// @kotowari[EX-ssh-045]
#[tokio::test(flavor = "multi_thread")]
async fn a_sync_cut_on_one_target_leaves_its_rest_untouched_and_writes_every_file_to_the_other() {
    let mut fixture =
        three_files(cut("WriteFile", 1, CutPoint::BeforeResponse, Repeat::Once)).await;
    let second = fixture.add_server("second", Faults::default()).await;
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(second.join(name), "remote old\n").unwrap();
    }

    let output = fixture.run_with_sides(
        &["sync", ".", "--force"],
        &["--left", "local", "--right", "fixture", "second"],
    );

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let targets = result["targets"].as_array().unwrap();
    let cut_target = targets
        .iter()
        .find(|target| target["target"]["label"] == "fixture")
        .unwrap();
    // "a.txt" は書けたことを確かめて済んだファイルになり、残りには手を付けない
    let failed = &cut_target["failed"];
    assert_eq!(paths(failed), ["b.txt", "c.txt"], "{result}");
    assert!(failure(failed, "b.txt").contains(UNTOUCHED), "{result}");
    assert!(failure(failed, "c.txt").contains(UNTOUCHED), "{result}");
    assert_eq!(fixture.read_remote("a.txt"), "local\n");
    assert_eq!(fixture.read_remote("b.txt"), "remote old\n");
    for name in ["a.txt", "b.txt", "c.txt"] {
        assert_eq!(fs::read_to_string(second.join(name)).unwrap(), "local\n");
    }
}

// @kotowari[EX-ssh-047]
#[tokio::test(flavor = "multi_thread")]
async fn a_permission_change_cut_after_writing_reports_written_content_and_unchanged_permissions() {
    // エージェントは新しいファイルを既定の権限で書くので、それと違う読み込み元の権限を写す。
    // 権限の変更は書き込みの直後
    let fixture = AgentFixture::new(
        Setup::default(),
        cut("WriteFile", 1, CutPoint::AfterResponse, Repeat::Once),
    )
    .await;
    fixture.put("a.txt", Some("new\n"), None);
    fs::set_permissions(
        fixture.local().join("a.txt"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();

    let output = fixture.run(&["merge", "a.txt", "--with-permissions"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let error = failure(&result["failed"], "a.txt");
    assert!(
        error.contains(CONTENT_WRITTEN) && error.contains(PERMISSIONS_UNCHANGED),
        "{error}"
    );
    assert_eq!(fixture.read_remote("a.txt"), "new\n");
    let mode = fs::metadata(fixture.remote().join("a.txt"))
        .unwrap()
        .permissions()
        .mode();
    assert_ne!(mode & 0o777, 0o600);
}

// @kotowari[EX-ssh-046]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_gone_before_the_next_change_is_confirmed_and_the_rest_is_untouched() {
    // "a.txt" を書いた後、"b.txt" の書き込みの要求の前にエージェントが終わっている。
    // "b.txt" の読み込みは起動し直して行うので、終わっているのは "b.txt" の最後の読み込み
    // （6 番目の読み込み、バックアップ）の後
    let fixture = three_files(cut("ReadFiles", 6, CutPoint::AfterResponse, Repeat::Once)).await;

    let output = fixture.run(&["merge", "a.txt", "b.txt", "c.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    assert_eq!(paths(&result["merged"]), ["a.txt"], "{result}");
    let b = failure(&result["failed"], "b.txt");
    assert!(b.contains(NOT_WRITTEN) && !b.contains(UNTOUCHED), "{b}");
    assert!(
        failure(&result["failed"], "c.txt").contains(UNTOUCHED),
        "{result}"
    );
    assert_eq!(fixture.read_remote("b.txt"), "remote old\n");
    assert_eq!(fixture.read_remote("c.txt"), "remote old\n");
    assert!(
        ssh_writes(&fixture).is_empty(),
        "{:#?}",
        ssh_writes(&fixture)
    );
}

// @kotowari[EX-ssh-050]
#[tokio::test(flavor = "multi_thread")]
async fn a_write_done_before_its_reply_is_lost_is_confirmed_as_written() {
    let fixture = AgentFixture::new(
        Setup::default(),
        cut("WriteFile", 1, CutPoint::BeforeResponse, Repeat::Once),
    )
    .await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));
    fixture.put("b.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt", "b.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    assert_eq!(paths(&result["merged"]), ["a.txt"], "{result}");
    assert_eq!(paths(&result["failed"]), ["b.txt"], "{result}");
    assert!(
        failure(&result["failed"], "b.txt").contains(UNTOUCHED),
        "{result}"
    );
    assert_eq!(fixture.read_remote("a.txt"), "local\n");
    assert_eq!(fixture.read_remote("b.txt"), "remote old\n");
    assert_eq!(count(&fixture, "WriteFile"), 1);
    assert_eq!(fixture.server.faults().agent_starts(), 2);
}

/// エージェントが一度に書く大きさ（4 MiB）を超える中身
fn larger_than_one_chunk(byte: u8) -> Vec<u8> {
    vec![byte; 4 * 1024 * 1024 + 1024]
}

/// 手元に保存したバックアップの session の名前
pub fn backup_sessions(fixture: &AgentFixture) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![fixture.home.path().to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || path.starts_with(fixture.remote()) {
                continue;
            }
            if dir.file_name().is_some_and(|name| name == "sessions") {
                found.push(entry.file_name().to_string_lossy().into_owned());
            } else {
                pending.push(path);
            }
        }
    }
    found
}

// @kotowari[EX-ssh-051]
#[tokio::test(flavor = "multi_thread")]
async fn a_write_cut_between_its_chunks_is_reported_as_partial_with_the_backup_location() {
    // 分けて送る書き込みの最初の一つを書いた後で、次の一つを送る前にエージェントとの間が切れる
    let fixture = AgentFixture::new(
        Setup::default(),
        cut("WriteFile", 1, CutPoint::AfterResponse, Repeat::Once),
    )
    .await;
    let new = larger_than_one_chunk(b'n');
    fs::write(fixture.local().join("big.bin"), &new).unwrap();
    fs::write(
        fixture.remote().join("big.bin"),
        larger_than_one_chunk(b'o'),
    )
    .unwrap();

    let output = fixture.run(&["merge", "big.bin"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let error = failure(&result["failed"], "big.bin");
    assert!(error.contains(PARTIAL), "{error}");
    let sessions = backup_sessions(&fixture);
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert!(
        error.contains(&format!("{}/big.bin", sessions[0])),
        "{error}"
    );
    // 書き戻さず、途中までの中身が残る
    let remote = fs::read(fixture.remote().join("big.bin")).unwrap();
    assert!(
        remote.len() < new.len() && new.starts_with(&remote),
        "{}",
        remote.len()
    );
}

// @kotowari[EX-ssh-052]
#[tokio::test(flavor = "multi_thread")]
async fn a_change_that_cannot_be_confirmed_after_the_cut_is_reported_as_unknown() {
    let mut faults = cut("WriteFile", 1, CutPoint::BeforeResponse, Repeat::Once);
    faults.refuse_agent_after_cut = true;
    let fixture = AgentFixture::new(Setup::default(), faults).await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));
    fixture.put("b.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt", "b.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let a = failure(&result["failed"], "a.txt");
    assert!(a.contains(UNKNOWN) && !a.contains(UNTOUCHED), "{a}");
    assert!(
        failure(&result["failed"], "b.txt").contains(UNTOUCHED),
        "{result}"
    );
    assert_eq!(fixture.read_remote("b.txt"), "remote old\n");
    assert_eq!(count(&fixture, "WriteFile"), 1);
    // つなぎ直そうとしたが、エージェントは起動しなかった
    let start_attempts = fixture
        .server
        .commands()
        .iter()
        .filter(|command| command.contains(" agent --root "))
        .count();
    assert!(start_attempts >= 2, "{start_attempts}");
    assert_eq!(fixture.server.faults().agent_starts(), 1);
    assert!(
        ssh_writes(&fixture).is_empty(),
        "{:#?}",
        ssh_writes(&fixture)
    );
}

// @kotowari[REQ-ssh-004, EX-ssh-007]
#[tokio::test(flavor = "multi_thread")]
async fn an_ssh_write_cut_is_not_repeated_and_the_confirmed_result_is_reported() {
    let fixture = AgentFixture::new(
        Setup {
            agent_disabled: true,
            ..Setup::default()
        },
        Faults {
            ssh_cut: Some(SshCut {
                command: SshCommand::Write,
                repeat: Repeat::Once,
            }),
            ..Faults::default()
        },
    )
    .await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let error = failure(&result["failed"], "a.txt");
    assert!(error.contains(NOT_WRITTEN), "{error}");
    assert_eq!(fixture.server.faults().ssh_cuts(), 1);
    assert_eq!(ssh_writes(&fixture).len(), 1, "{:#?}", ssh_writes(&fixture));
    assert_eq!(fixture.read_remote("a.txt"), "remote old\n");
}

// @kotowari[REQ-ssh-022]
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_without_a_cut_sends_no_request_for_confirmation() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt"]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    // 終わりの Shutdown は、コマンドが終わった後に偽サーバに届くことがあるので比べない
    let requests: Vec<String> = fixture
        .server
        .faults()
        .agent_requests()
        .into_iter()
        .filter(|request| request != "Shutdown")
        .collect();
    assert_eq!(
        requests,
        [
            "ListTree",
            "StatFiles",
            "ReadFiles",
            "StatFiles",
            "ReadFiles",
            "StatFiles",
            "ReadFiles",
            "InspectPath",
            "StatFiles",
            "ReadFiles",
            "WriteFile",
        ]
    );
}

// @kotowari[EX-ssh-048]
#[tokio::test(flavor = "multi_thread")]
async fn an_ssh_read_lost_again_after_reconnecting_fails_that_file_and_leaves_the_rest_untouched() {
    let fixture = AgentFixture::new(
        Setup {
            agent_disabled: true,
            ..Setup::default()
        },
        Faults {
            ssh_cut: Some(SshCut {
                command: SshCommand::PathInspection,
                repeat: Repeat::Every,
            }),
            ..Faults::default()
        },
    )
    .await;
    fixture.put("a.txt", Some("a\n"), None);
    fixture.put("b.txt", Some("b\n"), None);

    let output = fixture.run(&["merge", "a.txt", "b.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    assert!(
        failure(&result["failed"], "a.txt").contains(READ_FAILED),
        "{result}"
    );
    assert!(
        failure(&result["failed"], "b.txt").contains(UNTOUCHED),
        "{result}"
    );
    assert_eq!(fixture.server.faults().ssh_cuts(), 2);
    assert!(!fixture.remote().join("a.txt").exists());
    assert!(!fixture.remote().join("b.txt").exists());
}

// @kotowari[REQ-backup-017]
#[tokio::test(flavor = "multi_thread")]
async fn a_backup_read_lost_again_after_the_retry_stops_instead_of_failing_only_that_file() {
    // ファイルごとに、書き込み先の確かめの読み込み（3 番目）の後にバックアップの読み込み
    // （4 番目）をする。バックアップの読み込みから後は毎回切る
    let fixture = AgentFixture::new(
        Setup::default(),
        cut("ReadFiles", 4, CutPoint::BeforeResponse, Repeat::Every),
    )
    .await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));
    fixture.put("b.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt", "b.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let a = failure(&result["failed"], "a.txt");
    assert!(
        a.contains("backup failed") && a.contains(READ_FAILED),
        "{a}"
    );
    assert!(
        failure(&result["failed"], "b.txt").contains(UNTOUCHED),
        "{result}"
    );
    assert_eq!(fixture.read_remote("a.txt"), "remote old\n");
    assert_eq!(fixture.read_remote("b.txt"), "remote old\n");
}

// @kotowari[REQ-merge-037, EX-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_permission_copy_reports_the_written_content_with_exit_code_2() {
    let fixture = AgentFixture::new(
        Setup {
            agent_disabled: true,
            ..Setup::default()
        },
        Faults {
            refuse_ssh: Some(SshCommand::Chmod),
            ..Faults::default()
        },
    )
    .await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt", "--with-permissions"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    assert!(
        failure(&result["failed"], "a.txt").contains(CONTENT_WRITTEN),
        "{result}"
    );
    assert_eq!(fixture.read_remote("a.txt"), "local\n");
}

// @kotowari[EX-merge-051]
#[tokio::test(flavor = "multi_thread")]
async fn a_setuid_source_mode_is_not_copied_and_the_written_content_is_reported() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("run.sh", Some("#!/bin/sh\necho new\n"), Some("#!/bin/sh\n"));
    let local = fixture.local().join("run.sh");
    let remote = fixture.remote().join("run.sh");
    fs::set_permissions(&local, fs::Permissions::from_mode(0o4755)).unwrap();
    fs::set_permissions(&remote, fs::Permissions::from_mode(0o644)).unwrap();

    let output = fixture.run(&["merge", "run.sh", "--with-permissions"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let error = failure(&result["failed"], "run.sh");
    assert!(
        error.contains(CONTENT_WRITTEN) && error.contains("setuid"),
        "{error}"
    );
    assert_eq!(fixture.read_remote("run.sh"), "#!/bin/sh\necho new\n");
    assert_eq!(
        fs::metadata(&remote).unwrap().permissions().mode() & 0o7777,
        0o644
    );
}
