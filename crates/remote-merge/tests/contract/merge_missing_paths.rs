#![cfg(unix)]
//! merge と sync が、左右のどちらにもないパスを警告して failed に入れ、指定したパスがすべてない
//! ときは書き込まずに止まること（docs/ir/merge/basic.md の REQ-merge-035）の契約テスト。
//!
//! 警告は標準エラーに出るので、実行ファイルを試験 SSH サーバに対して
//! `--left local --right develop` で起動する。警告と failed の error の文言は IR で決めていないので、
//! 警告がそのパスを示すことだけを確かめる。

use std::process::Output;

use super::common::CliEnv;

/// 左にだけ "a.txt" があり、"missing.txt" は左右のどちらにもない
fn env() -> CliEnv {
    CliEnv::new(&[("a.txt", "new\n")], &[])
}

fn run(env: &CliEnv, command: &str, extra: &[&str]) -> Output {
    let mut cmd = env.cmd_with(command);
    cmd.args(["--left", "local", "--format", "json"]);
    match command {
        "sync" => cmd.args(["--right", "develop", "--force"]),
        _ => cmd.args(["--right", "develop"]),
    };
    cmd.args(extra).output().unwrap()
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"))
}

/// merge の出力か sync の最初の書き込み先の、`key` の項目のパス
fn paths(json: &serde_json::Value, command: &str, key: &str) -> Vec<String> {
    let result = match command {
        "sync" => &json["targets"][0],
        _ => json,
    };
    result[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["path"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

// @kotowari[REQ-merge-035, EX-merge-042]
#[test]
fn ex_merge_042_a_path_on_neither_side_is_warned_and_failed_in_a_dry_run() {
    for command in ["merge", "sync"] {
        let env = env();

        let output = run(&env, command, &["a.txt", "missing.txt", "--dry-run"]);

        let json = json(&output);
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("missing.txt"),
            "{command}: {output:?}"
        );
        assert!(
            paths(&json, command, "failed").contains(&"missing.txt".to_string()),
            "{command}: {json}"
        );
        assert!(
            !paths(&json, command, "merged").contains(&"missing.txt".to_string()),
            "{command}: {json}"
        );
        assert!(!env.remote_dir.join("a.txt").exists(), "{command}");
    }
}

// @kotowari[REQ-merge-035]
#[test]
fn req_merge_035_a_path_on_neither_side_is_warned_and_failed_when_writing() {
    for command in ["merge", "sync"] {
        let env = env();

        let output = run(&env, command, &["a.txt", "missing.txt"]);

        let json = json(&output);
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("missing.txt"),
            "{command}: {output:?}"
        );
        assert!(
            paths(&json, command, "failed").contains(&"missing.txt".to_string()),
            "{command}: {json}"
        );
        assert_eq!(
            std::fs::read_to_string(env.remote_dir.join("a.txt")).unwrap(),
            "new\n",
            "{command}"
        );
    }
}

// @kotowari[REQ-merge-035]
#[test]
fn req_merge_035_every_path_on_neither_side_stops_without_writing() {
    for command in ["merge", "sync"] {
        for dry_run in [false, true] {
            let env = env();
            let mut extra = vec!["missing.txt", "gone.txt"];
            if dry_run {
                extra.push("--dry-run");
            }

            let output = run(&env, command, &extra);

            assert_eq!(
                output.status.code(),
                Some(2),
                "{command} dry_run={dry_run}: {output:?}"
            );
            let entries: Vec<_> = std::fs::read_dir(&env.remote_dir).unwrap().collect();
            assert!(entries.is_empty(), "{command} dry_run={dry_run}");
        }
    }
}

/// 読み込み元の "x" は通常ファイル、書き込み先の "x" は "x/only.txt" を持つディレクトリで、
/// "other.txt" は左右で中身が違う。"x/nope.txt" は左右のどちらにもない
fn env_under_a_different_kind() -> CliEnv {
    CliEnv::new(
        &[("x", "incoming\n"), ("other.txt", "incoming\n")],
        &[("x/only.txt", "existing\n"), ("other.txt", "existing\n")],
    )
}

// @kotowari[REQ-merge-035, REQ-merge-001]
#[test]
fn req_merge_035_a_path_on_neither_side_under_a_different_kind_is_failed_not_skipped() {
    for command in ["merge", "sync"] {
        for dry_run in [false, true] {
            let label = format!("{command} dry_run={dry_run}");
            let mut only_missing = vec!["--delete", "x/nope.txt"];
            let mut with_other = vec!["--delete", "x/nope.txt", "other.txt"];
            if dry_run {
                only_missing.push("--dry-run");
                with_other.push("--dry-run");
            }

            let env = env_under_a_different_kind();
            let stopped = run(&env, command, &only_missing);
            assert_eq!(stopped.status.code(), Some(2), "{label}: {stopped:?}");
            assert_eq!(
                std::fs::read_to_string(env.remote_dir.join("other.txt")).unwrap(),
                "existing\n",
                "{label}"
            );

            let env = env_under_a_different_kind();
            let output = run(&env, command, &with_other);
            let json = json(&output);
            assert_eq!(
                paths(&json, command, "failed"),
                ["x/nope.txt"],
                "{label}: {json}"
            );
            assert!(
                paths(&json, command, "skipped").is_empty(),
                "{label}: {json}"
            );
            assert_eq!(
                std::fs::read_to_string(env.remote_dir.join("x/only.txt")).unwrap(),
                "existing\n",
                "{label}"
            );
        }
    }
}

// ─── sync の書き込み先が複数のとき ─────────────────────────────

/// sync の読み込み元と書き込み先 "a"・"b" をローカルのディレクトリにした組み立て
struct MultiTarget {
    dirs: [tempfile::TempDir; 4],
    config: remote_merge::config::AppConfig,
    targets: remote_merge::runtime::RuntimeTargets,
}

/// 三つとも "keep.txt" を持ち、"x.txt" は `b_has_x` のときだけ "b" にある
fn multi_target(b_has_x: bool) -> MultiTarget {
    use super::common::place_files;
    let dirs = [(); 4].map(|_| tempfile::TempDir::new().unwrap());
    let [source, a, b, config_dir] = &dirs;
    for root in [source.path(), a.path(), b.path()] {
        place_files(root, &[("keep.txt", "same\n")]);
    }
    if b_has_x {
        place_files(b.path(), &[("x.txt", "only on b\n")]);
    }
    let config_path = config_dir.path().join("config.toml");
    let server = |name: &str, root: &std::path::Path| {
        format!(
            "[servers.{name}]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n",
            root.display().to_string()
        )
    };
    std::fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = {:?}\n{}{}[backup]\nenabled = false\n",
            source.path().display().to_string(),
            server("a", a.path()),
            server("b", b.path()),
        ),
    )
    .unwrap();
    let config = remote_merge::config::load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = remote_merge::runtime::RuntimeTargets::production()
        .with_local("a", a.path())
        .with_local("b", b.path())
        .with_startup_directory(config_dir.path().to_path_buf());
    MultiTarget {
        dirs,
        config,
        targets,
    }
}

