#![cfg(unix)]
//! 設定ファイルの場所・--config・読み込みのエラー・ホームの展開（docs/ir/config/loading.md）の契約テスト。
//!
//! どの設定を読むかはカレントディレクトリと環境変数で決まる。テストのプロセスでそれらを変えると
//! 並列に走る他のテストと干渉するため、実行ファイルを起動して確かめる。
//! どの設定が読まれたかは、その設定の [local] の root_dir に置いたファイルが status の JSON に出るかで見る。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::common::{gen_config, TestDirs};

/// 設定を置く場所とカレントディレクトリをテストごとに選べる、隔離した実行環境。
///
/// HOME・XDG_CONFIG_HOME・XDG_DATA_HOME を一時ディレクトリに向けるため、利用者のマシンの設定は読まない。
/// グローバル設定の場所は "HOME/.config/remote-merge/config.toml" になる（Linux の場合）。
struct Workspace {
    dirs: TestDirs,
    /// テストが書いた設定ファイルと、その [local] の root_dir
    written: Vec<(PathBuf, PathBuf)>,
}

impl Workspace {
    fn new() -> Self {
        let dirs = TestDirs::new_2way(&[], &[]);
        let workspace = Self {
            dirs,
            written: Vec::new(),
        };
        fs::create_dir_all(workspace.home()).unwrap();
        workspace
    }

    fn root(&self) -> &Path {
        self.dirs.temp.path()
    }

    fn home(&self) -> PathBuf {
        self.root().join("home")
    }

    fn global_config_path(&self) -> PathBuf {
        self.home()
            .join(".config")
            .join("remote-merge")
            .join("config.toml")
    }

    /// 一時ディレクトリの下にディレクトリを作って返す
    fn dir(&self, relative: &str) -> PathBuf {
        let path = self.root().join(relative);
        fs::create_dir_all(&path).unwrap();
        path
    }

    /// [local] の root_dir の下にファイルを一つ置いたディレクトリを作る
    fn local_root_with(&self, relative: &str, file: &str) -> PathBuf {
        let root = self.dir(relative);
        fs::write(root.join(file), "only on the local side\n").unwrap();
        root
    }

    /// 試験サーバの develop を持つ設定を書く
    fn write_config(&mut self, path: &Path, local_root: &Path) {
        let content = gen_config(
            local_root,
            &self.dirs.remote_dir,
            None,
            self.dirs.server_port(),
        );
        self.write(path, local_root, &content);
    }

    /// develop を持たず、同じ試験サーバを staging として持つ設定を書く
    fn write_config_without_develop(&mut self, path: &Path, local_root: &Path) {
        let content = gen_config(
            local_root,
            &self.dirs.remote_dir,
            None,
            self.dirs.server_port(),
        )
        .replace("[servers.develop]", "[servers.staging]");
        self.write(path, local_root, &content);
    }

    fn write(&mut self, path: &Path, local_root: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
        self.written
            .push((path.to_path_buf(), local_root.to_path_buf()));
    }

