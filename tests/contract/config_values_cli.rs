#![cfg(unix)]
//! 設定の値の読み方と検査（docs/ir/config/values.md）のうち、実行ファイルを通して観測するものの契約テスト。
//!
//! strict_host_key_checking の警告は標準エラーに出るため、パスワードと鍵のパスは SSH の接続で初めて使われるため、
//! 実行ファイルを SSH の試験サーバに対して起動して確かめる。
//! 設定は既存の隔離の確認を通らない値を含むため、起動の前に `TestDirs::assert_isolated_values_config` をかける。

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use super::common::{gen_config, strip_ansi, TestDirs};

/// ローカルの側にだけ置くファイル。接続できたときに status の JSON の "files" に出る
const LOCAL_ONLY: &str = "only-local.txt";

/// 試験サーバの develop に `status --left local --right develop` を起動する、隔離した実行環境。
///
/// 環境変数は全て消し、HOME・XDG の変数・PATH と、テストが渡すものだけを設定する。
struct Workspace {
    dirs: TestDirs,
}

impl Workspace {
    fn new() -> Self {
        let dirs = TestDirs::new_2way(&[(LOCAL_ONLY, "only on the local side\n")], &[]);
        let workspace = Self { dirs };
        fs::create_dir_all(workspace.home()).unwrap();
        workspace
    }

    fn home(&self) -> PathBuf {
        self.dirs.temp.path().join("home")
    }

    /// 試験サーバの develop を持つ設定（password が "fixture-password"、strict_host_key_checking が "no"）
    fn base_config(&self) -> String {
        gen_config(
            &self.dirs.local_dir,
            &self.dirs.remote_dir,
            None,
            self.dirs.server_port(),
        )
    }

    /// `config` を書き、隔離を確かめてから起動する
    fn status(
        &self,
        config: &str,
        require_no_host_key_checking: bool,
        envs: &[(&str, &str)],
        format: &[&str],
    ) -> Output {
        let config_path = self.dirs.temp.path().join("values-config.toml");
        fs::write(&config_path, config).unwrap();
        self.dirs.assert_isolated_values_config(
            &config_path,
            &self.home(),
            require_no_host_key_checking,
        );
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
        cmd.env_clear();
        cmd.env("HOME", self.home());
        cmd.env("XDG_CONFIG_HOME", self.home().join(".config"));
        cmd.env("XDG_DATA_HOME", self.dirs.temp.path().join("xdg-data"));
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        cmd.envs(envs.iter().copied());
        cmd.current_dir(self.home());
        cmd.arg("--config").arg(&config_path);
        cmd.args(["status", "--left", "local", "--right", "develop"]);
        cmd.args(format);
        // ask のもとでのホスト鍵の確認が入力を待たないよう、標準入力を引き継がない
        cmd.stdin(Stdio::null());
        cmd.output().expect("failed to execute status")
    }

    /// --format json で起動し、標準出力の JSON を返す
    fn status_json(&self, config: &str, envs: &[(&str, &str)]) -> serde_json::Value {
        let output = self.status(config, true, envs, &["--format", "json"]);
        serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|err| panic!("status did not print JSON ({err}): {output:?}"))
    }
}

/// 標準出力と標準エラーをつないだもの
fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        strip_ansi(&output.stdout),
        strip_ansi(&output.stderr)
    )
}

fn with_password(config: &str, password: &str) -> String {
    let replaced = config.replace(
        "password = \"fixture-password\"",
        &format!("password = {password:?}"),
    );
    assert_ne!(replaced, config, "fixture password line missing");
    replaced
}

/// 接続できたこと: ローカルの側にだけ置いたファイルが "files" に出る
fn assert_connected(json: &serde_json::Value, case: &str) {
    let files = json["files"]
        .as_array()
        .unwrap_or_else(|| panic!("{case}: files missing: {json}"));
    assert!(
        files.iter().any(|file| file["path"] == LOCAL_ONLY),
        "{case}: {json}"
    );
}

/// 接続できなかったこと: 失敗しても JSON で返し、"error" を持ち "files" を持たない
fn assert_not_connected(json: &serde_json::Value, case: &str) {
    assert!(json.get("error").is_some(), "{case}: {json}");
    assert!(json.get("files").is_none(), "{case}: {json}");
}

// ─── 知らない strict_host_key_checking の警告（REQ-config-018） ─────────────────

