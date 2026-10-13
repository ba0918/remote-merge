#![cfg(unix)]
//! merge・sync・rollback の途中で書き込み先の root_dir ごと消えたとき、そのサーバへの処理を止め、残りの
//! ファイルを行わずに終了コード 2 で終えること（docs/ir/merge/concurrency.md の REQ-merge-044、
//! docs/decision/records/2026-10-12-foundation-3.md の D47・D48）の契約テスト。新しいファイルの書き込み
//! だけでなく、既にあるファイルの書く前の読み込み・--delete の削除・権限の変更で消えたときも止める。
//! root_dir がない失敗の文言は一度だけ出す。エージェントと SSH の経路で同じに確かめる。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::root_gone_support::*;

// @kotowari[REQ-merge-044, EX-merge-066]
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_stops_for_the_server_whose_root_dir_is_gone_in_the_middle() {
    for setup in routes() {
        let fixture = fixture_with(setup, &NEW).await;
        remove_root_at(&fixture, "WriteFile", SshCommand::Write, 2, When::Before);

        let output = fixture.run(&["merge", "a.txt", "b.txt", "c.txt"]);

        let result = json(&output);
        assert_eq!(paths(&result["merged"]), ["a.txt"], "{result}");
        assert_stopped(&fixture, &output, &result["failed"], "b.txt", "c.txt");
    }
}

// 既にあるファイルは、書く前の読み込みで root_dir がないと分かる。root_dir が通常ファイルに替わった
// ときも同じ
// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_of_existing_files_stops_when_the_root_dir_is_gone_before_a_read() {
    for (setup, gone) in routes_and_ways() {
        let fixture = fixture_with(setup, &EXISTING).await;
        gone_root_at(
            &fixture,
            "WriteFile",
            SshCommand::Write,
            1,
            When::AfterNext,
            gone,
        );

        let output = fixture.run(&["merge", "a.txt", "b.txt", "c.txt"]);

        let result = json(&output);
        assert_eq!(paths(&result["merged"]), ["a.txt"], "{result}");
        assert_stopped(&fixture, &output, &result["failed"], "b.txt", "c.txt");
    }
}

// 権限を写すときは、書いた後の権限の変更で root_dir がないと分かる。root_dir が通常ファイルに替わった
// ときも同じ
// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_with_permissions_stops_when_the_root_dir_is_gone_before_a_chmod() {
    for (setup, gone) in routes_and_ways() {
        let fixture = fixture_with(setup, &EXISTING).await;
        for (name, ..) in EXISTING {
            let local = fixture.local().join(name);
            fs::set_permissions(local, fs::Permissions::from_mode(0o600)).unwrap();
        }
        gone_root_at(
            &fixture,
            "WriteFile",
            SshCommand::Write,
            2,
            When::AfterNext,
            gone,
        );

        let output = fixture.run(&["merge", "a.txt", "b.txt", "c.txt", "--with-permissions"]);

        let result = json(&output);
        assert_eq!(paths(&result["merged"]), ["a.txt"], "{result}");
        assert_stopped(&fixture, &output, &result["failed"], "b.txt", "c.txt");
    }
}
