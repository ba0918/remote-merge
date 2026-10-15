#![cfg(unix)]
//! 設定ファイルの場所・--config・読み込みのエラー・ホームの展開（docs/ir/config/loading.md）の契約テスト。
//!
//! どの設定を読むかはカレントディレクトリと環境変数で決まる。テストのプロセスでそれらを変えると
//! 並列に走る他のテストと干渉するため、実行ファイルを起動して確かめる。
//! どの設定が読まれたかは、その設定の [local] の root_dir に置いたファイルが status の JSON に出るかで見る。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::common::{TestDirs, gen_config};

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
        self.xdg_config_home()
            .join("remote-merge")
            .join("config.toml")
    }

    /// OS の設定ディレクトリ（XDG_CONFIG_HOME）。ホームの ".config" とは別の場所にして、どちらを
    /// 読んだかをテストが見分けられるようにする（REQ-config-045）
    fn xdg_config_home(&self) -> PathBuf {
        self.root().join("xdg-config")
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
        self.status_with_env(cwd, config, format, true)
    }

    /// [`status`](Self::status) の、XDG_CONFIG_HOME を置かないもの（REQ-config-045 の既定の場所）
    fn status_without_xdg(&mut self, cwd: &Path, config: Option<&str>, format: &[&str]) -> Output {
        self.status_with_env(cwd, config, format, false)
    }

    fn status_with_env(
        &mut self,
        cwd: &Path,
        config: Option<&str>,
        format: &[&str],
        xdg: bool,
    ) -> Output {
        for (path, local_root) in self.written.clone() {
            self.dirs.assert_isolated_config_at(&path, &local_root);
        }
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
        cmd.env_clear();
        cmd.env("HOME", self.home());
        if xdg {
            cmd.env("XDG_CONFIG_HOME", self.xdg_config_home());
        }
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

/// 解釈できない設定のエラーで止まり、接頭辞のすぐ後に読めなかった設定ファイルの絶対パスを示すこと
fn assert_parse_error_names(output: &Output, config: &Path) {
    assert_stops_with_parse_error(output);
    assert_stops_with(
        output,
        &format!("Failed to parse config file: {}: ", config.display()),
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
    // 読まれれば止まる内容にして、下の層として合成される壊れ方とも見分ける
    workspace.write_unparsable(&cwd.join(".remote-merge.toml"));
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

// @kotowari[REQ-config-045]
#[cfg(target_os = "linux")]
#[test]
fn the_global_config_is_read_from_the_os_config_directory() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let local = workspace.local_root_with("global-local", "from-global.txt");
    let global_path = workspace.global_config_path();
    workspace.write_config(&global_path, &local);

    let json = workspace.status_json(&cwd, None);

    assert_eq!(listed_paths(&json), ["from-global.txt"]);
}

// XDG_CONFIG_HOME がないときは、ホームの ".config" の下から読む
// @kotowari[REQ-config-045]
#[cfg(target_os = "linux")]
#[test]
fn the_global_config_is_read_from_the_home_config_directory_without_xdg() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let local = workspace.local_root_with("global-local", "from-home.txt");
    let path = workspace
        .home()
        .join(".config")
        .join("remote-merge")
        .join("config.toml");
    workspace.write_config(&path, &local);

    let output = workspace.status_without_xdg(&cwd, None, &["--format", "json"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(listed_paths(&json), ["from-home.txt"]);
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

    let absolute = cwd.canonicalize().unwrap().join(".remote-merge.toml");
    assert_parse_error_names(&output, &absolute);
}

// @kotowari[REQ-config-009]
#[test]
fn unparsable_config_option_file_stops_with_a_parse_error() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    workspace.write_unparsable(&cwd.join("broken.toml"));

    let output = workspace.status(&cwd, Some("broken.toml"), &[]);

    let absolute = cwd.canonicalize().unwrap().join("broken.toml");
    assert_parse_error_names(&output, &absolute);
}

// @kotowari[REQ-config-009]
#[cfg(target_os = "linux")]
#[test]
fn unparsable_global_config_stops_with_a_parse_error_even_with_a_valid_project_config() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    workspace.write_unparsable(&workspace.global_config_path());
    let local = workspace.local_root_with("project-local", "from-project.txt");
    workspace.write_config(&cwd.join(".remote-merge.toml"), &local);

    let output = workspace.status(&cwd, None, &[]);

    assert_parse_error_names(&output, &workspace.global_config_path());
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

// @kotowari[REQ-config-041]
#[test]
fn an_empty_local_root_stops_with_the_reason() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    fs::write(cwd.join("in-work.txt"), "only in the working directory\n").unwrap();
    workspace.write_config(&cwd.join(".remote-merge.toml"), Path::new(""));

    let output = workspace.status(&cwd, None, &[]);

    assert_stops_with(&output, "root_dir must not be empty");
}

// @kotowari[REQ-config-013]
#[test]
fn a_config_without_a_local_root_dir_stops() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let content = format!(
        "[servers.develop]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"fixture-user\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = \"/srv/app\"\n",
        workspace.dirs.server_port()
    );
    fs::write(cwd.join(".remote-merge.toml"), content).unwrap();

    let output = workspace.status(&cwd, None, &[]);

    // 文言は IR が定めていないので固めない。止まることと、何も出さないことを確かめる
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(!combined_output(&output).is_empty(), "{output:?}");
}

