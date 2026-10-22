#![cfg(unix)]
//! CLI diff の比べ方の契約テスト（docs/ir/cli/diff-strategy.md の REQ-cli-080 から REQ-cli-085）が
//! 共有する組み方と読み方。
//!
//! diff は名前で指定したパスを一つずつ比べ、パスを指定しないときは root_dir 全体を走査する。
//! 要件はパスの数（20 個以下か 21 個以上か）と走査方式によらず同じ結果を求めるので、テストは
//! 同じファイルの状態に対して指定の仕方を変えて diff を実行する。21 個にするときは、左右に同じ
//! 中身で置いた詰め物のファイルを足す。
//!
//! 組み方は diff_support と同じく、左右の root_dir をローカルの一時ディレクトリにし、サーバ
//! "develop" を `RuntimeTargets::with_local` でローカルに差し替えて `execute_diff` を呼ぶ。設定には
//! `[filter]` を書ける。

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use remote_merge::cli::diff::{DiffArgs, execute_diff};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::output::{format_json, format_multi_diff_text};
use serde_json::Value;
use tempfile::TempDir;

use super::args_support::diff_args;
use super::common::place_files;

/// 全体を走査する経路に切り替わるパスの数（20 個を超える）
const MANY: usize = 21;

/// 指定するパスの数を 21 個にするための詰め物のファイル
fn padding() -> Vec<String> {
    (0..MANY - 1)
        .map(|index| format!("pad{index:02}.txt"))
        .collect()
}

/// diff の一回の結果。JSON と、同じ出力をテキストにしたものと、終了コード
pub(super) struct Run {
    pub(super) json: Value,
    pub(super) text: String,
    pub(super) code: i32,
}

impl Run {
    pub(super) fn paths(&self) -> Vec<String> {
        self.json["files"]
            .as_array()
            .map(|files| {
                files
                    .iter()
                    .map(|file| file["path"].as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn entry(&self, path: &str) -> &Value {
        self.json["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == path)
            .unwrap_or_else(|| panic!("{path} missing: {}", self.json))
    }
}

/// 左右の root_dir をローカルの一時ディレクトリにし、`[filter]` に `filter` を書いた設定
pub(super) struct Fixture {
    _config_dir: TempDir,
    left: TempDir,
    right: TempDir,
    config_path: std::path::PathBuf,
}

impl Fixture {
    pub(super) fn new(filter: &str) -> Self {
        let config_dir = TempDir::new().unwrap();
        let left = TempDir::new().unwrap();
        let right = TempDir::new().unwrap();
        let config_path = config_dir.path().join("config.toml");
        fs::write(&config_path, format!(
            "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n[filter]\n{filter}\n",
            left.path().display().to_string(), right.path().display().to_string()
        )).unwrap();
        Self {
            _config_dir: config_dir,
            left,
            right,
            config_path,
        }
    }

    /// `paths` に、左右に同じ中身で置いた詰め物のファイルを足して 21 個にする
    pub(super) fn padded(&self, paths: &[&str]) -> Vec<String> {
        let mut all = strings(paths);
        for pad in padding().into_iter().take(MANY - paths.len()) {
            place_files(self.left(), &[(pad.as_str(), "pad\n")]);
            place_files(self.right(), &[(pad.as_str(), "pad\n")]);
            all.push(pad);
        }
        all
    }

    pub(super) fn left(&self) -> &Path {
        self.left.path()
    }

    pub(super) fn right(&self) -> &Path {
        self.right.path()
    }

    pub(super) fn diff(&self, paths: &[String], max_entries: Option<usize>) -> Run {
        let config = load_config_from_paths(Some(&self.config_path), None).unwrap();
        let targets = RuntimeTargets::production()
            .with_local("develop", self.right())
            .with_backup_store(None)
            .with_startup_directory(self._config_dir.path().to_path_buf());
        let args = DiffArgs {
            paths: paths.to_vec(),
            max_entries,
            ..diff_args(&[])
        };
        let (output, code) = execute_diff(args, config, targets).expect("diff failed");
        Run {
            json: serde_json::from_str(&format_json(&output).unwrap()).unwrap(),
            text: format_multi_diff_text(&output),
            code,
        }
    }
}

pub(super) fn strings(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

/// `path` の更新時刻を `time` にそろえる
pub(super) fn set_mtime(path: &Path, time: SystemTime) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}
