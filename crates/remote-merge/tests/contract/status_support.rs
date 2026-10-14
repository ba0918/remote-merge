//! status の契約テストが共有する準備と呼び出し。
//!
//! サーバ "develop" と "staging" はリモートの設定を持つが、実体は一時ディレクトリに差し替える。

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use remote_merge::cli::status::{StatusArgs, StatusCommandResult, execute_status};
use remote_merge::config::{AppConfig, load_config_from_paths};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::types::FileStatusKind;
use tempfile::TempDir;

pub struct Fixture {
    pub local: TempDir,
    pub develop: TempDir,
    pub staging: TempDir,
    _config_dir: TempDir,
    pub config: AppConfig,
}

pub fn fixture() -> Fixture {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();
    let config_path = config_dir.path().join("config.toml");
    let server = |name: &str, root: &TempDir| {
        format!(
            "[servers.{name}]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n",
            root.path().display().to_string()
        )
    };
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = {:?}\n{}{}[backup]\nenabled = false\n",
            local.path().display().to_string(),
            server("develop", &develop),
            server("staging", &staging),
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    Fixture {
        local,
        develop,
        staging,
        _config_dir: config_dir,
        config,
    }
}

impl Fixture {
    pub fn targets(&self) -> RuntimeTargets {
        RuntimeTargets::production()
            .with_local("develop", self.develop.path())
            .with_local("staging", self.staging.path())
            .with_startup_directory(self._config_dir.path().to_path_buf())
    }

    pub fn run(&self, args: StatusArgs) -> anyhow::Result<StatusCommandResult> {
        execute_status(args, self.config.clone(), self.targets())
    }

    /// 全件を表示させて、パスごとの判定を返す。
    pub fn statuses(&self, left: &str, right: &str) -> HashMap<String, FileStatusKind> {
        let mut args = args(Some(left), Some(right));
        args.all = true;
        let output = self.run(args).unwrap().output;
        output
            .files
            .unwrap()
            .into_iter()
            .map(|file| (file.path, file.status))
            .collect()
    }
}

pub fn args(left: Option<&str>, right: Option<&str>) -> StatusArgs {
    StatusArgs {
        left: left.map(Into::into),
        right: right.map(Into::into),
        ref_server: None,
        format: "json".into(),
        summary: false,
        all: false,
        checksum: false,
        verbose: 0,
        max_entries: None,
    }
}

/// 中身を書き、更新時刻を UNIX 時刻 `mtime` 秒と [`SUBSECOND`] に揃える。
pub fn write_at(root: &Path, path: &str, content: &[u8], mtime: u64) {
    let full = root.join(path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full, content).unwrap();
    fs::File::options()
        .write(true)
        .open(&full)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::new(mtime, SUBSECOND))
        .unwrap();
}

/// `write_at` が更新時刻に足す秒の小数。ナノ秒の部分が 0 の時刻は秒の精度しかないと読む
/// （docs/ir/compare/judgement.md の TBL-compare-001）ので、秒より細かい精度を持つ時刻にする
const SUBSECOND: u32 = 500_000_000;

/// 左に通常ファイル、右に同じ名前のディレクトリを、大きさと更新時刻（UNIX 時刻 `mtime` 秒）を
/// そろえて置く。ディレクトリの大きさはファイルシステムで違うので、置いたディレクトリに合わせる
pub fn file_and_directory_with_equal_metadata(
    file_root: &Path,
    dir_root: &Path,
    path: &str,
    mtime: u64,
) {
    write_at(dir_root, &format!("{path}/inner.txt"), b"inner\n", mtime);
    let dir = dir_root.join(path);
    fs::File::open(&dir)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(mtime))
        .unwrap();
    let size = fs::metadata(&dir).unwrap().len();
    write_at(file_root, path, &vec![b'x'; size as usize], mtime);
}

pub fn symlink(root: &Path, path: &str, target: &str) {
    std::os::unix::fs::symlink(target, root.join(path)).unwrap();
}