// @kotowari[REQ-config-040]
#[test]
fn a_relative_config_path_whose_real_path_cannot_be_resolved_stops_with_its_absolute_path() {
    // 名前の長い要素を辿ってから ".." で戻る相対の --config。OS は相対のまま読めるが、
    // 実パスに直す途中の絶対パスが PATH_MAX（4096 バイト）を超えるので、基準のディレクトリを
    // 求められない（REQ-config-040 の既知の例外）
    let elements: Vec<String> = (0..6)
        .map(|index| format!("e{index}{}", "x".repeat(208)))
        .collect();
    let relative = format!(
        "{}/{}/proj/.remote-merge.toml",
        elements.join("/"),
        vec![".."; elements.len()].join("/")
    );
    let mut workspace = Workspace::new();
    let wanted_cwd = 4200 - relative.len() - 1;
    let base_len = workspace.root().as_os_str().len();
    if base_len + 300 > wanted_cwd {
        eprintln!(
            "skipped: the temporary directory ({base_len} bytes) is too deep to nest a working directory of {wanted_cwd} bytes"
        );
        return;
    }
    let mut cwd_relative = Vec::new();
    let mut remaining = wanted_cwd - base_len;
    while remaining > 0 {
        let length = remaining.min(201) - 1;
        cwd_relative.push("d".repeat(length.max(1)));
        remaining = remaining.saturating_sub(length.max(1) + 1);
    }
    let cwd = workspace.dir(&cwd_relative.join("/"));
    // 絶対パスでは長すぎて作れないので、作業ディレクトリからの相対で作る
    let created = Command::new("mkdir")
        .arg("-p")
        .arg(elements.join("/"))
        .current_dir(&cwd)
        .status()
        .unwrap();
    assert!(created.success());
    let project = cwd.join("proj");
    fs::create_dir(&project).unwrap();
    workspace.write_config(&project.join(".remote-merge.toml"), Path::new("."));
    assert!(cwd.as_os_str().len() + 1 + relative.len() > 4096);

    let output = workspace.status(&cwd, Some(&relative), &[]);

    assert_stops_with(
        &output,
        &format!(
            "Failed to resolve the directory of config file: {}",
            cwd.join(&relative).display()
        ),
    );
}

// @kotowari[REQ-config-040]
#[test]
fn an_absolute_local_root_loads_from_a_working_directory_deeper_than_path_max() {
    let temp = tempfile::tempdir().unwrap();
    let local_root = temp.path().join("local");
    fs::create_dir(&local_root).unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    // 絶対パスが 4096 バイトを超える作業ディレクトリは絶対パスでは作れも入れもしないので、
    // sh で一段ずつ cd して作り、そこで設定を書いて実行ファイルを起動する
    let level = "d".repeat(200);
    let descend = vec![format!("mkdir {level} && cd -P {level}"); 22].join(" && ");
    let config = format!(
        "[local]\nroot_dir = {:?}\n",
        local_root.display().to_string()
    );
    let script = format!(
        r#"cd "$1" && {descend} && printf '%s' "$2" > .remote-merge.toml && exec "$3" rollback --list --target local --format json"#
    );
    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg("sh")
        .arg(temp.path())
        .arg(&config)
        .arg(env!("CARGO_BIN_EXE_remote-merge"))
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", temp.path().join("xdg-data"))
        .output()
        .unwrap();
    if !Path::new(&temp.path().join(&level)).is_dir() {
        eprintln!("skipped: could not nest a working directory deeper than 4096 bytes: {output:?}");
        return;
    }
    assert!(temp.path().as_os_str().len() + 22 * 201 > 4096);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

// @kotowari[REQ-config-040, EX-config-016]
#[test]
fn ex_config_016_a_relative_local_root_is_resolved_from_the_directory_of_the_given_config() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let project = workspace.local_root_with("proj", "in-project.txt");
    fs::write(cwd.join("in-work.txt"), "only in the working directory\n").unwrap();
    workspace.write_config(&project.join(".remote-merge.toml"), Path::new("."));

    let json = workspace.status_json(&cwd, Some("../proj/.remote-merge.toml"));

    assert_eq!(
        listed_paths(&json),
        [".remote-merge.toml", "in-project.txt"]
    );
}

