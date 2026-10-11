#![cfg(unix)]
//! 走査の途中に読めないディレクトリがあるとき、どの経路でもそのディレクトリを飛ばした一覧を返さずに
//! 走査全体をエラーにすること（docs/ir/scan/limits.md の REQ-scan-014）の契約テスト。
//!
//! 実行ファイルを試験 SSH サーバに対して `status --left local --right develop` で起動する（組み方は
//! scan_listing_cli と同じ）。読めないディレクトリは片側にだけ置き、もう一方の側の走査が先に
//! 失敗して確かめたい経路を通らずに終わることがないようにする。root の利用者はアクセス権によらず
//! 読めるので、読めないディレクトリを作れないときはテストを飛ばす。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;

use super::common::{gen_config, place_files, TestDirs};
use super::scan_listing_cli::{status_over_ssh, AgentFixture};

/// 読めるファイルと、中にファイルのある読めないディレクトリ "locked"
const WITH_LOCKED: [(&str, &str); 2] = [("ok.txt", "same\n"), ("locked/secret.txt", "secret\n")];
const READABLE: [(&str, &str); 1] = [("ok.txt", "same\n")];

/// 読めなくしたディレクトリ。drop で読めるように戻し、一時ディレクトリを消せるようにする
struct Locked(PathBuf);

impl Locked {
    /// `root` の下の "locked" を読めなくする。この利用者でも読めてしまうなら None
    fn new(root: &Path) -> Option<Self> {
        let path = root.join("locked");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        let locked = Self(path);
        if fs::read_dir(&locked.0).is_ok() {
            eprintln!("skipped: the directory is readable without permission (running as root)");
            return None;
        }
        Some(locked)
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

/// 一覧（"files" を持つ JSON）を返さず、終了コード 2 で終わる
fn assert_scan_stopped(output: &Output) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let listed = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .is_ok_and(|json| json.get("files").is_some());
    assert!(!listed, "a file list must not be returned: {output:?}");
}

fn status_over_ssh_with(
    local_files: &[(&str, &str)],
    remote_files: &[(&str, &str)],
    locked: fn(&TestDirs) -> PathBuf,
) -> Option<Output> {
    let mut dirs = TestDirs::new_2way(local_files, remote_files);
    let _locked = Locked::new(&locked(&dirs))?;
    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let config = gen_config(&local_root, &remote_root, None, dirs.server_port());
    Some(status_over_ssh(&mut dirs, &config, &local_root, &[]))
}

// @kotowari[REQ-scan-014]
#[test]
fn req_scan_014_an_unreadable_directory_stops_the_local_scan() {
    let Some(output) = status_over_ssh_with(&WITH_LOCKED, &READABLE, |dirs| dirs.local_dir.clone())
    else {
        return;
    };

    assert_scan_stopped(&output);
}

// @kotowari[REQ-scan-014]
#[test]
fn req_scan_014_an_unreadable_directory_stops_the_scan_over_ssh() {
    let Some(output) =
        status_over_ssh_with(&READABLE, &WITH_LOCKED, |dirs| dirs.remote_dir.clone())
    else {
        return;
    };

    assert_scan_stopped(&output);
}

// @kotowari[REQ-scan-014, EX-scan-013]
#[tokio::test(flavor = "multi_thread")]
async fn ex_scan_013_an_unreadable_directory_stops_the_scan_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_files(&local_root, &READABLE);
    place_files(&remote_root, &WITH_LOCKED);
    let Some(_locked) = Locked::new(&remote_root) else {
        return;
    };

    let output = fixture.status_via_agent(&local_root, &remote_root, &[]);

    assert_scan_stopped(&output);
}