fn sync_x_with_delete(
    fixture: &MultiTarget,
    right: &[&str],
) -> anyhow::Result<remote_merge::cli::sync::SyncCommandResult> {
    let args = remote_merge::cli::sync::SyncArgs {
        paths: vec!["x.txt".into()],
        left: Some("local".into()),
        right: right.iter().map(|name| name.to_string()).collect(),
        dry_run: true,
        force: true,
        delete: true,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    };
    remote_merge::cli::sync::execute_sync(args, fixture.config.clone(), fixture.targets.clone())
}

// 指定したパスがある書き込み先があれば止まらず、ない書き込み先ではそのパスを failed に入れる
// @kotowari[REQ-merge-035]
#[test]
fn req_merge_035_sync_fails_a_path_only_for_the_target_where_it_is_on_neither_side() {
    use remote_merge::cli::sync::SyncCommandOutput;

    for right in [["a", "b"], ["b", "a"]] {
        let fixture = multi_target(true);

        let result = sync_x_with_delete(&fixture, &right)
            .unwrap_or_else(|error| panic!("{right:?}: {error}"));

        let SyncCommandOutput::Result(output) = &result.output else {
            panic!("{right:?}: expected sync result")
        };
        let target = |label: &str| {
            output
                .targets
                .iter()
                .find(|target| target.target.label == label)
                .unwrap_or_else(|| panic!("{right:?}: {label} missing: {output:?}"))
        };
        let failed: Vec<&str> = target("a").failed.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(failed, ["x.txt"], "{right:?}: {output:?}");
        assert!(target("a").merged.is_empty(), "{right:?}: {output:?}");
        let deleted: Vec<&str> = target("b")
            .deleted
            .iter()
            .map(|d| d.path.as_str())
            .collect();
        assert_eq!(deleted, ["x.txt"], "{right:?}: {output:?}");
        assert!(target("b").failed.is_empty(), "{right:?}: {output:?}");
    }
}

// 指定したパスがどの書き込み先でも左右のどちらにもないときは、エラーで止まる
// @kotowari[REQ-merge-035]
#[test]
fn req_merge_035_sync_stops_when_the_path_is_on_neither_side_for_every_target() {
    let fixture = multi_target(false);

    let result = sync_x_with_delete(&fixture, &["a", "b"]);

    let error = result
        .map(|result| result.exit_code)
        .expect_err("sync went on without the path on any side");
    assert!(
        format!("{error:#}").contains("not found on either side"),
        "{error:#}"
    );
}

// 書き込み先のツリーを取れないときは、その書き込み先でパスがあるかが分からないので、ほかの
// 書き込み先でパスがすべてなくても止まらず、その失敗と failed を報告する
// @kotowari[REQ-merge-035, REQ-merge-015]
#[test]
fn req_merge_035_sync_does_not_stop_when_a_target_could_not_be_scanned() {
    use remote_merge::cli::sync::SyncCommandOutput;
    use remote_merge::service::types::SyncTargetStatus;

    for right in [["a", "b"], ["b", "a"]] {
        let fixture = multi_target(false);
        let b_root = fixture.dirs[2].path().to_path_buf();
        std::fs::remove_dir_all(&b_root).unwrap();

        let result = sync_x_with_delete(&fixture, &right)
            .unwrap_or_else(|error| panic!("{right:?}: {error}"));

        let SyncCommandOutput::Result(output) = &result.output else {
            panic!("{right:?}: expected sync result")
        };
        let target = |label: &str| {
            output
                .targets
                .iter()
                .find(|target| target.target.label == label)
                .unwrap_or_else(|| panic!("{right:?}: {label} missing: {output:?}"))
        };
        let failed: Vec<&str> = target("a").failed.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(failed, ["x.txt"], "{right:?}: {output:?}");
        assert_eq!(target("b").status, SyncTargetStatus::Failed, "{right:?}");
        assert_ne!(result.exit_code, 0, "{right:?}");
    }
}
