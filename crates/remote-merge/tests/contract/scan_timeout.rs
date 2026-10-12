#![cfg(unix)]
//! SSH の経路の走査の時間切れの長さを [ssh] timeout_sec から決め、接続の時間切れと区別できる
//! エラーを出すこと（docs/ir/scan/limits.md の REQ-scan-013）の契約テスト。
//!
//! 試験 SSH サーバのログインシェルを、走査の find のときだけ timeout_sec より長く待ってから実行する
//! シェルに替え、実行ファイルを `status --left local --right develop` で起動する。接続の時間切れは、
//! 接続を受け付けるが SSH の挨拶を返さないポートに同じ timeout_sec で接続して起こす。文言は IR で
//! 決めていないので、二つのエラーの文が違うことだけを確かめる。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Output;
use std::time::{Duration, Instant};

use super::common::CliEnv;
use super::scan_listing_cli::launch_status;

const TIMEOUT_SEC: u64 = 2;
/// 走査の find を遅らせる秒数。timeout_sec より十分に長い
const SCAN_DELAY_SEC: u64 = 8;

/// 走査の find のときだけ `SCAN_DELAY_SEC` 秒待ってから sh で実行するログインシェルを書き、そのパス
fn slow_scan_shell(dir: &std::path::Path) -> &'static str {
    let path = dir.join("slow-scan-shell");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\ncase \"$2\" in *\"@RMSCAN\"*) sleep {SCAN_DELAY_SEC} ;; esac\nexec sh -c \"$2\"\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    Box::leak(path.to_str().unwrap().to_string().into_boxed_str())
}

fn with_timeout(config: &str) -> String {
    let replaced = config.replace("timeout_sec = 10", &format!("timeout_sec = {TIMEOUT_SEC}"));
    assert_ne!(replaced, config, "fixture timeout line missing");
    replaced
}

/// エラーで終わり、一覧を返さず、標準出力か標準エラーにエラーの文を出す。その文を返す
fn error_text(output: &Output) -> String {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"));
    assert!(json.get("files").is_none(), "{json}");
    json["error"]
        .as_str()
        .unwrap_or_else(|| panic!("error missing: {json}"))
        .to_string()
}

// @kotowari[REQ-scan-013]
#[test]
fn req_scan_013_the_ssh_scan_times_out_after_timeout_sec_with_its_own_error() {
    let env = CliEnv::new(&[("a.txt", "same\n")], &[("a.txt", "same\n")]);
    let config = with_timeout(&fs::read_to_string(&env.config_path).unwrap());
    fs::write(&env.config_path, &config).unwrap();
    env.use_remote_login_shell(slow_scan_shell(env.temp_root()));

    let started = Instant::now();
    let scan = env
        .cmd_with("status")
        .args([
            "--left", "local", "--right", "develop", "--all", "--format", "json",
        ])
        .output()
        .unwrap();
    let elapsed = started.elapsed();

    let scan_error = error_text(&scan);
    assert!(
        elapsed < Duration::from_secs(SCAN_DELAY_SEC),
        "the scan waited {elapsed:?} instead of timeout_sec"
    );

    // 同じ timeout_sec で、接続を受け付けるが SSH の挨拶を返さないポートに接続する
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let silent_port = silent.local_addr().unwrap().port();
    let fixture_port = toml::from_str::<toml::Value>(&config).unwrap()["servers"]["develop"]
        ["port"]
        .as_integer()
        .unwrap();
    let silent_config = config.replace(
        &format!("port = {fixture_port}"),
        &format!("port = {silent_port}"),
    );
    assert_ne!(silent_config, config, "fixture port line missing");
    let silent_config_path = env.temp_root().join("silent-config.toml");
    fs::write(&silent_config_path, silent_config).unwrap();
    let connection = launch_status(env.temp_root(), &silent_config_path, &[]);
    drop(silent);

    assert_ne!(
        scan_error,
        error_text(&connection),
        "a scan timeout must read differently from a connection timeout"
    );
}
