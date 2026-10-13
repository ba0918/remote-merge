#![cfg(unix)]
//! どの側でも、どのコマンドでも、root_dir がなくなったと分かればその側への処理を止めること
//! （docs/ir/merge/concurrency.md の REQ-merge-044、docs/decision/records/2026-10-12-foundation-3.md の
//! D48）の契約テスト。読み込み元の側の root_dir が消えた merge・sync では、それを使う残りのファイルを
//! 行わない。読むだけのコマンド（diff）でも止めて終了コード 2 で終える。

use super::root_gone_support::*;

/// remote にだけ置く三つのファイル
const REMOTE_ONLY: [(&str, Option<&str>, Option<&str>); 3] = [
    ("a.txt", None, Some("remote\n")),
    ("b.txt", None, Some("remote\n")),
    ("c.txt", None, Some("remote\n")),
];

/// 二つ目の読み込みの前に、読み込み元の remote の root_dir を消して `args` を remote を左にして動かす
async fn run_with_the_source_gone(
    setup: Setup,
    args: &[&str],
) -> (AgentFixture, std::process::Output) {
    let fixture = fixture_with(setup, &REMOTE_ONLY).await;
    remove_root_at(&fixture, "ReadFiles", SshCommand::Read, 2, When::Before);
    let output = fixture.run_with_sides(args, &["--left", "fixture", "--right", "local"]);
    (fixture, output)
}

// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_from_a_source_whose_root_dir_is_gone_stops_the_rest() {
    for setup in routes() {
        let (fixture, output) =
            run_with_the_source_gone(setup, &["merge", "a.txt", "b.txt", "c.txt"]).await;

        let result = json(&output);
        assert_eq!(paths(&result["merged"]), ["a.txt"], "{result}");
        assert_stopped(&fixture, &output, &result["failed"], "b.txt", "c.txt");
        assert!(!fixture.local().join("c.txt").exists());
    }
}

// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_sync_from_a_source_whose_root_dir_is_gone_stops_the_rest() {
    for setup in routes() {
        let (fixture, output) = run_with_the_source_gone(setup, &["sync", ".", "--force"]).await;

        let result = json(&output);
        let failed = &result["targets"][0]["failed"];
        assert_stopped(&fixture, &output, failed, "b.txt", "c.txt");
        assert!(!fixture.local().join("c.txt").exists());
    }
}

// status でハッシュを比べる途中に root_dir がなくなっても、止めた側の最初の理由を一度出して終了
// コード 2 で終える
// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_status_stops_when_a_root_dir_is_gone_while_comparing_hashes() {
    for setup in routes() {
        let fixture = fixture_with(
            setup,
            &[
                ("a.txt", Some("local\n"), Some("other\n")),
                ("b.txt", Some("local\n"), Some("other\n")),
                ("c.txt", Some("local\n"), Some("other\n")),
            ],
        )
        .await;
        remove_root_at(&fixture, "HashFiles", SshCommand::Hash, 1, When::Before);

        let output = fixture.run(&["status", "--checksum"]);

        // 記録（標準エラーのログ）ではなく、コマンドの出力に一度だけ出す
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let message = format!("root_dir is gone: fixture:{}", fixture.remote().display());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.matches(&message).count(), 1, "{stdout}");
        assert!(!fixture.remote().is_dir());
    }
}

// 読むだけのコマンドでも、root_dir がなくなったと分かれば止めて終了コード 2 で終える
// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_diff_stops_when_a_root_dir_is_gone_while_reading() {
    for setup in routes() {
        let fixture = fixture_with(
            setup,
            &[
                ("a.txt", Some("local\n"), Some("remote\n")),
                ("b.txt", Some("local\n"), Some("remote\n")),
                ("c.txt", Some("local\n"), Some("remote\n")),
            ],
        )
        .await;
        remove_root_at(&fixture, "ReadFiles", SshCommand::Read, 2, When::Before);

        let output = fixture.run(&["diff", "a.txt", "b.txt", "c.txt"]);

        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let message = format!("root_dir is gone: fixture:{}", fixture.remote().display());
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(all.matches(&message).count(), 1, "{all}");
        assert!(!fixture.remote().is_dir());
    }
}
