#![cfg(unix)]
//! include の書き方の整え方と無効な値の警告（docs/ir/config/filters.md の REQ-config-024）の契約テスト。
//!
//! 警告は設定の読み込み時に標準エラーに出るため、実行ファイルを SSH の試験サーバに対して起動して確かめる。
//! 先頭の "./" の取り除きは、設定を読むときに整えるので remote-merge-core の単体テストで確かめる。
//! 設定は `gen_config` の [filter] の exclude の行を include の行に置き換える（`with_filter`）だけで、
//! 既存の隔離の確認（`CliEnv::assert_isolated_config_at`）を通してから起動する。

use std::fs;
use std::path::Path;
use std::process::Output;

use super::common::{CliEnv, gen_config, isolated_command, strip_ansi, with_filter};

/// `config` を書き、隔離を確かめてから `status --left local --right develop` を `extra_args` を足して
/// [`isolated_command`] で起動する
pub(super) fn status(dirs: &CliEnv, config: &str, extra_args: &[&str]) -> Output {
    let config_path = dirs.temp.path().join("filters-config.toml");
    fs::write(&config_path, config).unwrap();
    let local_root = dirs.local_dir.clone();
    dirs.assert_isolated_config_at(&config_path, &local_root);
    isolated_command(&dirs.temp.path().join("home"))
        .arg("--config")
        .arg(&config_path)
        .args(["status", "--left", "local", "--right", "develop"])
        .args(extra_args)
        .output()
        .expect("failed to execute status")
}

/// 標準出力と標準エラーをつないだもの
pub(super) fn combined(output: &Output) -> String {
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
    let dirs = CliEnv::new(&[("src/a.txt", "local\n")], &[("src/a.txt", "remote\n")]);
    // 無効な値は有効な "src" と一緒に書き、どれも一時ディレクトリの中を指すものにする
    // （全て無効な include はエラーで止まる。整え方が壊れても走査が一時ディレクトリの外に向かわないようにする）
    let absolute = dirs.local_dir.join("src");
    let traversal = "src/../src";
    let globs = ["lib[1]", "lib*", "lib?"];
    let base = gen_config(&dirs.local_dir, &dirs.remote_dir, None, dirs.server_port());
    let config = with_filter(
        &base,
        &format!(
            "include = [{}, {traversal:?}, {}, \"src\"]",
            toml_string(&absolute),
            globs.map(|glob| format!("{glob:?}")).join(", ")
        ),
    );
    let output = status(&dirs, &config, &[]);
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
