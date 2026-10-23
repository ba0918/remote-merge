#![expect(
    unsafe_code,
    reason = "パスフレーズを尋ねる端末を持たせないため、起動するプロセスを libc の setsid で新しいセッションにする"
)]
#![cfg(unix)]
//! 設定の値の読み方と検査（docs/ir/config/values.md）のうち、実行ファイルを通して観測するものの契約テスト。
//!
//! strict_host_key_checking の警告は標準エラーに出るため、パスワードと鍵のパスとパスフレーズは SSH の接続で
//! 初めて使われるため、実行ファイルを SSH の試験サーバに対して起動して確かめる。
//! 設定は既存の隔離の確認を通らない値を含むため、起動の前に `CliEnv::assert_isolated_values_config` をかける。

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::common::{CliEnv, gen_config, isolated_command, strip_ansi};

/// ローカルの側にだけ置くファイル。接続できたときに status の JSON の "files" に出る
const LOCAL_ONLY: &str = "only-local.txt";

/// "-" と "." を含むサーバ名。環境変数の名前では、どちらも "_" になる
const DOTTED_SERVER: &str = "web-1.prod";

/// 試験サーバに `status --left local --right <サーバ名>` を起動する、隔離した実行環境。
///
/// 環境変数は全て消し、HOME・XDG の変数・PATH と、テストが渡すものだけを設定する。
struct Workspace {
    dirs: CliEnv,
}

