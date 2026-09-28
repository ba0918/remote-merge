//! merge の契約テストが共有する準備と呼び出し。
//!
//! 書き込み先 "develop" と参照先 "staging" はリモートの設定を持つ。
//! 関数呼び出しでは実体を一時ディレクトリに差し替え、バックアップの集約先も一時ディレクトリにする。
//! 実行ファイルの呼び出しは接続より前に止まる指定だけに使い、環境変数を消して HOME と XDG の
//! ディレクトリを一時ディレクトリにし、利用者の設定を読ませない。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

const SERVERS: [&str; 2] = ["develop", "staging"];

pub struct Fixture {
    home: TempDir,
    local: TempDir,
    servers: Vec<(&'static str, TempDir)>,
    config_path: PathBuf,
}

/// バックアップを無効にした構成
pub fn fixture() -> Fixture {
    build(false)
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
