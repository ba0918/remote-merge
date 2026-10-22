#![cfg(unix)]
//! 比べた後・最初の変更の前に書き込み先の root_dir がなくなったとき、merge・sync・rollback・--hunks・
//! --dry-run のどれでも、パスの深さによらず、root_dir がない error を一つのファイルにだけ出し、残りを
//! 手を付けていない error にすること（docs/ir/merge/concurrency.md の REQ-merge-044）の契約テスト。
//! root_dir がない error を付けるのは、まとめて尋ねたことでそれが分かったまとめの最初のファイル

use super::common::stdout_json;
use super::root_gone_support::*;

// --dry-run も、書き込み先のパスの確認で root_dir がないと分かれば止める。root 直下のパスでも同じ
// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_merge_of_root_level_paths_stops_when_the_root_dir_is_gone() {
    let fixture = fixture_with(&EXISTING).await;
    remove_root_at(&fixture, "InspectPaths", 1, When::Before);

    let output = fixture.run(&["merge", "a.txt", "b.txt", "c.txt", "--dry-run"]);

    let result = stdout_json(&output);
    assert_stopped(&fixture, &output, &result["failed"], "a.txt", "b.txt");
    assert!(failure(&result["failed"], "c.txt").contains(UNTOUCHED));
}

// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_sync_of_root_level_paths_stops_when_the_root_dir_is_gone() {
    let fixture = fixture_with(&NEW).await;
    remove_root_at(&fixture, "InspectPaths", 1, When::Before);

    let output = fixture.run(&["sync", ".", "--dry-run", "--force"]);

    let result = stdout_json(&output);
    let failed = &result["targets"][0]["failed"];
    assert_stopped(&fixture, &output, failed, "a.txt", "b.txt");
}

// --hunks の一つのファイルも、--dry-run で root_dir がないと分かれば failed にして終了コード 2
// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_hunk_merge_stops_when_the_root_dir_is_gone() {
    let fixture = fixture_with(&EXISTING).await;
    remove_root_at(&fixture, "InspectPaths", 1, When::Before);

    let output = fixture.run(&["merge", "a.txt", "--hunks", "0", "--dry-run", "--force"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = stdout_json(&output);
    let message = format!("root_dir is gone: fixture:{}", fixture.remote().display());
    assert!(
        failure(&result["failed"], "a.txt").contains(&message),
        "{result}"
    );
}

// バックアップのまとめた読み込みで分かったときは、その読み込みに入っていた最初のファイルに付け、
// バックアップしない新しいファイルは手を付けていないものにする
// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_root_dir_found_gone_by_the_backup_read_is_reported_on_the_file_read() {
    const MIXED: [(&str, Option<&str>, Option<&str>); 2] = [
        ("a.txt", Some("local\n"), None),
        ("b.txt", Some("local\n"), Some("remote old\n")),
    ];
    let fixture = fixture_with(&MIXED).await;
    remove_root_at(&fixture, "ReadFiles", 1, When::Before);

    let output = fixture.run(&["merge", "a.txt", "b.txt"]);

    let result = stdout_json(&output);
    assert_stopped(&fixture, &output, &result["failed"], "b.txt", "a.txt");
    let gone = failure(&result["failed"], "b.txt");
    assert!(gone.contains("cannot read b.txt"), "{gone}");
}

// rollback も、退避のためのまとめた読み込みで分かれば、最初のファイルにだけ root_dir がない error を
// 付け、残りは手を付けていない error にする
// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_rollback_whose_root_dir_is_gone_before_the_save_reports_it_once() {
    let fixture = fixture_with(&EXISTING).await;
    let merge = fixture.run(&["merge", "a.txt", "b.txt", "c.txt"]);
    assert!(merge.status.success(), "{merge:?}");
    remove_root_at(&fixture, "ReadFiles", 1, When::Before);

    let output = fixture.run(&["rollback", "--target", "fixture", "--force"]);

    let result = stdout_json(&output);
    assert_stopped(&fixture, &output, &result["failed"], "a.txt", "b.txt");
    assert!(failure(&result["failed"], "c.txt").contains(UNTOUCHED));
}

// rollback が戻す先の事実を集めるときに root_dir がなければ、パスの深さと --dry-run によらず、
// 走査の root_dir の誤りと同じ誤りで止める
// @kotowari[REQ-merge-044, REQ-scan-015]
#[tokio::test(flavor = "multi_thread")]
async fn a_rollback_without_the_root_dir_stops_the_same_way_for_any_depth() {
    let deep = [
        ("sub/a.txt", Some("local\n"), Some("remote old\n")),
        ("sub/b.txt", Some("local\n"), Some("remote old\n")),
    ];
    for files in [&EXISTING[..2], &deep[..]] {
        for dry_run in [false, true] {
            let fixture = fixture_with(files).await;
            let mut merge = vec!["merge"];
            merge.extend(files.iter().map(|(name, ..)| *name));
            assert!(fixture.run(&merge).status.success());
            remove_root_at(&fixture, "InspectPaths", 1, When::Before);
            let mut args = vec!["rollback", "--target", "fixture", "--force"];
            if dry_run {
                args.push("--dry-run");
            }

            let output = fixture.run(&args);

            assert_eq!(output.status.code(), Some(2), "{output:?}");
            let shown = format!("{output:?}");
            assert!(
                shown.contains("Remote root_dir not found: fixture:"),
                "{files:?} {dry_run}: {shown}"
            );
            assert!(!fixture.remote().exists(), "{output:?}");
        }
    }
}