// @kotowari[REQ-config-040]
#[test]
fn a_relative_local_root_in_the_global_config_is_resolved_from_its_directory() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    fs::write(cwd.join("in-work.txt"), "only in the working directory\n").unwrap();
    let global = workspace.global_config_path();
    let beside_global = global.parent().unwrap().join("local-root");
    fs::create_dir_all(&beside_global).unwrap();
    fs::write(
        beside_global.join("beside-global.txt"),
        "only on the local side\n",
    )
    .unwrap();
    workspace.write_config(&global, Path::new("local-root"));

    let json = workspace.status_json(&cwd, None);

    assert_eq!(listed_paths(&json), ["beside-global.txt"]);
}

// ─── 設定を使わないサブコマンドの --config（REQ-config-028） ─────────────────

// @kotowari[REQ-config-028]
#[test]
fn logs_warns_that_config_is_ignored_and_runs_without_reading_it() {
    // logs は保存済みの記録を読むだけで副作用がない。init は設定ファイルを書くため使わない
    let home = tempfile::tempdir().unwrap();
    let cache = home.path().join("cache");
    let log_dir = cache.join("remote-merge");
    fs::create_dir_all(&log_dir).unwrap();
    fs::write(
        log_dir.join("debug.log"),
        "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"level\":\"ERROR\",\"target\":\"remote_merge::ssh\",\"message\":\"saved entry\",\"fields\":{}}\n",
    )
    .unwrap();
    let missing = home.path().join("missing.toml");
    assert!(!missing.exists());

    let output = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", &cache)
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .current_dir(home.path())
        .arg("--config")
        .arg(&missing)
        .args(["logs", "--format", "json"])
        .output()
        .unwrap();

    // 指定先がない --config でも止まらず、保存済みの記録を出す
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Warning: --config is ignored for the 'logs' subcommand"),
        "{output:?}"
    );
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let first: serde_json::Value = serde_json::from_str(
        stdout
            .lines()
            .next()
            .unwrap_or_else(|| panic!("{output:?}")),
    )
    .unwrap_or_else(|err| panic!("{err}: {output:?}"));
    assert_eq!(first["message"], "saved entry");
}

// ─── 知らないキーとセクションで止める（REQ-config-039） ─────────────────

impl Workspace {
    /// `write_config` と同じ設定の `anchor` の行の後に `addition` の行を足して書く
    fn write_config_with_line_after(
        &mut self,
        path: &Path,
        local_root: &Path,
        anchor: &str,
        addition: &str,
    ) {
        let base = gen_config(
            local_root,
            &self.dirs.remote_dir,
            None,
            self.dirs.server_port(),
        );
        let content = base.replacen(anchor, &format!("{anchor}\n{addition}"), 1);
        assert_ne!(content, base, "fixture line {anchor:?} missing");
        self.write(path, local_root, &content);
    }
}

/// 比較を始めずに終了コード 2 で止まったこと。比較を始めれば status は標準出力に一覧を書く。
/// エラーの文言は IR が定めていないので見ない
fn assert_stops_before_comparing(output: &Output) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
}

// @kotowari[REQ-config-039, EX-config-015]
#[test]
fn misspelled_key_in_the_project_config_stops_before_comparing() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let local = workspace.local_root_with("project-local", "file.txt");
    workspace.write_config_with_line_after(
        &cwd.join(".remote-merge.toml"),
        &local,
        "[filter]",
        "inclde = [\"src\"]",
    );

    let output = workspace.status(&cwd, None, &[]);

    assert_stops_before_comparing(&output);
}

// @kotowari[REQ-config-039, EX-config-021]
#[cfg(target_os = "linux")]
#[test]
fn sensitive_left_in_the_global_filter_stops_before_comparing() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let local = workspace.local_root_with("global-local", "server.pem");
    let global_path = workspace.global_config_path();
    workspace.write_config_with_line_after(
        &global_path,
        &local,
        "[filter]",
        "sensitive = [\"*.pem\"]",
    );

    let output = workspace.status(&cwd, None, &[]);

    assert_stops_before_comparing(&output);
}

