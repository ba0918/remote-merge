#![cfg(unix)]
//! 集約先の準備に失敗したら書き込みを始めないこと（docs/ir/backup/failure.md の REQ-backup-045）の
//! 契約テスト。
//!
//! 集約先を置くディレクトリに書き込めなくして集約先を作れなくするか、集約先の中の領域を置く
//! 場所をファイルでふさぐ。root の利用者はアクセス権によらず書き込めるので、書き込めない
//! ディレクトリを作れないときはテストを飛ばす。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput, MergeCommandResult};
use remote_merge::runtime::RuntimeTargets;
use tempfile::TempDir;

use super::backup_support::{config, fixed_now, merge_args};

/// 書き込めなくしたディレクトリ。drop で書き込めるように戻し、一時ディレクトリを消せるようにする
struct ReadOnly(PathBuf);

impl ReadOnly {
    /// `dir` を書き込めなくする。この利用者でも書き込めてしまうなら None
    fn new(dir: &Path) -> Option<Self> {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o500)).unwrap();
        let read_only = Self(dir.to_path_buf());
        if fs::write(dir.join(".probe"), "").is_ok() {
            let _ = fs::remove_file(dir.join(".probe"));
            eprintln!("skipped: the directory is writable without permission (running as root)");
            return None;
        }
        Some(read_only)
    }
}

impl Drop for ReadOnly {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
    }
}

/// 書き込み先 develop を一時ディレクトリに差し替え、集約先を `store` にした差し替え
fn store_targets(develop: &TempDir, store: PathBuf) -> RuntimeTargets {
    RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(Some(store))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(fixed_now())
}

/// 左右に "a.txt" と "b.txt" を中身を違えて置き、二つを --force でマージする引数
fn two_files(local: &TempDir, develop: &TempDir) -> MergeArgs {
    for name in ["a.txt", "b.txt"] {
        fs::write(local.path().join(name), "new\n").unwrap();
        fs::write(develop.path().join(name), "old\n").unwrap();
    }
    let mut args = merge_args("a.txt");
    args.paths = vec!["a.txt".into(), "b.txt".into()];
    args.force = true;
    args
}

/// コマンド全体がエラーで止まり、どのファイルも書き込まれていない
fn assert_stopped_before_writing(result: anyhow::Result<MergeCommandResult>, develop: &TempDir) {
    if let Ok(result) = result {
        if let MergeCommandOutput::Files(output) = result.output {
            panic!("the merge must stop as a whole: {output:?}");
        }
        panic!("the merge must stop as a whole");
    }
    for name in ["a.txt", "b.txt"] {
        assert_eq!(
            fs::read_to_string(develop.path().join(name)).unwrap(),
            "old\n"
        );
    }
}

// @kotowari[REQ-backup-045, EX-backup-026]
#[test]
fn ex_backup_026_a_store_that_cannot_be_created_stops_the_merge_before_writing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let parent = TempDir::new().unwrap();
    let args = two_files(&local, &develop);
    let Some(_read_only) = ReadOnly::new(parent.path()) else {
        return;
    };

    let result = execute_merge(
        args,
        config(&local, &develop, true),
        store_targets(&develop, parent.path().join("store")),
    );

    assert_stopped_before_writing(result, &develop);
}

// 書き込み先ごとの領域を作れないときも、ファイルごとの失敗にせずにコマンド全体を止める
// @kotowari[REQ-backup-045]
#[test]
fn req_backup_045_an_area_that_cannot_be_created_in_the_store_stops_the_merge_before_writing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let args = two_files(&local, &develop);
    // セッションの領域を置く場所をファイルでふさぐ
    fs::write(store.path().join("sessions"), "occupied").unwrap();

    let result = execute_merge(
        args,
        config(&local, &develop, true),
        store_targets(&develop, store.path().to_path_buf()),
    );

    assert_stopped_before_writing(result, &develop);
}