impl Workspace {
    fn new() -> Self {
        let dirs = CliEnv::new(&[(LOCAL_ONLY, "only on the local side\n")], &[]);
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

    /// `config` を書いて隔離を確かめ、右を `server` にした status の Command を作る
    fn status_command(
        &self,
        server: &str,
        config: &str,
        require_no_host_key_checking: bool,
        envs: &[(&str, &str)],
        format: &[&str],
    ) -> Command {
        let config_path = self.dirs.temp.path().join("values-config.toml");
        fs::write(&config_path, config).unwrap();
        self.dirs.assert_isolated_values_config(
            &config_path,
            &self.home(),
            require_no_host_key_checking,
        );
        // ask のもとでのホスト鍵の確認が入力を待たないのは、isolated_command が標準入力を空にするため
        let mut cmd = isolated_command(&self.home());
        cmd.envs(envs.iter().copied());
        cmd.arg("--config").arg(&config_path);
        cmd.args(["status", "--left", "local", "--right", server]);
        cmd.args(format);
        cmd
    }

    /// 右を develop にして起動する
    fn status(
        &self,
        config: &str,
        require_no_host_key_checking: bool,
        envs: &[(&str, &str)],
        format: &[&str],
    ) -> Output {
        self.status_command(
            "develop",
            config,
            require_no_host_key_checking,
            envs,
            format,
        )
        .output()
        .expect("failed to execute status")
    }

    /// 右を develop にし、--format json で起動して、標準出力の JSON を返す
    fn status_json(&self, config: &str, envs: &[(&str, &str)]) -> serde_json::Value {
        self.status_json_on("develop", config, envs)
    }

    /// 右を `server` にし、--format json で起動して、標準出力の JSON を返す
    fn status_json_on(
        &self,
        server: &str,
        config: &str,
        envs: &[(&str, &str)],
    ) -> serde_json::Value {
        let output = self
            .status_command(server, config, true, envs, &["--format", "json"])
            .output()
            .expect("failed to execute status");
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

/// サーバ develop の名前を `name` に変えた設定
fn with_server_name(config: &str, name: &str) -> String {
    let replaced = config.replace("[servers.develop]", &format!("[servers.{name:?}]"));
    assert_ne!(replaced, config, "fixture server header missing");
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

/// 接続できなかったこと: ローカルの側にだけ置いたファイルが "files" に出ない（"files" がない場合も含む）
///
/// 失敗時の JSON のキーは IR が契約にしていないため、JSON として解釈できること（`status_json`）だけに頼る
fn assert_not_connected(json: &serde_json::Value, case: &str) {
    let listed = json["files"]
        .as_array()
        .is_some_and(|files| files.iter().any(|file| file["path"] == LOCAL_ONLY));
    assert!(!listed, "{case}: {json}");
}

// ─── 知らない strict_host_key_checking（REQ-config-018） ─────────────────

// @kotowari[REQ-config-018]
#[test]
fn unknown_strict_host_key_checking_value_stops_before_comparing() {
    let workspace = Workspace::new();
    let base = workspace.base_config();
    let config = base.replace(
        "strict_host_key_checking = \"no\"",
        "strict_host_key_checking = \"maybe\"",
    );
    assert_ne!(config, base, "fixture host key checking line missing");
    let output = workspace.status(&config, false, &[], &[]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        combined(&output).contains("strict_host_key_checking"),
        "{output:?}"
    );
}

// @kotowari[REQ-config-018]
#[test]
fn ask_value_in_any_case_is_read_without_an_error() {
    let workspace = Workspace::new();
    let base = workspace.base_config();
    for value in ["ask", "ASK"] {
        let config = base.replace(
            "strict_host_key_checking = \"no\"",
            &format!("strict_host_key_checking = {value:?}"),
        );
        assert_ne!(config, base, "fixture host key checking line missing");
        let output = workspace.status(&config, false, &[], &[]);
        assert!(
            !combined(&output).contains("strict_host_key_checking"),
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

// @kotowari[REQ-config-019]
#[test]
fn password_env_var_name_turns_hyphens_and_dots_in_the_server_name_into_underscores() {
    let workspace = Workspace::new();
    let config = with_server_name(
        &with_password(&workspace.base_config(), "wrong-password"),
        DOTTED_SERVER,
    );

    // 設定の password が正しくなくても、REMOTE_MERGE_PASSWORD_WEB_1_PROD のパスワードで接続できる
    let json = workspace.status_json_on(
        DOTTED_SERVER,
        &config,
        &[("REMOTE_MERGE_PASSWORD_WEB_1_PROD", "fixture-password")],
    );
    assert_connected(&json, "underscored env var");

    // 環境変数の名前だけを変える: サーバ名を大文字にしただけの名前は読まれず、設定の正しくない password で接続できない
    let json = workspace.status_json_on(
        DOTTED_SERVER,
        &config,
        &[("REMOTE_MERGE_PASSWORD_WEB-1.PROD", "fixture-password")],
    );
    assert_not_connected(&json, "uppercased-only env var");
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
    // エラーで止まること: 終了コードの値は IR が定めていないため、成功でないことだけを確かめる
    assert!(!output.status.success(), "{output:?}");
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
    assert!(!output.status.success(), "{output:?}");
    assert!(
        combined(&output).contains(&format!(
            "Failed to load SSH private key: {}",
            expanded.display()
        )),
        "{output:?}"
    );
}

// ─── 環境変数のパスフレーズ（REQ-config-050） ─────────────────

/// 鍵を暗号化したパスフレーズ
const KEY_PASSPHRASE: &str = "fixture passphrase";

/// `path` に、`KEY_PASSPHRASE` で暗号化した鍵を書く
fn write_encrypted_key(path: &Path) {
    use russh::keys::ssh_key::{Cipher, Kdf, LineEnding};
    let key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap();
    // 既定の bcrypt の回数は最適化しない組み立てで遅いので、回数を最小にする
    let kdf = Kdf::Bcrypt {
        salt: vec![7; 16],
        rounds: 1,
    };
    let encrypted = key
        .encrypt_with(Cipher::Aes256Ctr, kdf, 0, KEY_PASSPHRASE)
        .unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, encrypted.to_openssh(LineEnding::LF).unwrap()).unwrap();
}

/// 端末を持たない新しいセッションで起動する。製品はパスフレーズを端末（"/dev/tty"）から尋ねるので、
/// テストを端末から動かしても入力を待たず、尋ねても答えを得られない
fn output_without_terminal(mut command: Command) -> Output {
    // SAFETY: pre_exec の関数は fork の後の子で動くので、async-signal-safe な関数だけを呼ぶ。setsid はその一つ
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.output().expect("failed to execute status")
}

// @kotowari[REQ-config-050]
#[test]
fn key_passphrase_env_var_name_turns_hyphens_and_dots_in_the_server_name_into_underscores() {
    let workspace = Workspace::new();
    let key = workspace.home().join("keys/encrypted");
    write_encrypted_key(&key);
    let config = with_server_name(
        &with_key_auth(&workspace.base_config(), Some("key = \"~/keys/encrypted\"")),
        DOTTED_SERVER,
    );
    let load_error = format!("Failed to load SSH private key: {}", key.display());
    let run = |env_name: &str| {
        output_without_terminal(workspace.status_command(
            DOTTED_SERVER,
            &config,
            true,
            &[(env_name, KEY_PASSPHRASE)],
            &[],
        ))
    };

    // REMOTE_MERGE_KEY_PASSPHRASE_WEB_1_PROD のパスフレーズで鍵を開け、鍵を読めないエラーにならない
    // （試験サーバは鍵による認証を受け付けないので、その先の認証で止まる）
    let output = run("REMOTE_MERGE_KEY_PASSPHRASE_WEB_1_PROD");
    assert!(
        !combined(&output).contains(&load_error),
        "underscored env var: {output:?}"
    );

    // 環境変数の名前だけを変える: "." を残した名前は読まれず、端末がないので尋ねても答えを得られず、
    // 鍵を読めないエラーで止まる
    let output = run("REMOTE_MERGE_KEY_PASSPHRASE_WEB_1.PROD");
    assert!(!output.status.success(), "dotted env var: {output:?}");
    assert!(
        combined(&output).contains(&load_error),
        "dotted env var: {output:?}"
    );
}

// ─── auth が key のサーバの password（REQ-config-029） ─────────────────

/// develop を auth = "key" にし、password の行は残した設定
fn with_key_auth_keeping_password(config: &str) -> String {
    let replaced = config.replace(
        "auth = \"password\"\n        password = \"fixture-password\"",
        "auth = \"key\"\n        password = \"fixture-password\"",
    );
    assert_ne!(replaced, config, "fixture auth lines missing");
    replaced
}

const KEY_AUTH_PASSWORD_WARNING: &str =
    "servers.develop: password is set but auth is 'key' — password will be ignored";

// @kotowari[REQ-config-029]
#[test]
fn password_on_a_key_auth_server_warns_and_is_not_used_for_authentication() {
    let workspace = Workspace::new();
    assert!(!workspace.home().join(".ssh/id_rsa").exists());
    let config = with_key_auth_keeping_password(&workspace.base_config());
    let output = workspace.status(&config, true, &[], &[]);
    assert!(
        combined(&output).contains(KEY_AUTH_PASSWORD_WARNING),
        "{output:?}"
    );
    // 試験サーバが受け付ける正しい password が書かれていても、鍵による認証を選んで鍵を読めずに止まる
    assert!(!output.status.success(), "{output:?}");
    assert!(
        combined(&output).contains("Failed to load SSH private key: ~/.ssh/id_rsa"),
        "{output:?}"
    );

    // password の行だけを除くと警告は出ない: 警告が password によるものであることを示す
    let output = workspace.status(
        &with_key_auth(&workspace.base_config(), None),
        true,
        &[],
        &[],
    );
    assert!(
        !combined(&output).contains("password is set but auth is 'key'"),
        "{output:?}"
    );
}
