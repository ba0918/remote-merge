//! 共通テストヘルパーモジュール
//!
//! TUI E2E テストと CLI E2E テストで共有するヘルパー関数・構造体を集約する。
//! `tests/tui_e2e.rs` と `tests/cli_exit_code.rs` から抽出したもの + 新規ヘルパー。

#![allow(dead_code)]

use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use expectrl::process::unix::UnixProcess;
use expectrl::stream::log::LogStream;
use tempfile::TempDir;

// ─── 型エイリアス ────────────────────────────────────────

/// TUI E2E テスト用の Session 型エイリアス
pub type TuiSession =
    expectrl::Session<UnixProcess, LogStream<expectrl::process::unix::PtyStream, std::io::Stderr>>;

// ─── ファイル配置ヘルパー ────────────────────────────────

/// テキストファイルを配置する。親ディレクトリがなければ再帰的に作成する。
pub fn place_files(dir: &Path, files: &[(&str, &str)]) {
    for (path, content) in files {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
}

/// バイナリファイルを配置する（NUL バイト等を含むファイル用）。
pub fn place_binary_file(dir: &Path, path: &str, content: &[u8]) {
    let full = dir.join(path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full, content).unwrap();
}

/// シンボリックリンクを作成する。
///
/// `link_path` は `dir` からの相対パス。
/// `target_path` はリンク先のパス（相対でも絶対でも可）。
pub fn place_symlink(dir: &Path, link_path: &str, target_path: &str) {
    let full_link = dir.join(link_path);
    if let Some(parent) = full_link.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    unix_fs::symlink(target_path, &full_link).unwrap();
}

// ─── ANSI エスケープ除去 ─────────────────────────────────

/// ANSI エスケープシーケンスを除去してプレーンテキストにする
pub fn strip_ansi(input: &[u8]) -> String {
    let s = String::from_utf8_lossy(input);
    regex_lite_strip(&s)
}

/// 簡易 ANSI ストリッパー（正規表現クレート不要版）
///
/// CSI シーケンス (ESC [ ... letter) と OSC シーケンス (ESC ] ... BEL) を除去する。
pub fn regex_lite_strip(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC [ ... (letter) をスキップ
            if chars.peek() == Some(&'[') {
                chars.next(); // '['
                              // パラメータバイトと中間バイトを読み飛ばし
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphabetic() || next == '~' {
                        chars.next(); // 終端文字
                        break;
                    }
                    chars.next();
                }
            }
            // ESC ] (OSC) もスキップ
            else if chars.peek() == Some(&']') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    if next == '\x07' || next == '\\' {
                        chars.next();
                        break;
                    }
                    chars.next();
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ─── Config 生成 ─────────────────────────────────────────

/// テスト用の config TOML 文字列を生成する。
///
/// localhost SSH 経由で接続する設定を返す。
/// `staging_dir` が `Some` なら 3way 構成（develop + staging）の config を生成する。
pub fn gen_config(local_dir: &Path, develop_dir: &Path, staging_dir: Option<&Path>) -> String {
    let home = std::env::var("HOME").expect("HOME not set");
    let key_path = format!("{}/.ssh/id_ed25519", home);
    let user = std::env::var("USER").expect("USER not set");

    let mut config = format!(
        r#"[local]
root_dir = "{local}"

[servers.develop]
host = "localhost"
port = 22
user = "{user}"
auth = "key"
key = "{key}"
root_dir = "{develop}"
"#,
        local = local_dir.display(),
        develop = develop_dir.display(),
        user = user,
        key = key_path,
    );

    if let Some(staging) = staging_dir {
        config.push_str(&format!(
            r#"
[servers.staging]
host = "localhost"
port = 22
user = "{user}"
auth = "key"
key = "{key}"
root_dir = "{staging}"
"#,
            user = user,
            key = key_path,
            staging = staging.display(),
        ));
    }

    config.push_str(
        r#"
[filter]
exclude = [".git", "target"]

[ssh]
timeout_sec = 10
"#,
    );

    config
}

// ─── TestDirs（共通ディレクトリ構成） ─────────────────────

/// テスト環境のディレクトリ構成。E2eEnv と CliEnv の共通ロジックを集約する。
pub struct TestDirs {
    pub temp: TempDir,
    pub config_path: String,
    pub local_dir: PathBuf,
    pub remote_dir: PathBuf,
}

impl TestDirs {
    /// 2サーバー構成: local <-> develop(remote)
    pub fn new_2way(local_files: &[(&str, &str)], remote_files: &[(&str, &str)]) -> Self {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let base = temp.path();

        let local_dir = base.join("local");
        let remote_dir = base.join("remote");
        fs::create_dir_all(&local_dir).unwrap();
        fs::create_dir_all(&remote_dir).unwrap();

        place_files(&local_dir, local_files);
        place_files(&remote_dir, remote_files);

        let config_path = base.join("test-config.toml");
        let config_content = gen_config(&local_dir, &remote_dir, None);
        fs::write(&config_path, &config_content).unwrap();

        Self {
            temp,
            config_path: config_path.to_string_lossy().to_string(),
            local_dir,
            remote_dir,
        }
    }

    /// 3サーバー構成: local(ref) + develop(left) + staging(right)
    ///
    /// `remote_dir` は develop_dir を指す（2way との互換性のため）。
    pub fn new_3way(
        local_files: &[(&str, &str)],
        develop_files: &[(&str, &str)],
        staging_files: &[(&str, &str)],
    ) -> Self {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let base = temp.path();

        let local_dir = base.join("local");
        let develop_dir = base.join("develop");
        let staging_dir = base.join("staging");
        fs::create_dir_all(&local_dir).unwrap();
        fs::create_dir_all(&develop_dir).unwrap();
        fs::create_dir_all(&staging_dir).unwrap();

        place_files(&local_dir, local_files);
        place_files(&develop_dir, develop_files);
        place_files(&staging_dir, staging_files);

        let config_path = base.join("test-config.toml");
        let config_content = gen_config(&local_dir, &develop_dir, Some(&staging_dir));
        fs::write(&config_path, &config_content).unwrap();

        Self {
            temp,
            config_path: config_path.to_string_lossy().to_string(),
            local_dir,
            remote_dir: develop_dir,
        }
    }
}

// ─── E2eEnv（TUI E2E テスト用） ─────────────────────────

/// TUI E2E テスト用の環境を構築する。
/// local/remote/staging ディレクトリにファイルを配置し、テスト用 config を生成する。
pub struct E2eEnv {
    _dirs: TestDirs,
    pub config_path: String,
}

impl E2eEnv {
    /// 2サーバー構成: local <-> develop(remote)
    pub fn new(local_files: &[(&str, &str)], remote_files: &[(&str, &str)]) -> Self {
        let dirs = TestDirs::new_2way(local_files, remote_files);
        let config_path = dirs.config_path.clone();
        Self {
            _dirs: dirs,
            config_path,
        }
    }

    /// 3サーバー構成: develop(left) <-> staging(right) + local(ref)
    ///
    /// スクショの再現: `develop <-> staging` で local が reference。
    pub fn new_3way(
        local_files: &[(&str, &str)],
        develop_files: &[(&str, &str)],
        staging_files: &[(&str, &str)],
    ) -> Self {
        let dirs = TestDirs::new_3way(local_files, develop_files, staging_files);
        let config_path = dirs.config_path.clone();
        Self {
            _dirs: dirs,
            config_path,
        }
    }

    /// TUI を起動して expectrl Session を返す。
    /// 追加の CLI 引数を指定可能。
    /// PTY サイズを 200x50 にリサイズして 3way バッジが描画されるようにする。
    pub fn spawn_tui_with_args(&self, extra_args: &[&str]) -> TuiSession {
        let binary = env!("CARGO_BIN_EXE_remote-merge");
        let mut cmd = Command::new(binary);
        cmd.arg("--config").arg(&self.config_path);
        cmd.arg("--log-level").arg("debug");

        for arg in extra_args {
            cmd.arg(arg);
        }

        let mut session = expectrl::Session::spawn(cmd).expect("Failed to spawn TUI process");

        // PTY のウィンドウサイズを ioctl (TIOCSWINSZ) で設定
        // 環境変数 COLUMNS/LINES は PTY サイズに影響しないため ioctl が必要
        session
            .get_process_mut()
            .set_window_size(200, 50)
            .expect("Failed to set PTY window size");

        expectrl::session::log(session, std::io::stderr()).expect("Failed to set up logging")
    }

    /// TUI をデフォルト引数で起動する。
    pub fn spawn_tui(&self) -> TuiSession {
        self.spawn_tui_with_args(&[])
    }
}

// ─── CliEnv（CLI E2E テスト用） ──────────────────────────

/// CLI E2E テスト用の環境。SSH 接続を使う CLI テスト向け。
///
/// `TestDirs` を内部で使い、CLI サブコマンドの実行に特化したヘルパーを提供する。
pub struct CliEnv {
    _dirs: TestDirs,
    pub config_path: String,
    pub local_dir: PathBuf,
    pub remote_dir: PathBuf,
}

impl CliEnv {
    /// 2サーバー構成: local <-> develop(remote)
    pub fn new(local_files: &[(&str, &str)], remote_files: &[(&str, &str)]) -> Self {
        let dirs = TestDirs::new_2way(local_files, remote_files);
        let config_path = dirs.config_path.clone();
        let local_dir = dirs.local_dir.clone();
        let remote_dir = dirs.remote_dir.clone();
        Self {
            _dirs: dirs,
            config_path,
            local_dir,
            remote_dir,
        }
    }

    /// 3サーバー構成: local(ref) + develop(left) + staging(right)
    pub fn new_3way(
        local_files: &[(&str, &str)],
        develop_files: &[(&str, &str)],
        staging_files: &[(&str, &str)],
    ) -> Self {
        let dirs = TestDirs::new_3way(local_files, develop_files, staging_files);
        let config_path = dirs.config_path.clone();
        let local_dir = dirs.local_dir.clone();
        let remote_dir = dirs.remote_dir.clone();
        Self {
            _dirs: dirs,
            config_path,
            local_dir,
            remote_dir,
        }
    }

    /// CLI コマンドを生成（--config 付き）
    ///
    /// 環境変数はクリアされ、RUST_LOG 等の影響を排除する。
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
        cmd.env_clear();
        // 最低限必要な環境変数を復元
        if let Ok(home) = std::env::var("HOME") {
            cmd.env("HOME", home);
        }
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        cmd.arg("--config").arg(&self.config_path);
        cmd
    }

    /// CLI コマンドをサブコマンド付きで生成
    pub fn cmd_with(&self, subcommand: &str) -> Command {
        let mut cmd = self.cmd();
        cmd.arg(subcommand);
        cmd
    }
}

// ─── CLI コマンド生成（config なし） ─────────────────────

/// remote-merge バイナリの Command を生成する（config 指定なし）。
///
/// 環境変数をクリアして RUST_LOG 等の影響を排除する。
/// exit code テスト等、config 不要なケースで使う。
pub fn remote_merge_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
    cmd.env_clear();
    // 最低限必要な環境変数を復元
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", home);
    }
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd
}

// ─── アサーションヘルパー ────────────────────────────────

/// exit code が 0（成功）であることをアサートする。
///
/// 失敗時は stderr の内容を表示する。
pub fn assert_exit_success(output: &Output) {
    assert!(
        output.status.success(),
        "Expected exit code 0, got {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// 指定した exit code であることをアサートする。
///
/// 失敗時は stderr の内容を表示する。
pub fn assert_exit_error(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "Expected exit code {}, got {:?}. stderr: {}",
        code,
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// stderr に指定した文字列が含まれることをアサートする。
pub fn assert_stderr_contains(output: &Output, text: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(text),
        "Expected stderr to contain '{}', got: {}",
        text,
        stderr,
    );
}

/// stdout に指定した文字列が含まれることをアサートする。
pub fn assert_stdout_contains(output: &Output, text: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(text),
        "Expected stdout to contain '{}', got: {}",
        text,
        stdout,
    );
}