// @kotowari[REQ-config-039, EX-config-022]
#[test]
fn agent_timeout_secs_left_in_the_project_config_stops_before_comparing() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let local = workspace.local_root_with("project-local", "file.txt");
    workspace.write_config_with_line_after(
        &cwd.join(".remote-merge.toml"),
        &local,
        "[agent]",
        "timeout_secs = 30",
    );

    let output = workspace.status(&cwd, None, &[]);

    assert_stops_before_comparing(&output);
}

// @kotowari[REQ-config-039]
#[test]
fn unknown_section_in_the_project_config_stops_before_comparing() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    let local = workspace.local_root_with("project-local", "file.txt");
    workspace.write_config_with_line_after(
        &cwd.join(".remote-merge.toml"),
        &local,
        "strict_host_key_checking = \"no\"",
        "\n[backups]\nenabled = true",
    );

    let output = workspace.status(&cwd, None, &[]);

    assert_stops_before_comparing(&output);
}

// ─── 解釈できない設定のエラーに書かれた中身を出さない（REQ-config-043） ───────

const SENTINEL_PASSWORD: &str = "SENTINEL-PW";

impl Workspace {
    /// [servers] の develop を `fields` のインラインテーブル一行で書いた、製品が解釈できない設定を書く。
    ///
    /// 隔離の確認は fixture のパスワードを求めるため、このファイルにはかけない。製品は知らないキーか
    /// 値の型の違いで読み込みを止めるため、このファイルから接続先を得ることはない（終了コード 2 と、
    /// 標準出力が空であることをテストで確かめる）。接続先は念のため試験サーバにする
    fn write_server_line(&self, path: &Path, fields: &str) {
        let local = self.dir("project-local");
        let content = format!(
            "[local]\nroot_dir = \"{local}\"\n\n[servers]\ndevelop = {{ host = \"127.0.0.1\", port = {port}, auth = \"password\", root_dir = \"{remote}\", {fields} }}\n",
            local = local.display(),
            port = self.dirs.server_port(),
            remote = self.dirs.remote_dir.display(),
        );
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}

/// 解釈できない設定のエラーで止まり、標準エラーにも JSON の error にも `secret` が出ないこと
fn assert_parse_error_hides(output: &Output, secret: &str) {
    assert_stops_with_parse_error(output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains(secret), "{secret:?} in stderr: {stderr}");
    if !output.stdout.is_empty() {
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|err| panic!("{err}: {output:?}"));
        let error = json["error"].as_str().expect("error missing");
        assert!(!error.contains(secret), "{secret:?} in JSON error: {error}");
    }
}

// @kotowari[REQ-config-043, EX-config-024]
#[test]
fn ex_config_024_an_unknown_key_on_the_password_line_does_not_show_the_password() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    workspace.write_server_line(
        &cwd.join(".remote-merge.toml"),
        "user = \"fixture-user\", password = \"SENTINEL-PW\", bogus = 1",
    );

    let text = workspace.status(&cwd, None, &[]);
    let json = workspace.status(&cwd, None, &["--format", "json"]);

    assert_parse_error_hides(&text, SENTINEL_PASSWORD);
    assert!(!json.stdout.is_empty(), "{json:?}");
    assert_parse_error_hides(&json, SENTINEL_PASSWORD);
}

// @kotowari[REQ-config-043, EX-config-025]
#[test]
fn ex_config_025_a_type_mismatch_on_the_password_line_does_not_show_the_password() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    workspace.write_server_line(
        &cwd.join(".remote-merge.toml"),
        "password = \"SENTINEL-PW\", user = 1",
    );

    let output = workspace.status(&cwd, None, &[]);

    assert_parse_error_hides(&output, SENTINEL_PASSWORD);
}

// @kotowari[REQ-config-043]
#[test]
fn a_value_of_the_wrong_type_is_not_shown_in_the_parse_error() {
    let mut workspace = Workspace::new();
    let cwd = workspace.dir("work");
    workspace.write_server_line(
        &cwd.join(".remote-merge.toml"),
        "user = \"fixture-user\", password = \"fixture-password\", sudo = \"SENTINEL-PW\"",
    );

    let text = workspace.status(&cwd, None, &[]);
    let json = workspace.status(&cwd, None, &["--format", "json"]);

    assert_parse_error_hides(&text, SENTINEL_PASSWORD);
    assert_parse_error_hides(&json, SENTINEL_PASSWORD);
}
