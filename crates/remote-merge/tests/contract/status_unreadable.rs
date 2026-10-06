#![cfg(unix)]
//! status で中身を読み比べるファイルを読めないとき（docs/ir/cli/unreadable.md の REQ-cli-088）の
//! 契約テスト。
//!
//! 右がサーバで --ref がないときはハッシュの経路、右が local のときは中身を読む経路で比べるため、
//! 左右を入れ替えた二つの構成で確かめる。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge::service::output::format_json;

use super::status_support::{args, fixture, write_at};

const T0: u64 = 1_700_000_000;

// 大きさが同じで更新時刻の違うファイルは中身を読み比べる。develop の側を読めなくする
// @kotowari[EX-cli-086]
#[test]
fn a_file_that_cannot_be_read_for_comparison_is_reported_as_an_error_without_a_judgement() {
    let fixture = fixture();
    write_at(fixture.local.path(), "a.txt", b"alpha\n", T0);
    write_at(fixture.develop.path(), "a.txt", b"bravo\n", T0 + 60);
    write_at(fixture.local.path(), "b.txt", b"short\n", T0);
    write_at(fixture.develop.path(), "b.txt", b"much longer\n", T0);
    let unreadable = fixture.develop.path().join("a.txt");
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o200)).unwrap();
    assert!(
        fs::File::open(&unreadable).is_err(),
        "test needs an unreadable develop/a.txt"
    );

    for (left, right) in [("local", "develop"), ("develop", "local")] {
        let mut args = args(Some(left), Some(right));
        args.all = true;
        let result = fixture.run(args).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&format_json(&result.output).unwrap()).unwrap();

        assert_eq!(result.exit_code, 2, "{left} {right}: {json}");
        let errors = json["errors"]
            .as_array()
            .unwrap_or_else(|| panic!("errors missing: {json}"));
        assert_eq!(errors.len(), 1, "{json}");
        assert_eq!(errors[0]["path"], "a.txt", "{json}");
        assert!(
            errors[0]["reason"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty()),
            "{json}"
        );
        let files = json["files"].as_array().unwrap();
        assert!(
            !files.iter().any(|file| file["path"] == "a.txt"),
            "{left} {right}: {json}"
        );
        // ほかのファイルの比較は続ける
        assert!(
            files
                .iter()
                .any(|file| file["path"] == "b.txt" && file["status"] == "modified"),
            "{json}"
        );
    }
}

// テキストの出力では、読めなかったファイルのパスを標準エラーに出す（文言は固定しない）
// @kotowari[REQ-cli-088]
#[cfg(feature = "test-utils")]
#[test]
fn text_output_reports_an_unreadable_file_on_standard_error() {
    let env = super::common::CliEnv::new(&[("a.txt", "alpha\n")], &[("a.txt", "bravo\n")]);
    let remote = env.remote_dir.join("a.txt");
    fs::File::options()
        .write(true)
        .open(&remote)
        .unwrap()
        .set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(T0))
        .unwrap();
    fs::set_permissions(&remote, fs::Permissions::from_mode(0o200)).unwrap();
    assert!(
        fs::File::open(&remote).is_err(),
        "test needs an unreadable remote a.txt"
    );

    let output = env
        .cmd_with("status")
        .args(["--left", "local", "--right", "develop", "--all"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("a.txt"),
        "{output:?}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("a.txt"),
        "{output:?}"
    );
}
