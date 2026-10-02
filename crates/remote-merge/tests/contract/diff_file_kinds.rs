//! 機密ファイル・片側にだけあるファイル・バイナリの diff（docs/ir/cli/diff-output.md の
//! REQ-cli-059・060、docs/ir/cli/binary.md の REQ-cli-061）の契約テスト。組み方は diff_support にある。

use std::fs;

use remote_merge::service::output::{format_json, format_multi_diff_text};

use super::diff_support::*;

// ── REQ-cli-059: 機密ファイルの差分の隠し方 ──

// @kotowari[REQ-cli-059]
#[test]
fn req_cli_059_sensitive_text_is_hidden_without_force() {
    let fixture = DiffFixture::new(
        &[(".env", b"SECRET=left-value\n")],
        &[(".env", b"SECRET=right-value\n")],
    );

    let (output, _) = fixture.diff(&[".env"]);
    let json = json(&output);
    let file = entry(&json, ".env");

    assert_eq!(file["sensitive"], true);
    assert_eq!(file["hunks"], serde_json::json!([]));
    assert_eq!(file["note"], SENSITIVE_NOTE);
    let text = format_multi_diff_text(&output);
    assert!(text.contains(SENSITIVE_NOTE), "{text}");
    assert!(!text.contains("value"), "{text}");
    assert!(!format_json(&output).unwrap().contains("value"));
}

// @kotowari[REQ-cli-059]
#[test]
fn req_cli_059_sensitive_binary_hides_its_hashes_without_force() {
    let (left, right): (&[u8], &[u8]) = (b"left\0secret", b"right\0secret");
    let fixture = DiffFixture::new(&[(".env.local", left)], &[(".env.local", right)]);

    let (output, _) = fixture.diff(&[".env.local"]);
    let json = json(&output);
    let file = entry(&json, ".env.local");

    assert_eq!(file["sensitive"], true);
    assert_eq!(file["hunks"], serde_json::json!([]));
    assert_eq!(file["note"], SENSITIVE_NOTE);
    assert!(file.get("left_hash").is_none(), "{json}");
    assert!(file.get("right_hash").is_none(), "{json}");
    let text = format_multi_diff_text(&output);
    for hash in [sha256(left), sha256(right)] {
        assert!(!text.contains(&hash), "{text}");
        assert!(!json.to_string().contains(&hash), "{json}");
    }
    assert!(!text.contains("sha256"), "{text}");
}

// ── REQ-cli-060: 片側にだけあるファイル ──

// @kotowari[REQ-cli-060]
#[test]
fn req_cli_060_a_file_on_one_side_is_all_removed_or_all_added_lines() {
    let fixture = DiffFixture::new(
        &[("left-only.txt", b"one\ntwo\n")],
        &[("right-only.txt", b"three\nfour\n")],
    );

    let (output, _) = fixture.diff(&["left-only.txt", "right-only.txt"]);
    let json = json(&output);

    let pair = |kind: &str, content: &str| (kind.to_string(), content.to_string());
    assert_eq!(
        lines_of(entry(&json, "left-only.txt")),
        vec![pair("removed", "one"), pair("removed", "two")]
    );
    assert_eq!(
        lines_of(entry(&json, "right-only.txt")),
        vec![pair("added", "three"), pair("added", "four")]
    );
    let text = format_multi_diff_text(&output);
    for line in ["-one", "-two", "+three", "+four"] {
        assert!(text.lines().any(|l| l == line), "{line}: {text}");
    }
}

// ── REQ-cli-061: バイナリの判定と差分の出し方 ──

/// `left`・`right` の "f" を比べ、バイナリとしてハッシュが出ることを確かめる
fn assert_reported_as_binary(left: &[u8], right: &[u8]) {
    let fixture = DiffFixture::new(&[("f", left)], &[("f", right)]);

    let (output, _) = fixture.diff(&["f"]);
    let json = json(&output);
    let file = entry(&json, "f");

    assert_eq!(file["binary"], true, "{json}");
    assert_eq!(file["left_hash"], sha256(left), "{json}");
    assert_eq!(file["right_hash"], sha256(right), "{json}");
    assert_eq!(file["hunks"], serde_json::json!([]), "{json}");
    let text = format_multi_diff_text(&output);
    let expected = format!(
        "Binary files differ (left: sha256={}, right: sha256={})",
        sha256(left),
        sha256(right)
    );
    assert!(text.lines().any(|line| line == expected), "{text}");
}

// @kotowari[REQ-cli-061]
#[test]
fn req_cli_061_a_nul_in_the_first_8192_bytes_makes_the_file_binary() {
    assert_reported_as_binary(b"abc\0left\n", b"abc\0right\n");
}

