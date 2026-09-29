//! diff の --max-files が変更のあるファイルを数えること（docs/ir/cli/diff-output.md の
//! REQ-cli-055）の契約テスト。
//!
//! 左右の root_dir に、中身の違うファイル 3 件の "a/"、1 件の "b/"、直下の "c.txt" を置く。
//! "c.txt" はパスを指定する場合だけ使い、ディレクトリを指定する場合とパスなしの場合は
//! "c.txt" を左右で同じ中身にして変更のあるファイルを "a/" と "b/" の 4 件にする。

use std::fs;
use std::path::Path;

use remote_merge::cli::diff::{execute_diff, DiffArgs};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::output::format_multi_diff_text;
use remote_merge::service::types::MultiDiffOutput;
use tempfile::TempDir;

struct DiffFixture {
    _config_dir: TempDir,
    _left: TempDir,
    _right: TempDir,
    _backup: TempDir,
    config: AppConfig,
    targets: RuntimeTargets,
}

const CHANGED_IN_DIRECTORIES: [&str; 4] = ["a/1.txt", "a/2.txt", "a/3.txt", "b/1.txt"];

/// `root` に "a/" と "b/" のファイルを `content` で、"c.txt" を `top` で置く
fn place_tree(root: &Path, content: &str, top: &str) {
    for file in CHANGED_IN_DIRECTORIES {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    fs::write(root.join("c.txt"), top).unwrap();
}

/// `top_changed` が true なら "c.txt" も左右で中身を変える
fn diff_fixture(top_changed: bool) -> DiffFixture {
    let config_dir = TempDir::new().unwrap();
    let left = TempDir::new().unwrap();
    let right = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    place_tree(left.path(), "left\n", "top\n");
    let right_top = if top_changed { "other top\n" } else { "top\n" };
    place_tree(right.path(), "right\n", right_top);
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
    DiffFixture {
        _config_dir: config_dir,
        _left: left,
        _right: right,
        _backup: backup,
        config,
        targets,
    }
}

fn diff(fixture: DiffFixture, paths: &[&str], max_files: usize) -> MultiDiffOutput {
    let args = DiffArgs {
        paths: paths.iter().map(|path| path.to_string()).collect(),
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        format: "json".into(),
        max_lines: None,
        max_files,
        force: false,
        follow_external_links: false,
        max_entries: None,
    };
    execute_diff(args, fixture.config, fixture.targets)
        .expect("diff failed")
        .0
}

/// 出た項目のうち hunk を持つもの（このテストの変更のあるファイル）のパス
fn changed_paths(output: &MultiDiffOutput) -> Vec<String> {
    output
        .files
        .iter()
        .filter(|file| !file.hunks.is_empty())
        .map(|file| file.path.clone())
        .collect()
}

// @kotowari[REQ-cli-055]
#[test]
fn req_cli_055_directories_are_limited_by_their_changed_files() {
    let output = diff(diff_fixture(false), &["a/", "b/"], 1);

    assert_eq!(changed_paths(&output), vec!["a/1.txt".to_string()]);
    assert!(output.truncated);
    assert_eq!(output.changed_files_total, Some(4));
    assert_eq!(output.summary.files_with_changes, 1);
}

// @kotowari[REQ-cli-055]
#[test]
fn req_cli_055_text_for_directories_counts_the_changed_files_left_out() {
    let output = diff(diff_fixture(false), &["a/", "b/"], 1);

    let text = format_multi_diff_text(&output);

    assert!(
        text.contains("... and 3 more files (truncated, use --max-files 0 for all)"),
        "{text}"
    );
    assert!(text.contains("1 file(s) with changes"), "{text}");
}

// @kotowari[REQ-cli-055]
#[test]
fn req_cli_055_without_paths_the_same_changed_files_are_counted() {
    let output = diff(diff_fixture(false), &[], 1);

    assert_eq!(changed_paths(&output), vec!["a/1.txt".to_string()]);
    assert!(output.truncated);
    assert_eq!(output.changed_files_total, Some(4));
    assert_eq!(output.summary.files_with_changes, 1);
    let text = format_multi_diff_text(&output);
    assert!(text.contains("... and 3 more files"), "{text}");
    assert!(text.contains("1 file(s) with changes"), "{text}");
}

// @kotowari[REQ-cli-055]
#[test]
fn req_cli_055_file_paths_are_limited_to_the_first_changed_files() {
    let output = diff(diff_fixture(true), &["a/1.txt", "b/1.txt", "c.txt"], 2);

    assert_eq!(
        changed_paths(&output),
        vec!["a/1.txt".to_string(), "b/1.txt".to_string()]
    );
    assert!(output.truncated);
    assert_eq!(output.changed_files_total, Some(3));
}

// @kotowari[REQ-cli-055]
#[test]
fn req_cli_055_zero_outputs_every_changed_file() {
    let output = diff(diff_fixture(false), &["a/", "b/"], 0);

    assert_eq!(
        changed_paths(&output),
        CHANGED_IN_DIRECTORIES.map(String::from)
    );
    assert!(!output.truncated);
    assert_eq!(output.changed_files_total, None);
    assert!(!format_multi_diff_text(&output).contains("more files"));
}
