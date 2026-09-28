#![cfg(unix)]
//! include の無効な値の警告（docs/ir/config/filters.md の REQ-config-024）の契約テスト。
//!
//! 警告は設定の読み込み時に標準エラーに出るため、実行ファイルを SSH の試験サーバに対して起動して確かめる。
//! 設定は `gen_config` の [filter] の exclude の行を include の行に置き換えるだけで、
//! 既存の隔離の確認（`TestDirs::assert_isolated_config_at`）を通してから起動する。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use super::common::{gen_config, strip_ansi, TestDirs};

/// `gen_config` の [filter] の exclude の行を `include_line` に置き換えた設定
fn with_include(config: &str, include_line: &str) -> String {
    let replaced = config.replace("exclude = [\".git\", \"target\"]", include_line);
    assert_ne!(replaced, config, "fixture filter exclude line missing");
    replaced
}

/// `config` を書き、隔離を確かめてから `status --left local --right develop` を起動する
///
/// 環境変数は全て消し、HOME・XDG の変数・PATH だけを一時ディレクトリの下に向けて渡す。
fn status(dirs: &mut TestDirs, config: &str) -> Output {
    let config_path = dirs.temp.path().join("filters-config.toml");
    fs::write(&config_path, config).unwrap();
    let local_root = dirs.local_dir.clone();
    dirs.assert_isolated_config_at(&config_path, &local_root);
    let home: PathBuf = dirs.temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
    cmd.env_clear();
    cmd.env("HOME", &home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("XDG_DATA_HOME", dirs.temp.path().join("xdg-data"));
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.current_dir(&home);
    cmd.arg("--config").arg(&config_path);
    cmd.args(["status", "--left", "local", "--right", "develop"]);
    cmd.stdin(Stdio::null());
    cmd.output().expect("failed to execute status")
}

/// 標準出力と標準エラーをつないだもの
fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        strip_ansi(&output.stdout),
        strip_ansi(&output.stderr)
    )
}

fn toml_string(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

// @kotowari[REQ-config-024]
#[test]
fn absolute_traversal_and_glob_include_values_each_warn_with_the_value() {
    let mut dirs = TestDirs::new_2way(&[("src/a.txt", "local\n")], &[("src/a.txt", "remote\n")]);
    // 無効な値は有効な "src" と一緒に書き、どれも一時ディレクトリの中を指すものにする
    // （全て無効な include は FLAG-config-013 の範囲。整え方が壊れても走査が一時ディレクトリの外に向かわないようにする）
    let absolute = dirs.local_dir.join("src");
    let traversal = "src/../src";
    let glob = "lib[1]";
    let base = gen_config(&dirs.local_dir, &dirs.remote_dir, None, dirs.server_port());
    let config = with_include(
        &base,
        &format!(
            "include = [{}, {traversal:?}, {glob:?}, \"src\"]",
            toml_string(&absolute)
        ),
    );
    let output = status(&mut dirs, &config);
    let text = combined(&output);
    for expected in [
        format!(
            "Absolute path is not allowed in include filter: {}",
            absolute.display()
        ),
        format!("Path traversal is not allowed in include filter: {traversal}"),
        format!("Glob patterns are not supported in include filter: {glob}"),
    ] {
        assert!(text.contains(&expected), "{expected}: {output:?}");
    }
}
