//! CLI diff の差分の出し方の契約テスト（diff_selection・diff_format・diff_file_kinds）が共有する
//! 組み方と読み方。
//!
//! 組み方は diff_max_files と同じで、左右の root_dir をローカルの一時ディレクトリにし、サーバ
//! "develop" を `RuntimeTargets::with_local` でローカルに差し替えて `execute_diff` を呼ぶ。
//! JSON は `format_json` を通した文字列を読み、テキストは `format_multi_diff_text` の文字列で読む。

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use remote_merge::cli::diff::{execute_diff, DiffArgs};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::output::format_json;
use remote_merge::service::types::MultiDiffOutput;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

pub(super) struct DiffFixture {
    _config_dir: TempDir,
    pub(super) left: TempDir,
    pub(super) right: TempDir,
    _backup: TempDir,
    config: AppConfig,
    targets: RuntimeTargets,
}

impl DiffFixture {
    /// 左右の root_dir に `left_files`・`right_files` を置く。設定に [filter] は書かない
    pub(super) fn new(left_files: &[(&str, &[u8])], right_files: &[(&str, &[u8])]) -> Self {
        let config_dir = TempDir::new().unwrap();
        let left = TempDir::new().unwrap();
        let right = TempDir::new().unwrap();
        let backup = TempDir::new().unwrap();
        place(left.path(), left_files);
        place(right.path(), right_files);
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
        Self {
            _config_dir: config_dir,
            left,
            right,
            _backup: backup,
            config,
            targets,
        }
    }

    pub(super) fn try_run(&self, args: DiffArgs) -> anyhow::Result<(MultiDiffOutput, i32)> {
        execute_diff(args, self.config.clone(), self.targets.clone())
    }

    pub(super) fn run(&self, args: DiffArgs) -> (MultiDiffOutput, i32) {
        self.try_run(args).expect("diff failed")
    }

    /// 既定の引数で `paths` を比べ、出力と終了コードを返す
    pub(super) fn diff(&self, paths: &[&str]) -> (MultiDiffOutput, i32) {
        self.run(args(paths))
    }
}

pub(super) fn place(root: &Path, files: &[(&str, &[u8])]) {
    for (path, content) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}

/// 既定の引数（--max-lines なし、--max-files 100、--force なし）
pub(super) fn args(paths: &[&str]) -> DiffArgs {
    DiffArgs {
        paths: paths.iter().map(|path| path.to_string()).collect(),
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        format: "json".into(),
        max_lines: None,
        max_files: 100,
        force: false,
        follow_external_links: false,
        max_entries: None,
    }
}

pub(super) fn json(output: &MultiDiffOutput) -> Value {
    serde_json::from_str(&format_json(output).unwrap()).unwrap()
}

pub(super) fn paths_of(output: &MultiDiffOutput) -> BTreeSet<String> {
    output.files.iter().map(|file| file.path.clone()).collect()
}

pub(super) fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

/// JSON の files から `path` の項目を取り出す
pub(super) fn entry<'a>(json: &'a Value, path: &str) -> &'a Value {
    json["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == path)
        .unwrap_or_else(|| panic!("{path} missing: {json}"))
}

/// 項目の全ての hunk の行を (type, content) の並びにする
pub(super) fn lines_of(entry: &Value) -> Vec<(String, String)> {
    entry["hunks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|hunk| hunk["lines"].as_array().unwrap())
        .map(|line| {
            (
                line["type"].as_str().unwrap().to_string(),
                line["content"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

pub(super) fn count_changes(lines: &[(String, String)]) -> usize {
    lines
        .iter()
        .filter(|(kind, _)| kind == "added" || kind == "removed")
        .count()
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn last_line(text: &str) -> &str {
    text.trim_end().lines().last().unwrap()
}
