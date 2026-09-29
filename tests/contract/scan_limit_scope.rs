//! 走査の上限の超過の案内（docs/ir/scan/limits.md の REQ-scan-008）と、指定したパスによる走査の
//! 範囲の選び方（REQ-scan-009、判定表 TBL-scan-001）の契約テスト。
//!
//! どの範囲を走査したかは、上限を超える範囲を走査したときだけ上限の超過のエラーになることで
//! 見分ける。左右の root_dir に、ファイル 10 件の "big/"、3 件ずつの "small/" と "small2/"、
//! ファイル 1 件ずつの "one00/" から "one20/" の 21 個、直下のファイル "top.txt" を置き、上限を 5
//! にする。全体は 5 を大きく超え、"small/" だけ・"small2/" だけ・"oneNN/" の一つだけは 5 を大きく
//! 下回り、"small/" と "small2/" を合わせると 5 を超える。ディレクトリを数えるかどうかで結果が
//! 変わらない件数にし、ちょうど上限の件数には触れない。
//! merge と sync は必ず --dry-run で実行し、書き込みを起こさない。

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use remote_merge::cli::diff::{execute_diff, DiffArgs};
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::status::{execute_status, StatusArgs};
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::RuntimeTargets;
use tempfile::TempDir;

const LIMIT: usize = 5;

struct ScopeFixture {
    _config_dir: TempDir,
    _left: TempDir,
    _right: TempDir,
    _backup: TempDir,
    config: AppConfig,
    targets: RuntimeTargets,
}

/// 互いに異なる最上位のディレクトリの下のファイルのパス "one00/f.txt" から `count` 個
fn one_file_paths(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("one{index:02}/f.txt"))
        .collect()
}

/// `root` に "big/"・"small/"・"small2/"・"one00/" から "one20/"・"top.txt" を中身 `content` で置く
fn place_tree(root: &Path, content: &str) {
    let mut files: Vec<String> = (0..10).map(|index| format!("big/{index}.txt")).collect();
    for dir in ["small", "small2"] {
        files.extend(["a.txt", "b.txt", "c.txt"].map(|name| format!("{dir}/{name}")));
    }
    files.extend(one_file_paths(21));
    files.push("top.txt".into());
    for file in files {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}

fn scope_fixture() -> ScopeFixture {
    let config_dir = TempDir::new().unwrap();
    let left = TempDir::new().unwrap();
    let right = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    place_tree(left.path(), "left\n");
    place_tree(right.path(), "right\n");
    let config_path = config_dir.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n",
        left.path().display().to_string(), right.path().display().to_string()
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", right.path())
        .with_backup_store(Some(backup.path().to_path_buf()))
        .with_startup_directory(config_dir.path().to_path_buf());
    ScopeFixture {
        _config_dir: config_dir,
        _left: left,
        _right: right,
        _backup: backup,
        config,
        targets,
    }
}

fn status(fixture: ScopeFixture) -> anyhow::Result<()> {
    let args = StatusArgs {
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        format: "json".into(),
        summary: false,
        all: true,
        checksum: false,
        verbose: 0,
        max_entries: Some(LIMIT),
    };
    execute_status(args, fixture.config, fixture.targets).map(|_| ())
}

fn diff_without_paths(fixture: ScopeFixture) -> anyhow::Result<()> {
    let args = DiffArgs {
        paths: vec![],
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        format: "json".into(),
        max_lines: None,
        max_files: 0,
        force: false,
        follow_external_links: false,
        max_entries: Some(LIMIT),
    };
    execute_diff(args, fixture.config, fixture.targets).map(|_| ())
}

fn owned(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

/// merge を --dry-run で実行し、merged に載ったパスを返す。エラーにならなければ終了コードは 0
fn merge_dry_run(paths: &[&str], delete: bool) -> anyhow::Result<BTreeSet<String>> {
    let fixture = scope_fixture();
    let args = MergeArgs {
        paths: owned(paths),
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: true,
        force: false,
        delete,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: Some(LIMIT),
        hunks: None,
    };
    let result = execute_merge(args, fixture.config, fixture.targets)?;
    assert_eq!(result.exit_code, 0);
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected a merge plan")
    };
    assert!(output.failed.is_empty(), "{:?}", output.failed);
    Ok(output.merged.into_iter().map(|file| file.path).collect())
}

/// sync を --dry-run で実行し、merged に載ったパスを返す
///
/// sync は書き込み先の走査の失敗（接続の失敗を含む）を関数のエラーにせず結果の中の失敗として
/// 返すため、失敗がなく終了コードが 0 であることもここで確かめる。
fn sync_dry_run(paths: &[&str], delete: bool) -> anyhow::Result<BTreeSet<String>> {
    let fixture = scope_fixture();
    let args = SyncArgs {
        paths: owned(paths),
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: true,
        force: false,
        delete,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: Some(LIMIT),
    };
    let result = execute_sync(args, fixture.config, fixture.targets)?;
    assert_eq!(result.exit_code, 0);
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected a sync plan")
    };
    assert_eq!(output.targets.len(), 1, "{output:?}");
    assert!(output.targets[0].failed.is_empty(), "{output:?}");
    Ok(output.targets[0]
        .merged
        .iter()
        .map(|file| file.path.clone())
        .collect())
}

