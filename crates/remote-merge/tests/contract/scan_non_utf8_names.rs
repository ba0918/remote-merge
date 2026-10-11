#![cfg(unix)]
//! UTF-8 として読めない名前のファイルを、ツリーの走査が置換文字に置き換えた名前で一覧に載せず、
//! 読めない名前として報告すること（docs/ir/scan/listing.md の REQ-scan-012）の契約テスト。
//!
//! status は、実行ファイルを試験 SSH サーバに対して `status --left local --right develop` で起動し
//! （組み方は scan_listing_cli と同じ）、ローカル・SSH・エージェントの経路で確かめる。読めない名前の
//! ファイルは片側にだけ置き、確かめる経路の側に置く。
//! diff・merge・sync はローカルの経路で、関数を呼んで確かめる（組み方は filters と同じ）。

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Output;

use remote_merge::cli::diff::{execute_diff, DiffArgs};
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::RuntimeTargets;
use serde_json::Value;
use tempfile::TempDir;

use super::common::{gen_config, place_files, TestDirs};
use super::scan_listing_cli::{status_over_ssh, AgentFixture};

/// UTF-8 として読めない名前（"bad" と 0xff と ".txt"）
const BAD_NAME: &[u8] = b"bad\xff.txt";
const READABLE: [(&str, &str); 1] = [("ok.txt", "same\n")];

fn place_bad_name(root: &Path) {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join(OsStr::from_bytes(BAD_NAME)), "bad\n").unwrap();
}

/// JSON の `key` の配列の項目の "path"
fn paths_in(json: &Value, key: &str) -> Vec<String> {
    json[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["path"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// 置換文字の入った名前を一覧に載せず、読めない名前を errors に出し、ほかのファイルは比べて
/// 終了コード 2 で終わる
fn assert_reported_as_unreadable_name(output: &Output) {
    let json: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"));
    let files = paths_in(&json, "files");
    assert!(
        files.iter().all(|path| !path.contains('\u{FFFD}')),
        "{json}"
    );
    assert!(files.contains(&"ok.txt".to_string()), "{json}");
    assert!(!paths_in(&json, "errors").is_empty(), "{json}");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

fn status_over_ssh_with_bad_name(on_local: bool) -> Output {
    let mut dirs = TestDirs::new_2way(&READABLE, &READABLE);
    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    place_bad_name(if on_local { &local_root } else { &remote_root });
    let config = gen_config(&local_root, &remote_root, None, dirs.server_port());
    status_over_ssh(&mut dirs, &config, &local_root, &[])
}

// @kotowari[REQ-scan-012, EX-scan-014]
#[test]
fn ex_scan_014_status_reports_a_local_file_whose_name_is_not_utf8() {
    assert_reported_as_unreadable_name(&status_over_ssh_with_bad_name(true));
}

// @kotowari[REQ-scan-012]
#[test]
fn req_scan_012_status_reports_a_remote_file_whose_name_is_not_utf8_over_ssh() {
    assert_reported_as_unreadable_name(&status_over_ssh_with_bad_name(false));
}

// @kotowari[REQ-scan-012]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_012_status_reports_a_remote_file_whose_name_is_not_utf8_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_files(&local_root, &READABLE);
    place_files(&remote_root, &READABLE);
    place_bad_name(&remote_root);

    let output = fixture.status_via_agent(&local_root, &remote_root, &[]);

    assert_reported_as_unreadable_name(&output);
}

// ─── diff・merge・sync（ローカルの経路） ─────────────────────────────

struct Fixture {
    _source: TempDir,
    destination: TempDir,
    _config_dir: TempDir,
    config: AppConfig,
    targets: RuntimeTargets,
}

/// 左右に "ok.txt" を中身を違えて置き、左にだけ読めない名前のファイルを置く
fn fixture() -> Fixture {
    let source = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();
    place_files(source.path(), &[("ok.txt", "incoming\n")]);
    place_files(destination.path(), &[("ok.txt", "old\n")]);
    place_bad_name(source.path());
    let config_path = config_dir.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n",
        source.path().display().to_string(), destination.path().display().to_string(),
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_startup_directory(config_dir.path().to_path_buf());
    Fixture {
        _source: source,
        destination,
        _config_dir: config_dir,
        config,
        targets,
    }
}

/// 書き込み先の root_dir の名前。読めない名前のファイルを書き込んでいない
fn destination_names(fixture: &Fixture) -> Vec<Vec<u8>> {
    fs::read_dir(fixture.destination.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().as_bytes().to_vec())
        .collect()
}

// @kotowari[REQ-scan-012]
#[test]
fn req_scan_012_diff_reports_a_file_whose_name_is_not_utf8_in_errors() {
    let fixture = fixture();
    let args = DiffArgs {
        paths: vec![],
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        format: "json".into(),
        max_lines: None,
        max_files: 100,
        force: false,
        follow_external_links: false,
        max_entries: None,
    };

    let (output, code) = execute_diff(args, fixture.config, fixture.targets).unwrap();

    let paths: Vec<&str> = output.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, vec!["ok.txt"], "{output:?}");
    assert!(!output.errors.is_empty(), "{output:?}");
    assert!(
        output
            .errors
            .iter()
            .all(|error| !error.path.contains('\u{FFFD}')),
        "{output:?}"
    );
    assert_eq!(code, 2);
}

// @kotowari[REQ-scan-012]
#[test]
fn req_scan_012_merge_reports_a_file_whose_name_is_not_utf8_as_failed() {
    let fixture = fixture();
    let args = MergeArgs {
        paths: vec![".".into()],
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
    };

    let result = execute_merge(args, fixture.config.clone(), fixture.targets.clone()).unwrap();

    let MergeCommandOutput::Files(output) = &result.output else {
        panic!("expected merge output")
    };
    assert!(
        output.merged.iter().any(|file| file.path == "ok.txt"),
        "{output:?}"
    );
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert!(!output.failed[0].path.contains('\u{FFFD}'), "{output:?}");
    assert_eq!(result.exit_code, 2);
    assert_eq!(destination_names(&fixture), vec![b"ok.txt".to_vec()]);
}

// @kotowari[REQ-scan-012]
#[test]
fn req_scan_012_sync_reports_a_file_whose_name_is_not_utf8_as_failed() {
    let fixture = fixture();
    let args = SyncArgs {
        paths: vec![".".into()],
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    };

    let result = execute_sync(args, fixture.config.clone(), fixture.targets.clone()).unwrap();

    let SyncCommandOutput::Result(output) = &result.output else {
        panic!("expected sync result")
    };
    let target = &output.targets[0];
    assert!(
        target.merged.iter().any(|file| file.path == "ok.txt"),
        "{output:?}"
    );
    assert_eq!(target.failed.len(), 1, "{output:?}");
    assert!(!target.failed[0].path.contains('\u{FFFD}'), "{output:?}");
    assert_eq!(result.exit_code, 2);
    assert_eq!(destination_names(&fixture), vec![b"ok.txt".to_vec()]);
}
