#![cfg(unix)]
//! include の書き方の整え方と無効な値の警告（docs/ir/config/filters.md の REQ-config-024）の契約テスト。
//!
//! 警告は設定の読み込み時に標準エラーに出るため、実行ファイルを SSH の試験サーバに対して起動して確かめる。
//! 先頭の "./" の取り除きは、ローカルの走査では吸収されて見分けられないため、同じ起動の
//! `--format json` の "files" の "path" で確かめる。
//! 設定は `gen_config` の [filter] の exclude の行を include の行に置き換えるだけで、
//! 既存の隔離の確認（`TestDirs::assert_isolated_config_at`）を通してから起動する。

use std::collections::BTreeSet;
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

/// `config` を書き、隔離を確かめてから `status --left local --right develop` を `extra_args` を足して起動する
///
/// 環境変数は全て消し、HOME・XDG の変数・PATH だけを一時ディレクトリの下に向けて渡す。
fn status(dirs: &mut TestDirs, config: &str, extra_args: &[&str]) -> Output {
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
    cmd.args(extra_args);
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
    let globs = ["lib[1]", "lib*", "lib?"];
    let base = gen_config(&dirs.local_dir, &dirs.remote_dir, None, dirs.server_port());
    let config = with_include(
        &base,
        &format!(
            "include = [{}, {traversal:?}, {}, \"src\"]",
            toml_string(&absolute),
            globs.map(|glob| format!("{glob:?}")).join(", ")
        ),
    );
    let output = status(&mut dirs, &config, &[]);
    let text = combined(&output);
    let glob_warnings = globs
        .iter()
        .map(|glob| format!("Glob patterns are not supported in include filter: {glob}"));
    for expected in [
        format!(
            "Absolute path is not allowed in include filter: {}",
            absolute.display()
        ),
        format!("Path traversal is not allowed in include filter: {traversal}"),
    ]
    .into_iter()
    .chain(glob_warnings)
    {
        assert!(text.contains(&expected), "{expected}: {output:?}");
    }
}

/// include を `include_line` にして `status --format json` を起動し、標準出力の JSON の "files" の "path" の集合を返す
fn listed_over_ssh(dirs: &mut TestDirs, include_line: &str) -> BTreeSet<String> {
    let base = gen_config(&dirs.local_dir, &dirs.remote_dir, None, dirs.server_port());
    let config = with_include(&base, include_line);
    let output = status(dirs, &config, &["--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("status output is not JSON ({error}): {output:?}"));
    json["files"]
        .as_array()
        .unwrap_or_else(|| panic!("files missing: {json}"))
        .iter()
        .map(|file| file["path"].as_str().unwrap().to_string())
        .collect()
}

// @kotowari[REQ-config-024]
#[test]
fn include_with_leading_dot_slash_lists_the_same_paths_as_the_plain_value_over_ssh() {
    // ローカルの走査は canonicalize で "./" を吸収して見分けられないため、include の値を
    // find の開始パスにそのまま繋ぐ SSH の走査（右の develop）で観測する
    let mut dirs = TestDirs::new_2way(
        &[("src/a.txt", "local\n"), ("top.txt", "local\n")],
        &[("src/a.txt", "remote\n"), ("top.txt", "remote\n")],
    );
    let expected: BTreeSet<String> = ["src/a.txt".to_string()].into();
    assert_eq!(listed_over_ssh(&mut dirs, "include = [\"src\"]"), expected);
    assert_eq!(
        listed_over_ssh(&mut dirs, "include = [\"./src\"]"),
        expected
    );
}