    /// TOML として読めない設定を書く。
    ///
    /// 隔離の確認は設定を TOML として読んで行うため、このファイルにはかけられない。
    /// 製品も同じ toml クレートで読むため、ここで読めないことを確かめたファイルから接続先を得ることはない。
    fn write_unparsable(&self, path: &Path) {
        let content = "this is not valid toml {{{{\n";
        assert!(content.parse::<toml::Value>().is_err());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    /// cwd で `status --left local --right develop` を起動する。config があれば --config に渡す
    fn status(&mut self, cwd: &Path, config: Option<&str>, format: &[&str]) -> Output {
        for (path, local_root) in self.written.clone() {
            self.dirs.assert_isolated_config_at(&path, &local_root);
        }
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
        cmd.env_clear();
        cmd.env("HOME", self.home());
        cmd.env("XDG_CONFIG_HOME", self.home().join(".config"));
        cmd.env("XDG_DATA_HOME", self.root().join("xdg-data"));
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        cmd.current_dir(cwd);
        if let Some(config) = config {
            cmd.args(["--config", config]);
        }
        cmd.args(["status", "--left", "local", "--right", "develop"]);
        cmd.args(format);
        cmd.output().expect("failed to execute status")
    }

    fn status_json(&mut self, cwd: &Path, config: Option<&str>) -> serde_json::Value {
        let output = self.status(cwd, config, &["--format", "json"]);
        // ローカルの側にだけファイルを置くため "left_only" があり、終了コードは 1 になる
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

fn listed_paths(json: &serde_json::Value) -> Vec<String> {
    json["files"]
        .as_array()
        .expect("files missing")
        .iter()
        .map(|file| file["path"].as_str().unwrap().to_owned())
        .collect()
}

/// 標準出力と標準エラーをつないだもの（エラーの出力先は IR が定めていない）
fn combined_output(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_stops_with(output: &Output, text: &str) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let combined = combined_output(output);
    assert!(combined.contains(text), "missing {text:?} in {combined}");
}

/// TOML として読めない設定で止まり、接頭辞に続けて空でない理由を示すこと（理由の文言は toml のもので契約ではない）
fn assert_stops_with_parse_error(output: &Output) {
    let prefix = "Failed to parse config file: ";
    assert_stops_with(output, prefix);
    let combined = combined_output(output);
    let (_, after) = combined.split_once(prefix).unwrap();
    let reason = after.lines().next().unwrap_or("").trim();
    assert!(
        !reason.is_empty(),
        "no reason after {prefix:?} in {combined}"
    );
}

// @kotowari[REQ-config-005]
#[test]
fn project_config_in_the_current_directory_is_read() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let local = workspace.local_root_with("project-local", "from-project.txt");
    workspace.write_config(&cwd.join(".remote-merge.toml"), &local);

    let json = workspace.status_json(&cwd, None);

    assert_eq!(listed_paths(&json), ["from-project.txt"]);
}

// @kotowari[REQ-config-005]
#[test]
fn project_config_in_a_parent_directory_is_not_searched() {
    let mut workspace = Workspace::new();
    let parent = workspace.dir("parent");
    let cwd = workspace.dir("parent/child");
    let local = workspace.local_root_with("parent-local", "from-parent.txt");
    workspace.write_config(&parent.join(".remote-merge.toml"), &local);

    let output = workspace.status(&cwd, None, &[]);

    assert_stops_with(&output, "Config file not found.");
}

// @kotowari[REQ-config-006]
#[test]
fn config_option_is_read_relative_to_the_current_directory_instead_of_the_project_config() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let ignored = workspace.local_root_with("cwd-local", "from-cwd.txt");
    workspace.write_config(&cwd.join(".remote-merge.toml"), &ignored);
    let chosen = workspace.local_root_with("chosen-local", "from-option.txt");
    workspace.write_config(&cwd.join("configs").join("chosen.toml"), &chosen);

    let json = workspace.status_json(&cwd, Some("configs/chosen.toml"));

    assert_eq!(listed_paths(&json), ["from-option.txt"]);
}

// @kotowari[REQ-config-006]
#[cfg(target_os = "linux")]
#[test]
fn config_option_still_merges_servers_from_the_global_config() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let global_local = workspace.local_root_with("global-local", "from-global.txt");
    let global_path = workspace.global_config_path();
    workspace.write_config(&global_path, &global_local);
    let chosen = workspace.local_root_with("chosen-local", "from-option.txt");
    workspace.write_config_without_develop(&cwd.join("chosen.toml"), &chosen);

    let json = workspace.status_json(&cwd, Some("chosen.toml"));

    assert_eq!(listed_paths(&json), ["from-option.txt"]);
}

// @kotowari[REQ-config-007]
#[test]
fn config_option_naming_a_missing_file_stops_with_its_absolute_path() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");

    let output = workspace.status(&cwd, Some("missing.toml"), &[]);

    let absolute = cwd.canonicalize().unwrap().join("missing.toml");
    assert_stops_with(
        &output,
        &format!("Config file not found: {}", absolute.display()),
    );
}

// @kotowari[REQ-config-007]
#[test]
fn config_option_naming_a_directory_stops_with_its_absolute_path() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    workspace.dir("work/configs");

    let output = workspace.status(&cwd, Some("configs"), &[]);

    let absolute = cwd.canonicalize().unwrap().join("configs");
    assert_stops_with(
        &output,
        &format!("Config path is not a regular file: {}", absolute.display()),
    );
}

// @kotowari[REQ-config-008]
#[test]
fn missing_global_and_project_config_stops_naming_the_project_path() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");

    let output = workspace.status(&cwd, None, &[]);

    assert_stops_with(&output, "Config file not found.");
    let project_path = cwd.canonicalize().unwrap().join(".remote-merge.toml");
    assert_stops_with(&output, &project_path.display().to_string());
}

// @kotowari[REQ-config-009]
#[test]
fn unparsable_project_config_stops_with_a_parse_error() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    workspace.write_unparsable(&cwd.join(".remote-merge.toml"));

    let output = workspace.status(&cwd, None, &[]);

    assert_stops_with_parse_error(&output);
}

// @kotowari[REQ-config-009]
#[test]
fn unparsable_config_option_file_stops_with_a_parse_error() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    workspace.write_unparsable(&cwd.join("broken.toml"));

    let output = workspace.status(&cwd, Some("broken.toml"), &[]);

    assert_stops_with_parse_error(&output);
}

// @kotowari[REQ-config-010]
#[test]
fn local_root_starting_with_tilde_is_resolved_under_home() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let under_home = workspace.home().join("project");
    fs::create_dir_all(&under_home).unwrap();
    fs::write(
        under_home.join("under-home.txt"),
        "only on the local side\n",
    )
    .unwrap();
    workspace.write_config(&cwd.join(".remote-merge.toml"), Path::new("~/project"));

    let json = workspace.status_json(&cwd, None);

    assert_eq!(listed_paths(&json), ["under-home.txt"]);
}
