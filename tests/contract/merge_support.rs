//! merge の契約テストが共有する準備と呼び出し。
//!
//! 書き込み先 "develop" と参照先 "staging" はリモートの設定を持つ。
//! 関数呼び出し（merge と sync）では実体を一時ディレクトリに差し替え、バックアップの集約先も一時ディレクトリにする。
//! 実行ファイルの呼び出しは接続より前に止まる指定だけに使い、環境変数を消して HOME と XDG の
//! ディレクトリを一時ディレクトリにし、利用者の設定を読ませない。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::SystemTime;

use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::output::{format_json, format_merge_text};
use tempfile::TempDir;

const SERVERS: [&str; 2] = ["develop", "staging"];

pub struct Fixture {
    home: TempDir,
    local: TempDir,
    servers: Vec<(&'static str, TempDir)>,
    backup: TempDir,
    config_path: PathBuf,
}

/// バックアップを無効にした構成
pub fn fixture() -> Fixture {
    build(false, &[])
}

/// バックアップを有効にし、集約先を一時ディレクトリに差し替えた構成
pub fn fixture_with_backup() -> Fixture {
    build(true, &[])
}

/// バックアップを無効にし、設定の `[filter]` の `sensitive` に `patterns` を書いた構成
pub fn fixture_with_sensitive(patterns: &[&str]) -> Fixture {
    build(false, patterns)
}

fn build(backup_enabled: bool, sensitive: &[&str]) -> Fixture {
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
    if !sensitive.is_empty() {
        text.push_str(&format!("[filter]\nsensitive = {sensitive:?}\n"));
    }
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

    /// 権限を 0o200 に落として読めなくする。
    /// root で実行すると権限を落としても読めてしまうため、開けないことを確かめて前提の崩れを知らせる。
    pub fn make_unreadable(&self, side: &str, path: &str) {
        let full = self.root(side).join(path);
        fs::set_permissions(&full, fs::Permissions::from_mode(0o200)).unwrap();
        assert!(
            fs::File::open(&full).is_err(),
            "test needs an unreadable {side}/{path}"
        );
    }

    fn config(&self) -> AppConfig {
        load_config_from_paths(Some(&self.config_path), None).unwrap()
    }

    fn runtime_targets(&self) -> RuntimeTargets {
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
        Command::new(env!("CARGO_BIN_EXE_remote-merge"))
            .env_clear()
            .env("HOME", self.home.path().join("home"))
            .env("XDG_CONFIG_HOME", self.home.path().join("xdg-config"))
            .env("XDG_DATA_HOME", self.home.path().join("xdg-data"))
            .current_dir(self.home.path())
            .arg("--config")
            .arg(&self.config_path)
            .arg("merge")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

/// local から develop へ `paths` を書き込む引数（--force も --dry-run もない）
pub fn args(paths: &[&str]) -> MergeArgs {
    MergeArgs {
        paths: paths.iter().map(|path| path.to_string()).collect(),
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: false,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
    }
}

/// local から develop へ `paths` を同期する引数。
/// 書き込む予定があると確認のプロンプトで標準入力を読むため --force を付ける。
pub fn sync_args(paths: &[&str]) -> SyncArgs {
    SyncArgs {
        paths: paths.iter().map(|path| path.to_string()).collect(),
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    }
}
