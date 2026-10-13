#![cfg(unix)]
//! sync と rollback の途中で書き込み先の root_dir ごと消えたとき、そのサーバへの処理を止め、残りの
//! ファイルを行わずに終了コード 2 で終えること（docs/ir/merge/concurrency.md の REQ-merge-044、
//! docs/decision/records/2026-10-12-foundation-3.md の D47・D48）の契約テスト。新しいファイルの書き込み
//! だけでなく、既にあるファイルの書く前の読み込み・--delete の削除・権限の変更で消えたときも止める。
//! root_dir がない失敗の文言は一度だけ出す。エージェントと SSH の経路で同じに確かめる。

use super::root_gone_support::*;

// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_sync_stops_for_the_server_whose_root_dir_is_gone_in_the_middle() {
    for setup in routes() {
        let fixture = fixture_with(setup, &NEW).await;
        remove_root_at(&fixture, "WriteFile", SshCommand::Write, 2, When::Before);

        let output = fixture.run(&["sync", ".", "--force"]);

        let result = json(&output);
        let failed = &result["targets"][0]["failed"];
        assert_stopped(&fixture, &output, failed, "b.txt", "c.txt");
    }
}

// --delete の削除で root_dir がないと分かる。root_dir が通常ファイルに替わったときも同じ
// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_sync_with_delete_stops_when_the_root_dir_is_gone_before_a_removal() {
    for (setup, gone) in routes_and_ways() {
        let fixture = fixture_with(
            setup,
            &[
                ("x.txt", None, Some("x\n")),
                ("y.txt", None, Some("y\n")),
                ("z.txt", None, Some("z\n")),
            ],
        )
        .await;
        gone_root_at(
            &fixture,
            "Remove",
            SshCommand::Remove,
            2,
            When::Before,
            gone,
        );

        let output = fixture.run(&["sync", ".", "--delete", "--force"]);

        let result = json(&output);
        let failed = &result["targets"][0]["failed"];
        assert_stopped(&fixture, &output, failed, "y.txt", "z.txt");
    }
}

// 書き戻しの途中で root_dir がないと分かる
// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_rollback_stops_when_the_root_dir_is_gone_in_the_middle() {
    for setup in routes() {
        let fixture = fixture_with(setup, &EXISTING).await;
        let merge = fixture.run(&["merge", "a.txt", "b.txt", "c.txt"]);
        assert!(merge.status.success(), "{merge:?}");
        remove_root_at(&fixture, "WriteFile", SshCommand::Write, 1, When::AfterNext);

        let output = fixture.run(&["rollback", "--target", "fixture", "--force"]);

        let result = json(&output);
        let failed = &result["failed"];
        let message = format!("root_dir is gone: fixture:{}", fixture.remote().display());
        let gone = paths(failed)
            .into_iter()
            .find(|path| failure(failed, path).contains(&message))
            .unwrap_or_else(|| panic!("no root_dir is gone: {result}"));
        let untouched = paths(failed)
            .into_iter()
            .find(|path| *path != gone)
            .unwrap_or_else(|| panic!("no untouched file: {result}"));
        assert_eq!(paths(failed).len(), 2, "{result}");
        assert_stopped(&fixture, &output, failed, gone, untouched);
    }
}