// @kotowari[REQ-config-018]
#[test]
fn unknown_strict_host_key_checking_value_warns_and_falls_back_to_ask() {
    let workspace = Workspace::new();
    let base = workspace.base_config();
    let config = base.replace(
        "strict_host_key_checking = \"no\"",
        "strict_host_key_checking = \"maybe\"",
    );
    assert_ne!(config, base, "fixture host key checking line missing");
    // 終了コードとその後の接続の結果は確かめない（ask のもとでの未知のホスト鍵の扱いは REQ-ssh-001 の範囲）
    let output = workspace.status(&config, false, &[], &[]);
    assert!(
        combined(&output)
            .contains("Unknown strict_host_key_checking value: 'maybe', falling back to 'ask'"),
        "{output:?}"
    );
}

// @kotowari[REQ-config-018]
#[test]
fn ask_value_in_any_case_does_not_warn() {
    let workspace = Workspace::new();
    let base = workspace.base_config();
    // 同じ準備で警告が出ることを先に確かめ、警告がないことが観測の不足によらないことを示す
    for (value, warns) in [("maybe", true), ("ask", false), ("ASK", false)] {
        let config = base.replace(
            "strict_host_key_checking = \"no\"",
            &format!("strict_host_key_checking = {value:?}"),
        );
        assert_ne!(config, base, "fixture host key checking line missing");
        let output = workspace.status(&config, false, &[], &[]);
        assert_eq!(
            combined(&output).contains("Unknown strict_host_key_checking value"),
            warns,
            "{value:?}: {output:?}"
        );
    }
}

// ─── 環境変数のパスワード（REQ-config-019） ─────────────────

// @kotowari[REQ-config-019]
#[test]
fn password_from_the_uppercase_server_env_var_wins_over_the_config_password() {
    let workspace = Workspace::new();
    let config = with_password(&workspace.base_config(), "wrong-password");

    // 設定の password が正しくなくても、REMOTE_MERGE_PASSWORD_DEVELOP のパスワードで接続できる
    let json = workspace.status_json(
        &config,
        &[("REMOTE_MERGE_PASSWORD_DEVELOP", "fixture-password")],
    );
    assert_connected(&json, "uppercase env var");

    // 環境変数の名前だけを変える: サーバ名を大文字にしない名前は読まれず、設定の正しくない password で接続できない
    let json = workspace.status_json(
        &config,
        &[("REMOTE_MERGE_PASSWORD_develop", "fixture-password")],
    );
    assert_not_connected(&json, "lowercase env var");
}

// @kotowari[REQ-config-019]
#[test]
fn empty_server_env_var_is_treated_as_unset() {
    let workspace = Workspace::new();
    let config = workspace.base_config();

    // 空の REMOTE_MERGE_PASSWORD_DEVELOP は設定されていないものとして扱い、設定の正しい password で接続できる
    let json = workspace.status_json(&config, &[("REMOTE_MERGE_PASSWORD_DEVELOP", "")]);
    assert_connected(&json, "empty env var");

    // 環境変数の値だけを変える: 空でない値は設定の正しい password より優先され、接続できない
    let json = workspace.status_json(
        &config,
        &[("REMOTE_MERGE_PASSWORD_DEVELOP", "wrong-password")],
    );
    assert_not_connected(&json, "wrong env var");
}

// ─── 鍵のパス（REQ-config-020） ─────────────────

/// develop を auth = "key" にし、password の行を除いた設定（key は `key_line` があれば足す）
fn with_key_auth(config: &str, key_line: Option<&str>) -> String {
    let replacement = match key_line {
        Some(line) => format!("auth = \"key\"\n        {line}"),
        None => "auth = \"key\"".to_string(),
    };
    let replaced = config.replace(
        "auth = \"password\"\n        password = \"fixture-password\"",
        &replacement,
    );
    assert_ne!(replaced, config, "fixture auth lines missing");
    replaced
}

// @kotowari[REQ-config-020]
#[test]
fn omitted_key_uses_the_default_path_and_names_it_when_it_cannot_be_read() {
    let workspace = Workspace::new();
    assert!(!workspace.home().join(".ssh/id_rsa").exists());
    let config = with_key_auth(&workspace.base_config(), None);
    let output = workspace.status(&config, true, &[], &[]);
    assert!(
        combined(&output).contains("Failed to load SSH private key: ~/.ssh/id_rsa"),
        "{output:?}"
    );
}

// @kotowari[REQ-config-020]
#[test]
fn key_starting_with_tilde_is_expanded_under_home_and_named_when_it_cannot_be_read() {
    let workspace = Workspace::new();
    let expanded = workspace.home().join("keys/missing");
    assert!(!expanded.exists());
    let config = with_key_auth(&workspace.base_config(), Some("key = \"~/keys/missing\""));
    let output = workspace.status(&config, true, &[], &[]);
    assert!(
        combined(&output).contains(&format!(
            "Failed to load SSH private key: {}",
            expanded.display()
        )),
        "{output:?}"
    );
}
