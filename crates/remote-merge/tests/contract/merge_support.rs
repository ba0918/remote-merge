//! merge の契約テストが共有する準備と呼び出し。
//!
//! 書き込み先 "develop" と参照先 "staging" はリモートの設定を持つ。
//! 関数呼び出し（merge と sync）では実体を一時ディレクトリに差し替え、バックアップの集約先も一時ディレクトリにする。
//! 実行ファイルの呼び出しは接続より前に止まる指定だけに使い、環境変数を消して HOME と XDG の
//! ディレクトリを一時ディレクトリにし、利用者の設定を読ませない。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::SystemTime;

use remote_merge::cli::diff::execute_diff;
use remote_merge::cli::merge::{MergeArgs, MergeCommandOutput, execute_merge};
use remote_merge::cli::sync::{SyncArgs, SyncCommandOutput, execute_sync};
use remote_merge::config::{AppConfig, load_config_from_paths};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::output::{format_json, format_merge_text};
use tempfile::TempDir;

use super::args_support::diff_args;

const SERVERS: [&str; 2] = ["develop", "staging"];

pub struct Fixture {
    home: TempDir,
    local: TempDir,
    servers: Vec<(&'static str, TempDir)>,
    backup: TempDir,
    config_path: PathBuf,
}

/// 書き込み先の祖先が通常ファイルのときのパスの確認の誤り
pub const NOT_A_DIRECTORY: &str = "cannot resolve path: not a directory";
/// 実パスが長すぎるときのパスの確認の誤り
pub const PATH_TOO_LONG: &str = "cannot resolve path: path too long";
/// 書き込み先に置く、祖先にあたる通常ファイルの中身
pub const DESTINATION_FILE: &str = "a regular file\n";

/// バックアップを無効にした構成
pub fn fixture() -> Fixture {
    build(false)
}

/// `[filter]` に `filter` を書いた構成
pub fn fixture_with_filter(filter: &str) -> Fixture {
    let fixture = build(false);
    let mut text = fs::read_to_string(&fixture.config_path).unwrap();
    text.push_str(&format!("[filter]\n{filter}\n"));
    fs::write(&fixture.config_path, text).unwrap();
    fixture
}

/// バックアップを有効にし、集約先を一時ディレクトリに差し替えた構成
pub fn fixture_with_backup() -> Fixture {
    build(true)
}

fn build(backup_enabled: bool) -> Fixture {
    let home = TempDir::new().unwrap();
    let local = TempDir::new().unwrap();
    let servers: Vec<(&'static str, TempDir)> = SERVERS
        .iter()
        .map(|name| (*name, TempDir::new().unwrap()))
        .collect();
    let mut text = format!(
        "[local]\nroot_dir = {:?}\n",
        local.path().display().to_string()
    );
    for (name, root) in &servers {
        text.push_str(&format!(
            "[servers.{name}]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n",
            root.path().display().to_string()
        ));
    }
    text.push_str(&format!("[backup]\nenabled = {backup_enabled}\n"));
    let config_path = home.path().join("config.toml");
    fs::write(&config_path, text).unwrap();
    Fixture {
        home,
        local,
        servers,
        backup: TempDir::new().unwrap(),
        config_path,
    }
}

impl Fixture {
    /// "local" かサーバ名のディレクトリ
    pub fn root(&self, side: &str) -> &Path {
        if side == "local" {
            return self.local.path();
        }
        self.servers
            .iter()
            .find(|(name, _)| *name == side)
            .map(|(_, root)| root.path())
            .unwrap_or_else(|| panic!("unknown side {side}"))
    }

    pub fn write(&self, side: &str, path: &str, content: &str) {
        fs::write(self.root(side).join(path), content).unwrap();
    }

    /// UTF-8 として読めない中身も置けるよう、バイト列のまま書く
    pub fn write_bytes(&self, side: &str, path: &str, content: &[u8]) {
        fs::write(self.root(side).join(path), content).unwrap();
    }

    pub fn read_bytes(&self, side: &str, path: &str) -> Vec<u8> {
        fs::read(self.root(side).join(path)).unwrap()
    }

    pub fn read(&self, side: &str, path: &str) -> String {
        fs::read_to_string(self.root(side).join(path)).unwrap()
    }

    /// `side` の `path` に `target` を指す symlink を作る
    pub fn symlink(&self, side: &str, path: &str, target: &str) {
        std::os::unix::fs::symlink(target, self.root(side).join(path)).unwrap();
    }

    /// 配下のディレクトリを作る（`write` は親ディレクトリを作らないため先に呼ぶ）
    pub fn create_dir(&self, side: &str, path: &str) {
        fs::create_dir_all(self.root(side).join(path)).unwrap();
    }

    pub fn metadata(&self, side: &str, path: &str) -> fs::Metadata {
        fs::metadata(self.root(side).join(path)).unwrap()
    }

    pub fn modified(&self, side: &str, path: &str) -> SystemTime {
        self.metadata(side, path).modified().unwrap()
    }

    pub fn set_modified(&self, side: &str, path: &str, time: SystemTime) {
        fs::OpenOptions::new()
            .write(true)
            .open(self.root(side).join(path))
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    /// 権限を 0o200 に落として読めなくし、読めなくなったかを返す（root で動くと読めてしまう。
    /// [`restrict_permissions`](super::common::restrict_permissions)）
    #[must_use]
    pub fn make_unreadable(&self, side: &str, path: &str) -> bool {
        super::common::restrict_permissions(&self.root(side).join(path), 0o200)
    }

    pub(super) fn append_config(&self, text: &str) {
        let mut config = fs::read_to_string(&self.config_path).unwrap();
        config.push_str(text);
        fs::write(&self.config_path, config).unwrap();
    }

    pub(super) fn config(&self) -> AppConfig {
        load_config_from_paths(Some(&self.config_path), None).unwrap()
    }

    pub(super) fn runtime_targets(&self) -> RuntimeTargets {
        self.servers
            .iter()
            .fold(RuntimeTargets::production(), |targets, (name, root)| {
                targets.with_local(*name, root.path())
            })
            .with_backup_store(Some(self.backup.path().to_path_buf()))
            .with_startup_directory(self.home.path().to_path_buf())
    }

    /// 関数呼び出しで merge し、結果の JSON と終了コードを返す
    pub fn merge_json(&self, args: MergeArgs) -> (serde_json::Value, i32) {
        let result = execute_merge(args, self.config(), self.runtime_targets()).unwrap();
        let MergeCommandOutput::Files(output) = result.output else {
            panic!("expected per-file merge output")
        };
        let json = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
        (json, result.exit_code)
    }

    /// 関数呼び出しで一度 merge し、同じ結果を `format_json` の JSON と
    /// --format text と同じ `format_merge_text` の文字列にして返す
    pub fn merge_json_and_text(&self, args: MergeArgs) -> (serde_json::Value, String) {
        let result = execute_merge(args, self.config(), self.runtime_targets()).unwrap();
        let MergeCommandOutput::Files(output) = result.output else {
            panic!("expected per-file merge output")
        };
        let json = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
        (json, format_merge_text(&output))
    }

    /// 関数呼び出しで merge し、`execute_merge` が返したエラーを返す（成功したら落ちる）
    pub fn merge_error(&self, args: MergeArgs) -> anyhow::Error {
        match execute_merge(args, self.config(), self.runtime_targets()) {
            Ok(result) => panic!("expected merge to fail, got exit code {}", result.exit_code),
            Err(error) => error,
        }
    }

    /// 関数呼び出しで local と develop の `path` を diff し、--format json と同じ整形の JSON を返す
    pub fn diff_json(&self, path: &str) -> serde_json::Value {
        let args = diff_args(&[path]);
        let (output, _) = execute_diff(args, self.config(), self.runtime_targets()).unwrap();
        serde_json::from_str(&format_json(&output).unwrap()).unwrap()
    }

    /// 関数呼び出しで sync し、結果の JSON と終了コードを返す
    pub fn sync_json(&self, args: SyncArgs) -> (serde_json::Value, i32) {
        let result = execute_sync(args, self.config(), self.runtime_targets()).unwrap();
        let SyncCommandOutput::Result(output) = result.output else {
            panic!("expected sync result")
        };
        let json = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
        (json, result.exit_code)
    }

    /// 実行ファイルの merge を `args` で起動する
    pub fn run_cli(&self, args: &[&str]) -> Output {
        super::common::isolated_command(&self.home.path().join("home"))
            .arg("--config")
            .arg(&self.config_path)
            .arg("merge")
            .args(args)
            .output()
            .unwrap()
    }
}

/// `base` の下に、実パスがちょうど `length` バイトになるまでディレクトリを重ねて作る
pub fn deep_directory(base: &std::path::Path, length: usize) -> std::path::PathBuf {
    let mut path = base.to_path_buf();
    while path.as_os_str().len() < length {
        let room = length - path.as_os_str().len() - 1;
        path.push("d".repeat(room.min(200)));
    }
    std::fs::create_dir_all(&path).unwrap();
    assert_eq!(path.as_os_str().len(), length);
    path
}