// @kotowari[REQ-cli-061]
#[test]
fn req_cli_061_invalid_utf8_in_the_first_8192_bytes_makes_the_file_binary() {
    assert_reported_as_binary(b"\xff\xfeleft\n", b"\xff\xferight\n");
}

// 先頭の 8,192 バイトは ASCII だけにし、境界で多バイト文字が切れて不正な UTF-8 にならないようにする
// @kotowari[REQ-cli-061]
#[test]
fn req_cli_061_a_nul_only_after_8192_bytes_leaves_the_file_as_text() {
    let head = "a\n".repeat(4096);
    let left = format!("{head}\0left-tail\n");
    let right = format!("{head}\0right-tail\n");
    let fixture = DiffFixture::new(&[("f", left.as_bytes())], &[("f", right.as_bytes())]);

    let (output, _) = fixture.diff(&["f"]);
    let json = json(&output);
    let file = entry(&json, "f");

    assert!(file.get("binary").is_none(), "{json}");
    assert!(file.get("left_hash").is_none(), "{json}");
    let lines = lines_of(file);
    assert!(
        lines.contains(&("removed".to_string(), "\0left-tail".to_string())),
        "{lines:?}"
    );
    assert!(
        lines.contains(&("added".to_string(), "\0right-tail".to_string())),
        "{lines:?}"
    );
}

// @kotowari[REQ-cli-061]
#[test]
fn req_cli_061_the_side_without_the_binary_is_missing() {
    let content: &[u8] = b"only\0left";
    let fixture = DiffFixture::new(&[("f", content)], &[]);

    let (output, _) = fixture.diff(&["f"]);
    let json = json(&output);
    let file = entry(&json, "f");

    assert_eq!(file["binary"], true, "{json}");
    assert_eq!(file["left_hash"], sha256(content), "{json}");
    assert!(file.get("right_hash").is_none(), "{json}");
    let text = format_multi_diff_text(&output);
    let expected = format!(
        "Binary files differ (left: sha256={}, right: missing)",
        sha256(content)
    );
    assert!(text.lines().any(|line| line == expected), "{text}");
}

// 読めなくしても読めてしまう（root で動く）ときは確かめられないため飛ばす
// @kotowari[REQ-cli-061]
#[cfg(unix)]
#[test]
fn req_cli_061_the_unreadable_side_of_a_binary_is_missing() {
    use std::os::unix::fs::PermissionsExt;

    let content: &[u8] = b"right\0bytes";
    let fixture = DiffFixture::new(&[("f", b"left\0bytes")], &[("f", content)]);
    let unreadable = fixture.left.path().join("f");
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&unreadable).is_ok() {
        eprintln!("skipped: the file is readable without permission (running as root)");
        return;
    }

    let result = fixture.diff(&["f"]);
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644)).unwrap();
    let (output, _) = result;
    let json = json(&output);
    let file = entry(&json, "f");

    assert_eq!(file["binary"], true, "{json}");
    assert!(file.get("left_hash").is_none(), "{json}");
    assert_eq!(file["right_hash"], sha256(content), "{json}");
    let text = format_multi_diff_text(&output);
    let expected = format!(
        "Binary files differ (left: missing, right: sha256={})",
        sha256(content)
    );
    assert!(text.lines().any(|line| line == expected), "{text}");
}

// 中身のない側もあるファイルとして SHA-256 を出し、"missing" にしない
// @kotowari[REQ-cli-061]
#[test]
fn req_cli_061_an_existing_empty_side_of_a_binary_has_a_hash() {
    let content: &[u8] = b"bin\0ary";
    for (left, right) in [(b"" as &[u8], content), (content, b"" as &[u8])] {
        let fixture = DiffFixture::new(&[("f", left)], &[("f", right)]);

        let (output, _) = fixture.diff(&["f"]);
        let json = json(&output);
        let file = entry(&json, "f");

        assert_eq!(file["left_hash"], sha256(left), "{json}");
        assert_eq!(file["right_hash"], sha256(right), "{json}");
        assert!(!format_multi_diff_text(&output).contains("missing"));
    }
}

// @kotowari[REQ-cli-061, REQ-cli-052]
#[test]
fn req_cli_061_a_changed_binary_under_a_directory_is_reported() {
    let (left, right): (&[u8], &[u8]) = (b"left\0", b"right\0");
    let fixture = DiffFixture::new(&[("d/b.bin", left)], &[("d/b.bin", right)]);

    let (output, _) = fixture.diff(&["d"]);
    let json = json(&output);
    let file = entry(&json, "d/b.bin");

    assert_eq!(file["binary"], true, "{json}");
    assert_eq!(file["left_hash"], sha256(left), "{json}");
    assert_eq!(file["right_hash"], sha256(right), "{json}");
}