fn assert_truncated<T: std::fmt::Debug>(result: anyhow::Result<T>, case: &str) {
    let error = result.expect_err(&format!("{case}: the scan must exceed the limit"));
    assert!(
        format!("{error:#}").contains("Tree scan truncated"),
        "{case}: {error:#}"
    );
}

fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

/// 判定表の行ごとに、merge と sync のそれぞれで同じ場合を確かめる
type Run = fn(&[&str], bool) -> anyhow::Result<BTreeSet<String>>;
const COMMANDS: [(&str, Run); 2] = [("merge", merge_dry_run), ("sync", sync_dry_run)];

// @kotowari[REQ-scan-008]
#[test]
fn every_command_reports_the_scan_limit_with_three_ways_out() {
    let merge = merge_dry_run(&["."], false).map(|_| ());
    let sync = sync_dry_run(&["."], false).map(|_| ());
    for (command, result) in [
        ("status", status(scope_fixture())),
        ("diff", diff_without_paths(scope_fixture())),
        ("merge", merge),
        ("sync", sync),
    ] {
        let error = result.expect_err(&format!("{command}: the scan must exceed the limit"));
        let message = format!("{error:#}");
        assert!(
            message.starts_with(&format!("Tree scan truncated at {LIMIT} entries.")),
            "{command}: {message}"
        );
        for hint in ["--max-entries", "max_scan_entries", "specify file paths"] {
            assert!(message.contains(hint), "{command}: {hint}: {message}");
        }
    }
}

// @kotowari[REQ-scan-009]
#[test]
fn status_and_diff_without_paths_scan_the_whole_root_dir() {
    assert_truncated(status(scope_fixture()), "status");
    assert_truncated(diff_without_paths(scope_fixture()), "diff");
}

// @kotowari[REQ-scan-009]
#[test]
fn a_root_marker_or_an_empty_path_scans_the_whole_root_dir() {
    for (command, run) in COMMANDS {
        for path in [".", "./", ""] {
            assert_truncated(run(&[path], false), &format!("{command} {path:?}"));
        }
    }
}

// @kotowari[REQ-scan-009]
#[test]
fn a_path_with_glob_characters_scans_the_whole_root_dir() {
    for (command, run) in COMMANDS {
        assert_truncated(run(&["small/*.txt"], false), command);
    }
}

// @kotowari[REQ-scan-009]
#[test]
fn twenty_one_paths_scan_the_whole_root_dir_but_twenty_do_not() {
    // 互いに異なる最上位のディレクトリの下のファイルのため、親ディレクトリごとの走査は 1 件ずつで
    // 上限を大きく下回り、エラーになるかどうかはパスの個数だけで決まる
    let twenty_one = one_file_paths(21);
    let twenty_one: Vec<&str> = twenty_one.iter().map(String::as_str).collect();
    let twenty = &twenty_one[..20];
    for (command, run) in COMMANDS {
        assert_truncated(run(&twenty_one, false), command);
        assert_eq!(run(twenty, false).unwrap(), set(twenty), "{command}");
    }
}

// @kotowari[REQ-scan-009]
#[test]
fn mixing_directory_and_file_paths_scans_the_whole_root_dir() {
    for (command, run) in COMMANDS {
        assert_truncated(run(&["small/", "small/a.txt"], false), command);
    }
}

// @kotowari[REQ-scan-009]
#[test]
fn delete_scans_the_whole_root_dir() {
    for (command, run) in COMMANDS {
        assert_truncated(run(&["small/"], true), command);
    }
}

// @kotowari[REQ-scan-009]
#[test]
fn directory_paths_scan_only_below_each_directory_with_the_limit_per_directory() {
    for (command, run) in COMMANDS {
        assert_eq!(
            run(&["small/"], false).unwrap(),
            set(&["small/a.txt", "small/b.txt", "small/c.txt"]),
            "{command}"
        );
        // 二つを合わせると上限を超えるが、上限はディレクトリごとの走査に当てる
        assert_eq!(
            run(&["small/", "small2/"], false).unwrap(),
            set(&[
                "small/a.txt",
                "small/b.txt",
                "small/c.txt",
                "small2/a.txt",
                "small2/b.txt",
                "small2/c.txt",
            ]),
            "{command}"
        );
    }
}

// @kotowari[REQ-scan-009]
#[test]
fn file_paths_scan_only_their_parent_directories_unless_one_is_directly_under_the_root_dir() {
    for (command, run) in COMMANDS {
        assert_eq!(
            run(&["small/a.txt"], false).unwrap(),
            set(&["small/a.txt"]),
            "{command}"
        );
        assert_truncated(run(&["top.txt"], false), command);
    }
}
