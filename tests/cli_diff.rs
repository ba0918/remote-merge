#![cfg(unix)]
//! `diff` サブコマンドの E2E テスト。
//!
//! SSH 接続（localhost）を使用するため `#[ignore]` 付き。
//! `cargo test --test cli_diff -- --ignored` で実行する。

mod common;
use common::*;

/// JSON フォーマットで diff を出力し、有効な JSON でありファイル情報を含む
// @kotowari[EX-cli-035]
#[test]
fn test_diff_json_format() {
    let env = CliEnv::new(
        &[("file.txt", "local content\n")],
        &[("file.txt", "remote content\n")],
    );

    let output = env
        .cmd_with("diff")
        .args(["file.txt", "--format", "json"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 有効な JSON であることを確認
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(parsed["files"][0]["path"], "file.txt");
    let hunks = parsed["files"][0]["hunks"].to_string();
    assert!(
        hunks.contains("local content") && hunks.contains("remote content"),
        "{parsed}"
    );
}

/// バイナリファイルの diff で SHA-256 ハッシュが表示される
// @kotowari[EX-cli-017]
#[test]
fn test_diff_binary_file() {
    let env = CliEnv::new(&[], &[]);
    // NUL バイトを含むバイナリファイルを配置
    let binary_content = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
    place_binary_file(&env.local_dir, "image.png", binary_content);
    place_binary_file(
        &env.remote_dir,
        "image.png",
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rDIFF",
    );

    let output = env
        .cmd_with("diff")
        .arg("image.png")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 実際の出力: "Binary files differ (left: sha256=..., right: sha256=...)"
    assert!(
        stdout.contains("Binary files differ"),
        "Expected 'Binary files differ' in output, got: {}",
        stdout
    );
    assert!(
        stdout.contains("sha256="),
        "Expected 'sha256=' hash in output, got: {}",
        stdout
    );
}

// @kotowari[EX-cli-039]
#[test]
fn diff_shows_link_targets_and_resolved_file_contents() {
    let env = CliEnv::new(&[("target.txt", "left content\n")], &[]);
    place_symlink(&env.local_dir, "link.txt", "target.txt");
    // リモートにもリンク先ファイルとシンボリックリンクを配置（異なるターゲット）
    place_files(&env.remote_dir, &[("other.txt", "right content\n")]);
    place_symlink(&env.remote_dir, "link.txt", "other.txt");

    let output = env
        .cmd_with("diff")
        .arg("link.txt")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("target.txt") && stdout.contains("other.txt"),
        "{output:?}"
    );
    assert!(
        stdout.contains("left content") && stdout.contains("right content"),
        "{output:?}"
    );
}

// @kotowari[EX-cli-040]
#[test]
fn same_link_target_with_changed_content_is_a_diff() {
    let env = CliEnv::new(
        &[("target.txt", "left body\n")],
        &[("target.txt", "right body\n")],
    );
    place_symlink(&env.local_dir, "link.txt", "target.txt");
    place_symlink(&env.remote_dir, "link.txt", "target.txt");
    let output = env.cmd_with("diff").arg("link.txt").output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(
        body.contains("left body") && body.contains("right body"),
        "{output:?}"
    );
}

// @kotowari[EX-cli-041]
#[test]
fn link_and_regular_file_with_equal_contents_still_differ_in_kind() {
    let env = CliEnv::new(
        &[("target.txt", "same body\n")],
        &[("link.txt", "same body\n")],
    );
    place_symlink(&env.local_dir, "link.txt", "target.txt");
    let output = env
        .cmd_with("diff")
        .args(["link.txt", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["files"][0]["link_targets"]["left"], "target.txt");
    assert!(result["files"][0]["link_targets"]["right"].is_null());
    assert!(
        result["files"][0]["hunks"].as_array().unwrap().is_empty(),
        "{result}"
    );
}

// @kotowari[EX-cli-058]
#[test]
fn external_link_is_not_read_without_explicit_permission() {
    let env = CliEnv::new(&[], &[]);
    let outside = env.local_dir.parent().unwrap().join("outside.txt");
    std::fs::write(&outside, "external private content\n").unwrap();
    place_symlink(&env.local_dir, "link.txt", outside.to_str().unwrap());
    place_symlink(&env.remote_dir, "link.txt", outside.to_str().unwrap());

    let output = env.cmd_with("diff").arg("link.txt").output().unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        combined.contains("outside.txt") && combined.contains("not compared"),
        "{output:?}"
    );
    assert!(!combined.contains("external private content"), "{output:?}");
}

// @kotowari[EX-cli-039, EX-cli-058]
#[test]
fn json_diff_keeps_link_targets_and_reports_unexamined_external_content() {
    let env = CliEnv::new(&[], &[]);
    let outside = env.local_dir.parent().unwrap().join("outside.txt");
    std::fs::write(&outside, "external private content\n").unwrap();
    place_symlink(&env.local_dir, "link.txt", outside.to_str().unwrap());
    place_symlink(&env.remote_dir, "link.txt", outside.to_str().unwrap());

    let output = env
        .cmd_with("diff")
        .args(["link.txt", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result["files"][0]["link_targets"]["left"],
        outside.to_str().unwrap()
    );
    assert_eq!(
        result["files"][0]["link_targets"]["right"],
        outside.to_str().unwrap()
    );
    assert_eq!(result["errors"][0]["path"], "link.txt");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("external private content"));
}

// @kotowari[EX-cli-049]
#[test]
fn sensitive_target_contents_remain_hidden_through_an_ordinary_link_name() {
    let env = CliEnv::new(
        &[(".env", "TEST_SECRET=left-example\n")],
        &[(".env", "TEST_SECRET=right-example\n")],
    );
    place_symlink(&env.local_dir, "link.txt", ".env");
    place_symlink(&env.remote_dir, "link.txt", ".env");

    for format in ["text", "json"] {
        let output = env
            .cmd_with("diff")
            .args(["link.txt", "--format", format])
            .output()
            .unwrap();
        let body = String::from_utf8_lossy(&output.stdout);
        assert!(
            !body.contains("left-example") && !body.contains("right-example"),
            "{output:?}"
        );
        assert!(body.contains(".env"), "{output:?}");
    }
}

// @kotowari[EX-cli-050]
#[test]
fn force_explicitly_shows_sensitive_link_target_changes() {
    let env = CliEnv::new(
        &[(".env", "TEST_SECRET=left-example\n")],
        &[(".env", "TEST_SECRET=right-example\n")],
    );
    place_symlink(&env.local_dir, "link.txt", ".env");
    place_symlink(&env.remote_dir, "link.txt", ".env");
    let output = env
        .cmd_with("diff")
        .args(["link.txt", "--force"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(
        body.contains("left-example") && body.contains("right-example"),
        "{output:?}"
    );
}

// @kotowari[EX-cli-053]
#[test]
fn matching_links_with_matching_contents_have_no_difference() {
    let env = CliEnv::new(&[("target.txt", "same\n")], &[("target.txt", "same\n")]);
    place_symlink(&env.local_dir, "link.txt", "target.txt");
    place_symlink(&env.remote_dir, "link.txt", "target.txt");

    let output = env.cmd_with("diff").arg("link.txt").output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("0 file(s) with changes"));
}

// @kotowari[EX-cli-059]
#[test]
fn external_file_content_is_compared_only_with_follow_flag() {
    let env = CliEnv::new(&[], &[]);
    let base = env.local_dir.parent().unwrap();
    std::fs::write(base.join("left-outside.txt"), "outside left\n").unwrap();
    std::fs::write(base.join("right-outside.txt"), "outside right\n").unwrap();
    place_symlink(
        &env.local_dir,
        "link.txt",
        base.join("left-outside.txt").to_str().unwrap(),
    );
    place_symlink(
        &env.remote_dir,
        "link.txt",
        base.join("right-outside.txt").to_str().unwrap(),
    );

    let output = env
        .cmd_with("diff")
        .args(["link.txt", "--follow-external-links"])
        .output()
        .unwrap();
    let body = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        body.contains("outside left") && body.contains("outside right"),
        "{output:?}"
    );
}

// @kotowari[REQ-cli-026]
#[test]
fn a_parent_link_cannot_read_outside_root_without_follow_flag() {
    let env = CliEnv::new(&[], &[]);
    let external = env.local_dir.parent().unwrap().join("external-directory");
    std::fs::create_dir_all(&external).unwrap();
    std::fs::write(external.join("child.txt"), "outside child content\n").unwrap();
    place_symlink(&env.local_dir, "shared", external.to_str().unwrap());
    place_symlink(&env.remote_dir, "shared", external.to_str().unwrap());

    let output = env
        .cmd_with("diff")
        .arg("shared/child.txt")
        .output()
        .unwrap();
    let body = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(!body.contains("outside child content"), "{output:?}");
    assert!(body.contains("not compared"), "{output:?}");
}

// @kotowari[EX-cli-048]
#[test]
fn broken_link_keeps_other_diffs_and_reports_unreadable_target() {
    let env = CliEnv::new(&[("good.txt", "left\n")], &[("good.txt", "right\n")]);
    place_symlink(&env.local_dir, "link.txt", "missing.txt");
    place_symlink(&env.remote_dir, "link.txt", "missing.txt");

    let output = env
        .cmd_with("diff")
        .args(["link.txt", "good.txt", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let link = result["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "link.txt")
        .expect("link result missing");
    assert_eq!(link["link_targets"]["left"], "missing.txt");
    assert!(result["files"].to_string().contains("right"), "{result}");
    assert_eq!(result["errors"][0]["path"], "link.txt");
    assert!(
        result["errors"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("unreadable"),
        "{result}"
    );
}

// @kotowari[EX-cli-054]
#[test]
fn one_sided_file_link_shows_available_content_without_read_error() {
    let env = CliEnv::new(&[("target.txt", "available body\n")], &[]);
    place_symlink(&env.local_dir, "link.txt", "target.txt");
    let output = env.cmd_with("diff").arg("link.txt").output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(
        body.contains("target.txt") && body.contains("available body"),
        "{output:?}"
    );
}

// @kotowari[EX-cli-042]
#[test]
fn external_binary_links_report_distinct_targets_and_sha256_hashes() {
    let env = CliEnv::new(&[], &[]);
    let outside = env.local_dir.parent().unwrap();
    let left = outside.join("left.bin");
    let right = outside.join("right.bin");
    std::fs::write(&left, [0, 1, 255]).unwrap();
    std::fs::write(&right, [0, 2, 255]).unwrap();
    place_symlink(&env.local_dir, "link.bin", left.to_str().unwrap());
    place_symlink(&env.remote_dir, "link.bin", right.to_str().unwrap());

    let output = env
        .cmd_with("diff")
        .args(["link.bin", "--follow-external-links", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let file = &result["files"][0];
    assert_eq!(file["link_targets"]["left"], left.to_str().unwrap());
    assert_eq!(file["link_targets"]["right"], right.to_str().unwrap());
    let left_hash = file["left_hash"].as_str().expect("missing left SHA-256");
    let right_hash = file["right_hash"].as_str().expect("missing right SHA-256");
    assert_eq!(left_hash.len(), 64);
    assert_eq!(right_hash.len(), 64);
    assert_ne!(left_hash, right_hash);
}

// @kotowari[EX-cli-042]
#[test]
fn binary_link_text_shows_both_link_names_and_binary_hashes() {
    let env = CliEnv::new(&[], &[]);
    let outside = env.local_dir.parent().unwrap();
    let left = outside.join("left.bin");
    let right = outside.join("right.bin");
    std::fs::write(&left, [0, 1, 255]).unwrap();
    std::fs::write(&right, [0, 2, 255]).unwrap();
    place_symlink(&env.local_dir, "link.bin", left.to_str().unwrap());
    place_symlink(&env.remote_dir, "link.bin", right.to_str().unwrap());

    let output = env
        .cmd_with("diff")
        .args(["link.bin", "--follow-external-links"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(
        body.contains("left.bin") && body.contains("right.bin"),
        "{output:?}"
    );
    assert!(body.contains("sha256="), "{output:?}");
}

// @kotowari[REQ-cli-023]
#[test]
fn nested_link_to_sensitive_file_does_not_show_resolved_contents() {
    let env = CliEnv::new(
        &[(".env", "TEST_SECRET=left-example\n")],
        &[(".env", "TEST_SECRET=right-example\n")],
    );
    place_symlink(&env.local_dir, "inner.txt", ".env");
    place_symlink(&env.remote_dir, "inner.txt", ".env");
    place_symlink(&env.local_dir, "link.txt", "inner.txt");
    place_symlink(&env.remote_dir, "link.txt", "inner.txt");

    for format in ["text", "json"] {
        let output = env
            .cmd_with("diff")
            .args(["link.txt", "--format", format])
            .output()
            .unwrap();
        let body = String::from_utf8_lossy(&output.stdout);
        assert!(
            !body.contains("left-example") && !body.contains("right-example"),
            "{output:?}"
        );
    }
}

// @kotowari[EX-cli-043]
#[test]
fn directory_links_report_link_names_and_children_under_entry_path() {
    let env = CliEnv::new(&[("left-dir/child.txt", "left body\n")], &[]);
    place_files(&env.remote_dir, &[("right-dir/child.txt", "right body\n")]);
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");

    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let files = result["files"].as_array().expect("diff files missing");
    assert!(
        files.iter().any(|f| f["path"] == "shared"
            && f["link_targets"]["left"] == "left-dir"
            && f["link_targets"]["right"] == "right-dir"),
        "{result}"
    );
    assert!(
        files.iter().any(|f| f["path"] == "shared/child.txt"
            && f["hunks"].to_string().contains("left body")
            && f["hunks"].to_string().contains("right body")),
        "{result}"
    );
}

// @kotowari[EX-cli-044]
#[test]
fn nested_directory_link_cycle_keeps_readable_child_diff_and_reports_cycle() {
    let env = CliEnv::new(&[("left-dir/good.txt", "left body\n")], &[]);
    place_files(&env.remote_dir, &[("right-dir/good.txt", "right body\n")]);
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");
    place_symlink(&env.local_dir, "left-dir/loop", ".");
    place_symlink(&env.remote_dir, "right-dir/loop", ".");

    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json", "--max-entries", "20"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["files"].to_string().contains("left body"),
        "{result}"
    );
    assert!(result["errors"].to_string().contains("cycle"), "{result}");
}

// @kotowari[REQ-cli-021]
#[test]
fn cycle_below_plain_subdirectory_is_reported_before_entry_limit() {
    let env = CliEnv::new(&[("left-dir/sub/good.txt", "left\n")], &[]);
    place_files(&env.remote_dir, &[("right-dir/sub/good.txt", "right\n")]);
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");
    place_symlink(&env.local_dir, "left-dir/sub/loop", "..");
    place_symlink(&env.remote_dir, "right-dir/sub/loop", "..");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--max-entries", "20", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result["errors"].to_string().contains("cycle"), "{result}");
    assert!(
        result["files"].to_string().contains("shared/sub/good.txt"),
        "{result}"
    );
}

// @kotowari[EX-cli-045]
#[test]
fn directory_link_entry_limit_counts_children_across_nested_links() {
    let env = CliEnv::new(&[("left-dir/a.txt", "left\n")], &[]);
    place_files(&env.remote_dir, &[("right-dir/a.txt", "right\n")]);
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");
    place_files(
        &env.local_dir,
        &[
            ("second-dir/b.txt", "second left\n"),
            ("second-dir/c.txt", "second left\n"),
        ],
    );
    place_files(
        &env.remote_dir,
        &[
            ("second-dir/b.txt", "second right\n"),
            ("second-dir/c.txt", "second right\n"),
        ],
    );
    place_symlink(&env.local_dir, "left-dir/next", "../second-dir");
    place_symlink(&env.remote_dir, "right-dir/next", "../second-dir");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--max-entries", "3", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["errors"].to_string().contains("entry limit"),
        "{result}"
    );
    assert!(result["files"].to_string().contains("left"), "{result}");
}

// @kotowari[EX-cli-056]
#[test]
fn one_sided_directory_link_shows_child_contents_without_read_error() {
    let env = CliEnv::new(&[("actual/child.txt", "left child\n")], &[]);
    place_symlink(&env.local_dir, "shared", "actual");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["files"].to_string().contains("shared/child.txt"),
        "{result}"
    );
    assert!(
        result["files"].to_string().contains("left child"),
        "{result}"
    );
    assert!(result.get("errors").is_none(), "{result}");
}

// @kotowari[EX-cli-061]
#[test]
fn directory_link_against_regular_file_shows_both_kinds_of_content() {
    let env = CliEnv::new(
        &[("actual/child.txt", "left child\n")],
        &[("shared", "right body\n")],
    );
    place_symlink(&env.local_dir, "shared", "actual");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["files"].to_string().contains("left child"),
        "{result}"
    );
    assert!(
        result["files"].to_string().contains("right body"),
        "{result}"
    );
    assert!(
        result["files"].to_string().contains("shared/child.txt"),
        "{result}"
    );
}

// @kotowari[EX-cli-046]
#[test]
fn directory_link_against_plain_directory_keeps_link_and_child_results() {
    let env = CliEnv::new(
        &[("actual/child.txt", "left child\n")],
        &[("shared/child.txt", "right child\n")],
    );
    place_symlink(&env.local_dir, "shared", "actual");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "shared"
                && file["link_targets"]["left"] == "actual"
                && file["link_targets"]["right"].is_null()
                && file["note"].as_str().unwrap_or("").contains("directory")),
        "{result}"
    );
    assert!(
        result["files"].to_string().contains("right child"),
        "{result}"
    );
}

// @kotowari[EX-cli-057]
#[test]
fn different_directory_link_names_count_as_change_even_if_children_match() {
    let env = CliEnv::new(&[("left-dir/child.txt", "same\n")], &[]);
    place_files(&env.remote_dir, &[("right-dir/child.txt", "same\n")]);
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["summary"]["files_with_changes"], 1, "{result}");
    let files = result["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{result}");
    assert_eq!(files[0]["path"], "shared");
}

// @kotowari[REQ-cli-025, REQ-cli-026]
#[test]
fn trailing_slash_does_not_bypass_external_directory_link_guard() {
    let env = CliEnv::new(&[], &[]);
    let external = env.local_dir.parent().unwrap().join("outside-dir");
    std::fs::create_dir_all(&external).unwrap();
    std::fs::write(external.join("child.txt"), "private outside content\n").unwrap();
    place_symlink(&env.local_dir, "shared", external.to_str().unwrap());
    place_symlink(&env.remote_dir, "shared", external.to_str().unwrap());
    for path in ["shared", "shared/"] {
        let output = env
            .cmd_with("diff")
            .args([path, "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{path}: {output:?}");
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["errors"][0]["path"], "shared", "{result}");
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("private outside content"),
            "{output:?}"
        );
    }
}

// @kotowari[EX-cli-051]
#[test]
fn directory_spelling_with_or_without_slash_finds_same_child_diff() {
    let env = CliEnv::new(&[("src/app.rs", "left\n")], &[("src/app.rs", "right\n")]);
    let plain = env.cmd_with("diff").arg("src").output().unwrap();
    let slash = env.cmd_with("diff").arg("src/").output().unwrap();
    assert_eq!(plain.status.code(), Some(1), "{plain:?}");
    assert_eq!(slash.status.code(), plain.status.code(), "{slash:?}");
    assert_eq!(plain.stdout, slash.stdout);
    assert!(
        String::from_utf8_lossy(&plain.stdout).contains("src/app.rs"),
        "{plain:?}"
    );
}

// @kotowari[EX-cli-052]
#[test]
fn matching_directory_contents_have_no_diff_with_either_spelling() {
    let env = CliEnv::new(&[("src/app.rs", "same\n")], &[("src/app.rs", "same\n")]);
    for path in ["src", "src/"] {
        let output = env.cmd_with("diff").arg(path).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("0 file(s) with changes"),
            "{output:?}"
        );
    }
}

// @kotowari[REQ-cli-025]
#[test]
fn empty_directory_has_same_no_diff_result_with_or_without_slash() {
    let env = CliEnv::new(&[], &[]);
    std::fs::create_dir(env.local_dir.join("empty")).unwrap();
    std::fs::create_dir(env.remote_dir.join("empty")).unwrap();
    for path in ["empty", "empty/"] {
        let output = env.cmd_with("diff").arg(path).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{path}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("0 file(s) with changes"),
            "{output:?}"
        );
    }
}

// @kotowari[EX-cli-060]
#[test]
fn directory_link_spelling_keeps_link_and_child_changes() {
    let env = CliEnv::new(&[("left-dir/a.txt", "left\n")], &[]);
    place_files(&env.remote_dir, &[("right-dir/a.txt", "right\n")]);
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");
    let plain = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    let slash = env
        .cmd_with("diff")
        .args(["shared/", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(plain.status.code(), Some(1), "{plain:?}");
    assert_eq!(slash.status.code(), plain.status.code(), "{slash:?}");
    assert_eq!(plain.stdout, slash.stdout);
    let result: serde_json::Value = serde_json::from_slice(&plain.stdout).unwrap();
    assert!(
        result["files"].to_string().contains("shared/a.txt"),
        "{result}"
    );
    assert!(result["files"].to_string().contains("left-dir"), "{result}");
}

// @kotowari[EX-cli-047]
#[test]
fn unreadable_child_of_directory_link_keeps_other_child_diffs() {
    let env = CliEnv::new(&[("left-dir/good.txt", "left\n")], &[]);
    place_files(&env.remote_dir, &[("right-dir/good.txt", "right\n")]);
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");
    place_symlink(&env.local_dir, "left-dir/broken.txt", "missing.txt");
    place_symlink(&env.remote_dir, "right-dir/broken.txt", "missing.txt");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["files"].to_string().contains("shared/good.txt"),
        "{result}"
    );
    assert_eq!(result["errors"][0]["path"], "shared/broken.txt", "{result}");
}

// @kotowari[REQ-cli-023]
#[test]
fn sensitive_intermediate_link_name_masks_even_when_final_name_is_public() {
    let env = CliEnv::new(
        &[("public.txt", "left private body\n")],
        &[("public.txt", "right private body\n")],
    );
    for root in [&env.local_dir, &env.remote_dir] {
        place_symlink(root, ".env", "public.txt");
        place_symlink(root, "inner.txt", ".env");
        place_symlink(root, "outer.txt", "inner.txt");
    }
    let output = env
        .cmd_with("diff")
        .args(["outer.txt", "--format", "json"])
        .output()
        .unwrap();
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(
        !body.contains("left private body") && !body.contains("right private body"),
        "{output:?}"
    );
}

// @kotowari[EX-cli-055]
#[test]
fn external_directory_nested_secret_stays_hidden_without_force() {
    let env = CliEnv::new(&[], &[]);
    let outside = env.local_dir.parent().unwrap().join("outside-dir");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join(".env"), "TEST_SECRET=outside-example\n").unwrap();
    place_symlink(&outside, "nested.txt", ".env");
    place_symlink(&env.local_dir, "shared", outside.to_str().unwrap());
    place_symlink(&env.remote_dir, "shared", outside.to_str().unwrap());
    let output = env
        .cmd_with("diff")
        .args(["shared", "--follow-external-links", "--format", "json"])
        .output()
        .unwrap();
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(!body.contains("outside-example"), "{output:?}");
    assert!(body.contains("shared/nested.txt"), "{output:?}");
}

// @kotowari[EX-cli-039]
#[test]
fn json_diff_separates_link_names_from_resolved_text_changes() {
    let env = CliEnv::new(
        &[("left.txt", "left body\n")],
        &[("right.txt", "right body\n")],
    );
    place_symlink(&env.local_dir, "link.txt", "left.txt");
    place_symlink(&env.remote_dir, "link.txt", "right.txt");

    let output = env
        .cmd_with("diff")
        .args(["link.txt", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let file = &result["files"][0];
    assert_eq!(file["link_targets"]["left"], "left.txt");
    assert_eq!(file["link_targets"]["right"], "right.txt");
    let hunks = file["hunks"].to_string();
    assert!(
        hunks.contains("left body") && hunks.contains("right body"),
        "{file}"
    );
    assert!(
        !hunks.contains("left.txt") && !hunks.contains("right.txt"),
        "{file}"
    );
}

fn json_item<'a>(result: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    result["files"]
        .as_array()
        .expect("diff files missing")
        .iter()
        .find(|file| file["path"] == path)
        .unwrap_or_else(|| panic!("{path} missing: {result}"))
}

fn json_error<'a>(result: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    result["errors"]
        .as_array()
        .expect("diff errors missing")
        .iter()
        .find(|error| error["path"] == path)
        .unwrap_or_else(|| panic!("error for {path} missing: {result}"))
}

fn assert_reason_given(error: &serde_json::Value, result: &serde_json::Value) {
    assert!(
        error["reason"]
            .as_str()
            .is_some_and(|reason| !reason.trim().is_empty()),
        "{result}"
    );
}

fn assert_reported_incomplete(result: &serde_json::Value) {
    let errors = result["errors"].as_array().expect("diff errors missing");
    assert!(!errors.is_empty(), "{result}");
    for error in errors {
        assert_reason_given(error, result);
    }
}

// @kotowari[EX-cli-039]
#[test]
fn text_diff_shows_link_targets_and_resolved_contents_on_separate_lines() {
    let env = CliEnv::new(&[("target.txt", "left content\n")], &[]);
    place_symlink(&env.local_dir, "link.txt", "target.txt");
    place_files(&env.remote_dir, &[("other.txt", "right content\n")]);
    place_symlink(&env.remote_dir, "link.txt", "other.txt");

    let output = env.cmd_with("diff").arg("link.txt").output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    let link_names = ["target.txt", "other.txt"];
    let contents = ["left content", "right content"];
    for wanted in link_names.iter().chain(&contents) {
        assert!(body.contains(wanted), "{wanted:?} missing: {body}");
    }
    for line in body.lines() {
        let names_link = link_names.iter().any(|name| line.contains(name));
        let shows_content = contents.iter().any(|content| line.contains(content));
        assert!(
            !(names_link && shows_content),
            "{line:?} mixes both: {body}"
        );
    }
}

// @kotowari[REQ-cli-024]
#[test]
fn json_link_item_keeps_the_symlink_flag() {
    let env = CliEnv::new(
        &[("target.txt", "left body\n")],
        &[("target.txt", "right body\n")],
    );
    place_symlink(&env.local_dir, "link.txt", "target.txt");
    place_symlink(&env.remote_dir, "link.txt", "target.txt");
    let output = env
        .cmd_with("diff")
        .args(["link.txt", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let link = json_item(&result, "link.txt");
    assert_eq!(link["symlink"], true, "{result}");
    assert_eq!(link["link_targets"]["left"], "target.txt", "{result}");
    assert_eq!(link["link_targets"]["right"], "target.txt", "{result}");
}

// @kotowari[EX-cli-042]
#[test]
fn external_binary_link_hashes_are_sha256_of_the_target_contents() {
    let env = CliEnv::new(&[], &[]);
    let outside = env.local_dir.parent().unwrap();
    let left_bytes = [0u8, 1, 255];
    let right_bytes = [0u8, 2, 255];
    std::fs::write(outside.join("left.bin"), left_bytes).unwrap();
    std::fs::write(outside.join("right.bin"), right_bytes).unwrap();
    place_symlink(
        &env.local_dir,
        "link.bin",
        outside.join("left.bin").to_str().unwrap(),
    );
    place_symlink(
        &env.remote_dir,
        "link.bin",
        outside.join("right.bin").to_str().unwrap(),
    );

    let output = env
        .cmd_with("diff")
        .args(["link.bin", "--follow-external-links", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let link = json_item(&result, "link.bin");
    assert_eq!(
        link["left_hash"],
        remote_merge::diff::binary::compute_sha256(&left_bytes),
        "{result}"
    );
    assert_eq!(
        link["right_hash"],
        remote_merge::diff::binary::compute_sha256(&right_bytes),
        "{result}"
    );
}

// @kotowari[EX-cli-045]
#[test]
fn directory_link_entry_limit_keeps_the_child_diff_read_before_the_limit() {
    let env = CliEnv::new(&[("left-dir/a.txt", "left\n")], &[]);
    place_files(&env.remote_dir, &[("right-dir/a.txt", "right\n")]);
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");
    for (root, body) in [
        (&env.local_dir, "second left\n"),
        (&env.remote_dir, "second right\n"),
    ] {
        place_files(
            root,
            &[("second-dir/b.txt", body), ("second-dir/c.txt", body)],
        );
    }
    place_symlink(&env.local_dir, "left-dir/next", "../second-dir");
    place_symlink(&env.remote_dir, "right-dir/next", "../second-dir");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--max-entries", "3", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_reported_incomplete(&result);
    let child_hunks = json_item(&result, "shared/a.txt")["hunks"].to_string();
    assert!(
        child_hunks.contains("\"left\"") && child_hunks.contains("\"right\""),
        "{result}"
    );
}

// @kotowari[EX-cli-046]
#[test]
fn directory_link_against_plain_directory_compares_the_child_read_through_the_link() {
    let env = CliEnv::new(
        &[("actual/child.txt", "left child\n")],
        &[("shared/child.txt", "right child\n")],
    );
    place_symlink(&env.local_dir, "shared", "actual");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let child_hunks = json_item(&result, "shared/child.txt")["hunks"].to_string();
    assert!(
        child_hunks.contains("left child") && child_hunks.contains("right child"),
        "{result}"
    );
}

// @kotowari[EX-cli-061]
#[test]
fn directory_link_against_regular_file_reports_the_link_target() {
    let env = CliEnv::new(
        &[("actual/child.txt", "left child\n")],
        &[("shared", "right body\n")],
    );
    place_symlink(&env.local_dir, "shared", "actual");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let link = json_item(&result, "shared");
    assert_eq!(link["link_targets"]["left"], "actual", "{result}");
    assert!(link["link_targets"]["right"].is_null(), "{result}");
}

// @kotowari[EX-cli-056]
#[test]
fn one_sided_directory_link_reports_its_link_target_and_null_for_the_missing_side() {
    let env = CliEnv::new(&[("actual/child.txt", "left child\n")], &[]);
    place_symlink(&env.local_dir, "shared", "actual");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let link = json_item(&result, "shared");
    assert_eq!(link["link_targets"]["left"], "actual", "{result}");
    assert!(link["link_targets"]["right"].is_null(), "{result}");
}

// @kotowari[EX-cli-047]
#[test]
fn unreadable_child_of_directory_link_is_reported_with_a_reason() {
    let env = CliEnv::new(&[("left-dir/good.txt", "left\n")], &[]);
    place_files(&env.remote_dir, &[("right-dir/good.txt", "right\n")]);
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");
    place_symlink(&env.local_dir, "left-dir/broken.txt", "missing.txt");
    place_symlink(&env.remote_dir, "right-dir/broken.txt", "missing.txt");
    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_reason_given(json_error(&result, "shared/broken.txt"), &result);
}

// @kotowari[REQ-cli-023]
#[test]
fn sensitive_link_name_hides_the_target_contents_without_force() {
    let env = CliEnv::new(
        &[("settings.txt", "left private body\n")],
        &[("settings.txt", "right private body\n")],
    );
    place_symlink(&env.local_dir, ".env", "settings.txt");
    place_symlink(&env.remote_dir, ".env", "settings.txt");

    for format in ["text", "json"] {
        let output = env
            .cmd_with("diff")
            .args([".env", "--format", format])
            .output()
            .unwrap();
        let body = String::from_utf8_lossy(&output.stdout);
        assert!(body.contains(".env"), "{output:?}");
        assert!(
            !body.contains("left private body") && !body.contains("right private body"),
            "{output:?}"
        );
    }
}

// @kotowari[EX-cli-055]
#[test]
fn nested_link_from_external_directory_to_secret_shows_neither_contents_nor_hashes() {
    let env = CliEnv::new(&[], &[]);
    let base = env.local_dir.parent().unwrap().to_path_buf();
    let sides: [(&std::path::Path, &str, &[u8]); 2] = [
        (
            &env.local_dir,
            "left-outside",
            b"TEST_SECRET=left-example\0\n",
        ),
        (
            &env.remote_dir,
            "right-outside",
            b"TEST_SECRET=right-example\0\n",
        ),
    ];
    for (root, outside_name, secret) in sides {
        let outside = base.join(outside_name);
        place_binary_file(&outside, "secret/.env", secret);
        place_symlink(&outside, "shared-dir/nested", "../secret/.env");
        place_symlink(root, "shared", outside.join("shared-dir").to_str().unwrap());
    }

    let output = env
        .cmd_with("diff")
        .args(["shared", "--follow-external-links", "--format", "json"])
        .output()
        .unwrap();
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(
        !body.contains("left-example") && !body.contains("right-example"),
        "{output:?}"
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let nested = json_item(&result, "shared/nested");
    assert_eq!(nested["link_targets"]["left"], "../secret/.env", "{result}");
    assert_eq!(
        nested["link_targets"]["right"], "../secret/.env",
        "{result}"
    );
    assert!(nested["left_hash"].is_null(), "{result}");
    assert!(nested["right_hash"].is_null(), "{result}");
}

// @kotowari[REQ-cli-026]
#[test]
fn nested_link_to_outside_file_is_not_read_without_follow_flag() {
    let env = CliEnv::new(&[], &[]);
    let outside = env.local_dir.parent().unwrap().join("outside.txt");
    std::fs::write(&outside, "external private content\n").unwrap();
    for root in [&env.local_dir, &env.remote_dir] {
        std::fs::create_dir_all(root.join("actual")).unwrap();
        place_symlink(root, "actual/nested.txt", outside.to_str().unwrap());
        place_symlink(root, "shared", "actual");
    }

    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("external private content"),
        "{output:?}"
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let nested = json_item(&result, "shared/nested.txt");
    assert_eq!(
        nested["link_targets"]["left"],
        outside.to_str().unwrap(),
        "{result}"
    );
    assert_reason_given(json_error(&result, "shared/nested.txt"), &result);
}

// @kotowari[REQ-cli-026]
#[test]
fn force_does_not_read_an_external_link_without_follow_flag() {
    let env = CliEnv::new(&[], &[]);
    let outside = env.local_dir.parent().unwrap().join("outside.txt");
    std::fs::write(&outside, "external private content\n").unwrap();
    place_symlink(&env.local_dir, "link.txt", outside.to_str().unwrap());
    place_symlink(&env.remote_dir, "link.txt", outside.to_str().unwrap());

    let output = env
        .cmd_with("diff")
        .args(["link.txt", "--force", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("external private content"),
        "{output:?}"
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_reason_given(json_error(&result, "link.txt"), &result);
}

// @kotowari[REQ-cli-026]
#[test]
fn parent_directory_path_is_rejected_even_with_follow_flag() {
    let env = CliEnv::new(&[], &[]);
    let outside = env.local_dir.parent().unwrap().join("outside.txt");
    std::fs::write(&outside, "external private content\n").unwrap();

    let output = env
        .cmd_with("diff")
        .args(["../outside.txt", "--follow-external-links"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("traversal"),
        "{output:?}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("external private content"),
        "{output:?}"
    );
}

// @kotowari[REQ-cli-026]
#[test]
fn one_side_reaching_outside_through_a_directory_link_is_not_compared_without_follow_flag() {
    let env = CliEnv::new(&[], &[("inside-dir/child.txt", "same body\n")]);
    let outside = env.local_dir.parent().unwrap().join("outside-dir");
    place_files(&outside, &[("child.txt", "same body\n")]);
    place_symlink(&env.local_dir, "shared", outside.to_str().unwrap());
    place_symlink(&env.remote_dir, "shared", "inside-dir");

    let output = env
        .cmd_with("diff")
        .args(["shared/child.txt", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_reason_given(json_error(&result, "shared/child.txt"), &result);
}

// @kotowari[REQ-cli-026]
#[test]
fn one_sided_external_link_reports_its_link_target_without_follow_flag() {
    let env = CliEnv::new(&[], &[("link.txt", "inside body\n")]);
    let outside = env.local_dir.parent().unwrap().join("outside.txt");
    std::fs::write(&outside, "external private content\n").unwrap();
    place_symlink(&env.local_dir, "link.txt", outside.to_str().unwrap());

    let output = env
        .cmd_with("diff")
        .args(["link.txt", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("external private content"),
        "{output:?}"
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let link = json_item(&result, "link.txt");
    assert_eq!(
        link["link_targets"]["left"],
        outside.to_str().unwrap(),
        "{result}"
    );
    assert!(link["link_targets"]["right"].is_null(), "{result}");
    assert_reason_given(json_error(&result, "link.txt"), &result);
}

// @kotowari[EX-cli-044]
#[test]
fn cycle_on_one_side_of_a_directory_link_is_reported() {
    let env = CliEnv::new(&[("left-dir/good.txt", "left body\n")], &[]);
    place_files(&env.remote_dir, &[("right-dir/good.txt", "right body\n")]);
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");
    place_symlink(&env.local_dir, "left-dir/loop", ".");

    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json", "--max-entries", "20"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_reason_given(json_error(&result, "shared/loop"), &result);
    let child_hunks = json_item(&result, "shared/good.txt")["hunks"].to_string();
    assert!(
        child_hunks.contains("left body") && child_hunks.contains("right body"),
        "{result}"
    );
}

// @kotowari[REQ-cli-021]
#[test]
fn plain_subdirectory_entries_under_a_directory_link_count_toward_the_entry_limit() {
    let children = [
        ("left-dir/sub/a.txt", "left\n"),
        ("left-dir/sub/b.txt", "left\n"),
        ("left-dir/sub/c.txt", "left\n"),
        ("left-dir/sub/d.txt", "left\n"),
    ];
    let env = CliEnv::new(&children, &[]);
    place_files(
        &env.remote_dir,
        &[
            ("right-dir/sub/a.txt", "right\n"),
            ("right-dir/sub/b.txt", "right\n"),
            ("right-dir/sub/c.txt", "right\n"),
            ("right-dir/sub/d.txt", "right\n"),
        ],
    );
    place_symlink(&env.local_dir, "shared", "left-dir");
    place_symlink(&env.remote_dir, "shared", "right-dir");

    let output = env
        .cmd_with("diff")
        .args(["shared", "--max-entries", "3", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_reported_incomplete(&result);
}

// @kotowari[REQ-cli-021]
#[test]
fn entries_up_to_the_limit_under_a_directory_link_are_compared_completely() {
    let env = CliEnv::new(
        &[
            ("files-left/a.txt", "left\n"),
            ("files-left/b.txt", "left\n"),
            ("files-left/c.txt", "left\n"),
            ("nested-left/sub/a.txt", "left\n"),
            ("nested-left/sub/b.txt", "left\n"),
        ],
        &[
            ("files-right/a.txt", "right\n"),
            ("files-right/b.txt", "right\n"),
            ("files-right/c.txt", "right\n"),
            ("nested-right/sub/a.txt", "right\n"),
            ("nested-right/sub/b.txt", "right\n"),
        ],
    );
    place_symlink(&env.local_dir, "files", "files-left");
    place_symlink(&env.remote_dir, "files", "files-right");
    place_symlink(&env.local_dir, "nested", "nested-left");
    place_symlink(&env.remote_dir, "nested", "nested-right");

    for (path, children) in [
        (
            "files",
            ["files/a.txt", "files/b.txt", "files/c.txt"].as_slice(),
        ),
        (
            "nested",
            ["nested/sub/a.txt", "nested/sub/b.txt"].as_slice(),
        ),
    ] {
        let output = env
            .cmd_with("diff")
            .args([path, "--max-entries", "3", "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{path}: {output:?}");
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(result.get("errors").is_none(), "{result}");
        for child in children {
            json_item(&result, child);
        }
    }
}

// @kotowari[REQ-cli-023]
#[test]
fn sensitive_link_chain_on_one_side_hides_its_contents() {
    let env = CliEnv::new(
        &[(".env", "TEST_SECRET=left-example\n")],
        &[("public.txt", "right public body\n")],
    );
    place_symlink(&env.local_dir, "inner.txt", ".env");
    place_symlink(&env.remote_dir, "inner.txt", "public.txt");
    place_symlink(&env.local_dir, "link.txt", "inner.txt");
    place_symlink(&env.remote_dir, "link.txt", "inner.txt");

    for format in ["text", "json"] {
        let output = env
            .cmd_with("diff")
            .args(["link.txt", "--format", format])
            .output()
            .unwrap();
        let body = String::from_utf8_lossy(&output.stdout);
        assert!(!body.contains("left-example"), "{output:?}");
        if format == "json" {
            let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            json_item(&result, "link.txt");
        } else {
            assert!(body.contains("link.txt"), "{output:?}");
        }
    }
}

// @kotowari[REQ-cli-020]
#[test]
fn directory_link_against_binary_file_reports_the_file_hash() {
    let env = CliEnv::new(&[("actual/child.txt", "left child\n")], &[]);
    let right_bytes = b"right\0body\n";
    place_binary_file(&env.remote_dir, "shared", right_bytes);
    place_symlink(&env.local_dir, "shared", "actual");

    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let link = json_item(&result, "shared");
    assert_eq!(
        link["right_hash"],
        remote_merge::diff::binary::compute_sha256(right_bytes),
        "{result}"
    );
}

// @kotowari[REQ-cli-020]
#[test]
fn link_to_binary_against_link_to_text_reports_both_hashes() {
    let env = CliEnv::new(&[], &[("text.txt", "right text\n")]);
    let left_bytes = b"left\0binary\n";
    place_binary_file(&env.local_dir, "data.bin", left_bytes);
    place_symlink(&env.local_dir, "link.dat", "data.bin");
    place_symlink(&env.remote_dir, "link.dat", "text.txt");

    let output = env
        .cmd_with("diff")
        .args(["link.dat", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let link = json_item(&result, "link.dat");
    assert_eq!(
        link["left_hash"],
        remote_merge::diff::binary::compute_sha256(left_bytes),
        "{result}"
    );
}

// @kotowari[EX-cli-048]
#[test]
fn broken_link_on_the_local_side_is_an_error_even_when_the_other_side_reads() {
    let env = CliEnv::new(&[("good.txt", "left\n")], &[("good.txt", "right\n")]);
    place_files(&env.remote_dir, &[("present.txt", "right target body\n")]);
    place_symlink(&env.local_dir, "link.txt", "missing.txt");
    place_symlink(&env.remote_dir, "link.txt", "present.txt");

    let output = env
        .cmd_with("diff")
        .args(["link.txt", "good.txt", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_reason_given(json_error(&result, "link.txt"), &result);
    let good_hunks = json_item(&result, "good.txt")["hunks"].to_string();
    assert!(
        good_hunks.contains("left") && good_hunks.contains("right"),
        "{result}"
    );
}

// @kotowari[EX-cli-044]
#[test]
fn returning_to_a_traversed_directory_on_one_side_is_reported_as_a_cycle() {
    let env = CliEnv::new(&[("left-dir/sub/good.txt", "left body\n")], &[]);
    place_files(
        &env.remote_dir,
        &[("right-dir/sub/good.txt", "right body\n")],
    );
    place_symlink(&env.local_dir, "shared", "left-dir/sub");
    place_symlink(&env.remote_dir, "shared", "right-dir/sub");
    place_symlink(&env.local_dir, "left-dir/sub/up", "..");

    let output = env
        .cmd_with("diff")
        .args(["shared", "--format", "json", "--max-entries", "20"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_reason_given(json_error(&result, "shared/up/sub"), &result);
    let child_hunks = json_item(&result, "shared/good.txt")["hunks"].to_string();
    assert!(
        child_hunks.contains("left body") && child_hunks.contains("right body"),
        "{result}"
    );
}

// @kotowari[REQ-cli-023]
#[test]
fn sensitive_intermediate_link_reached_through_dot_components_hides_contents() {
    let env = CliEnv::new(
        &[("public.txt", "left private body\n")],
        &[("public.txt", "right private body\n")],
    );
    for root in [&env.local_dir, &env.remote_dir] {
        std::fs::create_dir_all(root.join("sub")).unwrap();
        place_symlink(root, ".env", "public.txt");
        place_symlink(root, "inner.txt", ".env");
        place_symlink(root, "dot.txt", "./inner.txt");
        place_symlink(root, "parent.txt", "sub/../inner.txt");
    }

    for path in ["dot.txt", "parent.txt"] {
        let output = env
            .cmd_with("diff")
            .args([path, "--format", "json"])
            .output()
            .unwrap();
        let body = String::from_utf8_lossy(&output.stdout);
        assert!(
            !body.contains("left private body") && !body.contains("right private body"),
            "{path}: {output:?}"
        );
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        json_item(&result, path);
    }
}

// @kotowari[REQ-cli-058]
#[test]
fn text_total_counts_the_children_compared_under_a_directory_link() {
    let env = CliEnv::new(
        &[("actual/a.txt", "left a\n"), ("actual/b.txt", "left b\n")],
        &[("actual/a.txt", "right a\n"), ("actual/b.txt", "right b\n")],
    );
    place_symlink(&env.local_dir, "shared", "actual");
    place_symlink(&env.remote_dir, "shared", "actual");

    let output = env.cmd_with("diff").arg("shared").output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    let total: usize = body
        .lines()
        .rev()
        .find_map(|line| line.trim_end().strip_suffix(" total"))
        .and_then(|rest| rest.rsplit(' ').next())
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("no total in the last line: {body}"));
    assert!(total >= 2, "{body}");
}

/// 機密ファイル (.env) の diff で内容が隠され、--force の案内が表示される
// @kotowari[REQ-cli-005]
#[test]
fn test_diff_sensitive_file_warning() {
    let env = CliEnv::new(
        &[(".env", "SECRET_KEY=abc123\n")],
        &[(".env", "SECRET_KEY=xyz789\n")],
    );

    let output = env
        .cmd_with("diff")
        .arg(".env")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 実際の出力: "Content hidden (sensitive file). Use --force to show."
    assert!(
        stdout.contains("Content hidden (sensitive file). Use --force to show."),
        "Expected 'Content hidden (sensitive file). Use --force to show.' in output, got: {}",
        stdout
    );
    // 機密内容は表示されないことを確認
    assert!(
        !stdout.contains("abc123") && !stdout.contains("xyz789"),
        "Sensitive content should not be shown without --force, got: {}",
        stdout
    );
}

/// 機密ファイルに --force を付けると内容が表示される
// @kotowari[REQ-cli-005]
#[test]
fn test_diff_sensitive_file_force() {
    let env = CliEnv::new(
        &[(".env", "SECRET_KEY=abc123\n")],
        &[(".env", "SECRET_KEY=xyz789\n")],
    );

    let output = env
        .cmd_with("diff")
        .args([".env", "--force"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 実際の出力: "-SECRET_KEY=abc123" / "+SECRET_KEY=xyz789"
    assert!(
        stdout.contains("-SECRET_KEY=abc123") && stdout.contains("+SECRET_KEY=xyz789"),
        "With --force, sensitive diff content should be shown, got: {}",
        stdout
    );
}

/// 末尾スラッシュの有無で同じ結果が得られる（パス正規化）
// @kotowari[EX-cli-051]
#[test]
fn test_diff_trailing_slash_normalized() {
    let env = CliEnv::new(
        &[("src/app.rs", "fn app() {}\n")],
        &[("src/app.rs", "fn app() { changed }\n")],
    );

    let output_with_slash = env
        .cmd_with("diff")
        .arg("src/")
        .output()
        .expect("failed to execute");

    let output_without_slash = env
        .cmd_with("diff")
        .arg("src")
        .output()
        .expect("failed to execute");

    let stdout_with = String::from_utf8_lossy(&output_with_slash.stdout);
    let stdout_without = String::from_utf8_lossy(&output_without_slash.stdout);
    assert_eq!(
        stdout_with, stdout_without,
        "Trailing slash should not affect diff result"
    );
}

/// テキスト中に NUL バイトを含むファイルがバイナリとして検出され SHA-256 表示される
// @kotowari[REQ-cli-009]
#[test]
fn test_diff_null_bytes_detected_as_binary() {
    let env = CliEnv::new(&[], &[]);
    // テキストの途中に NUL バイトを含むファイル
    let content_with_null = b"hello\x00world\n";
    place_binary_file(&env.local_dir, "mixed.dat", content_with_null);
    place_binary_file(&env.remote_dir, "mixed.dat", b"hello\x00different\n");

    let output = env
        .cmd_with("diff")
        .arg("mixed.dat")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 実際の出力: "Binary files differ (left: sha256=..., right: sha256=...)"
    assert!(
        stdout.contains("Binary files differ"),
        "File with NUL bytes should be detected as binary with 'Binary files differ', got: {}",
        stdout
    );
    assert!(
        stdout.contains("sha256="),
        "Expected 'sha256=' hash for binary file, got: {}",
        stdout
    );
}

// ─── symlink を経由する root_dir ─────────────────────────

/// root_dir を symlink にする側
enum LinkedRoot {
    Local,
    Remote,
}

/// 左右の file.txt を中身を違えて置き、それを指す link.txt も左右に置く。`linked` 側の root_dir を
/// 実ディレクトリへの symlink にした設定で `diff <path> --format json` を起動し、JSON を返す
///
/// 設定は一時ディレクトリに書いて隔離の確認を通し、`--config` で渡す。環境変数は全て消して
/// HOME・XDG の変数を一時ディレクトリの下に向け、作業ディレクトリも一時ディレクトリの下にする。
fn diff_with_linked_root(
    linked: LinkedRoot,
    path: &str,
) -> (std::process::Output, serde_json::Value) {
    diff_with_linked_root_placing(linked, path, |_, _, _| {})
}

/// `diff_with_linked_root` と同じ構成に、`place` で root_dir の中身を足してから diff を起動する
///
/// `place` は symlink にした側の実ディレクトリの実パス、その側の設定に書く root_dir（symlink）、
/// もう一方の側の root_dir を受け取る。
fn diff_with_linked_root_placing(
    linked: LinkedRoot,
    path: &str,
    place: impl FnOnce(&std::path::Path, &std::path::Path, &std::path::Path),
) -> (std::process::Output, serde_json::Value) {
    let mut dirs = TestDirs::new_2way(
        &[("file.txt", "left body\n")],
        &[("file.txt", "right body\n")],
    );
    place_symlink(&dirs.local_dir, "link.txt", "file.txt");
    place_symlink(&dirs.remote_dir, "link.txt", "file.txt");
    let temp = dirs.temp.path().to_path_buf();
    let (mut local_root, mut remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let link = temp.join("linked-root");
    let real = match linked {
        LinkedRoot::Local => std::mem::replace(&mut local_root, link.clone()),
        LinkedRoot::Remote => std::mem::replace(&mut remote_root, link.clone()),
    };
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let other = match linked {
        LinkedRoot::Local => &remote_root,
        LinkedRoot::Remote => &local_root,
    };
    place(&std::fs::canonicalize(&real).unwrap(), &link, other);
    let config_path = temp.join("linked-root-config.toml");
    let config = gen_config(&local_root, &remote_root, None, dirs.server_port());
    std::fs::write(&config_path, config).unwrap();
    dirs.assert_isolated_config_at(&config_path, &local_root);

    let home = temp.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_remote-merge"));
    cmd.env_clear();
    cmd.env("HOME", &home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("XDG_DATA_HOME", temp.join("xdg-data"));
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.current_dir(&home);
    cmd.arg("--config").arg(&config_path);
    cmd.args(["diff", path, "--format", "json"]);
    cmd.stdin(std::process::Stdio::null());
    let output = cmd.output().expect("failed to execute diff");
    let result = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {output:?}"));
    (output, result)
}

/// root_dir の中のファイルの中身の差分が出て、root_dir の外という報告がないことを確かめる
fn assert_compared_inside_root(
    output: &std::process::Output,
    result: &serde_json::Value,
    path: &str,
) {
    let file = json_item(result, path);
    let hunks = file["hunks"].to_string();
    assert!(
        hunks.contains("left body") && hunks.contains("right body"),
        "{result}"
    );
    assert!(!result.to_string().contains("outside root_dir"), "{result}");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

// @kotowari[REQ-cli-026]
#[test]
fn local_root_dir_through_a_symlink_compares_files_inside_it() {
    let (output, result) = diff_with_linked_root(LinkedRoot::Local, "file.txt");
    assert_compared_inside_root(&output, &result, "file.txt");
}

// @kotowari[REQ-cli-026]
#[test]
fn remote_root_dir_through_a_symlink_compares_files_inside_it() {
    let (output, result) = diff_with_linked_root(LinkedRoot::Remote, "file.txt");
    assert_compared_inside_root(&output, &result, "file.txt");
}

// @kotowari[REQ-cli-026]
#[test]
fn link_inside_a_root_dir_through_a_symlink_is_followed_without_follow_flag() {
    for linked in [LinkedRoot::Local, LinkedRoot::Remote] {
        let (output, result) = diff_with_linked_root(linked, "link.txt");
        assert_compared_inside_root(&output, &result, "link.txt");
    }
}

/// symlink にした側だけに、絶対パスのリンク文字列で始まり、途中の段の名前が機密パターンに当たる
/// 連鎖（a.txt → <root_dir>/mid → secret.pem → plain.txt）を置き、もう一方の側の a.txt は
/// 通常のファイルにして、--force なしで diff する
///
/// `configured_form` が true なら a.txt のリンク文字列を設定に書いた root_dir（symlink）の形に、
/// false なら実パスの形にする。
fn diff_sensitive_chain_through_absolute_link(linked: LinkedRoot, configured_form: bool) {
    let (output, result) =
        diff_with_linked_root_placing(linked, "a.txt", |real, configured, other| {
            let root = if configured_form { configured } else { real };
            place_files(real, &[("plain.txt", "chain-secret-marker\n")]);
            place_symlink(real, "secret.pem", "plain.txt");
            place_symlink(real, "mid", "secret.pem");
            place_symlink(real, "a.txt", root.join("mid").to_str().unwrap());
            place_files(other, &[("a.txt", "other body\n")]);
        });
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("chain-secret-marker"),
        "{output:?}"
    );
    json_item(&result, "a.txt");
}

// @kotowari[REQ-cli-023]
#[test]
fn sensitive_chain_link_under_a_local_root_dir_through_a_symlink_hides_contents() {
    diff_sensitive_chain_through_absolute_link(LinkedRoot::Local, false);
}

// @kotowari[REQ-cli-023]
#[test]
fn sensitive_chain_link_under_a_remote_root_dir_through_a_symlink_hides_contents() {
    diff_sensitive_chain_through_absolute_link(LinkedRoot::Remote, false);
}

// @kotowari[REQ-cli-023]
#[test]
fn sensitive_chain_link_written_through_the_configured_root_dir_hides_contents() {
    for linked in [LinkedRoot::Local, LinkedRoot::Remote] {
        diff_sensitive_chain_through_absolute_link(linked, true);
    }
}

// ─── 字面で root_dir の下に辿れない連鎖 ─────────────────

/// リンク文字列の書き方
enum ChainLinkText {
    /// "../<root_dir のディレクトリ名>/mid"
    ParentOfRoot,
    /// "<root_dir の実パス>/../<root_dir のディレクトリ名>/mid"
    RealRootThroughParent,
    /// root_dir を指す別名の symlink を通る絶対パス "<別名>/mid"
    AliasOfRoot,
}

/// `linked` の側の root_dir にだけ、実際には root_dir の中にとどまるが、最初の段のリンク文字列が
/// 字面では root_dir の外を通る連鎖（a.txt → `text` → mid → secret.pem → plain.txt）を置き、
/// もう一方の側の a.txt は通常のファイルにして --force なしの JSON で diff し、最終参照先の
/// 中身が出ないことと a.txt の項目が出ることを確かめる
fn assert_unfollowable_chain_hides_contents(linked: LinkedRoot, text: ChainLinkText) {
    let env = CliEnv::new(&[], &[]);
    let (root, other) = match linked {
        LinkedRoot::Local => (&env.local_dir, &env.remote_dir),
        LinkedRoot::Remote => (&env.remote_dir, &env.local_dir),
    };
    let name = root.file_name().unwrap().to_str().unwrap();
    let link_text = match text {
        ChainLinkText::ParentOfRoot => format!("../{name}/mid"),
        ChainLinkText::RealRootThroughParent => format!(
            "{}/../{name}/mid",
            std::fs::canonicalize(root).unwrap().display()
        ),
        ChainLinkText::AliasOfRoot => {
            let alias = env.temp_root().join(format!("{name}-alias"));
            std::os::unix::fs::symlink(root, &alias).unwrap();
            format!("{}/mid", alias.display())
        }
    };
    place_files(root, &[("plain.txt", "chain-secret-marker\n")]);
    place_symlink(root, "secret.pem", "plain.txt");
    place_symlink(root, "mid", "secret.pem");
    place_symlink(root, "a.txt", &link_text);
    place_files(other, &[("a.txt", "other body\n")]);

    let output = env
        .cmd_with("diff")
        .args(["a.txt", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("chain-secret-marker"),
        "{output:?}"
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {output:?}"));
    json_item(&result, "a.txt");
}

// @kotowari[REQ-cli-023]
#[test]
fn chain_link_through_the_parent_of_the_local_root_dir_hides_contents() {
    assert_unfollowable_chain_hides_contents(LinkedRoot::Local, ChainLinkText::ParentOfRoot);
}

// @kotowari[REQ-cli-023]
#[test]
fn chain_link_through_the_parent_of_the_remote_root_dir_hides_contents() {
    assert_unfollowable_chain_hides_contents(LinkedRoot::Remote, ChainLinkText::ParentOfRoot);
}

// @kotowari[REQ-cli-023]
#[test]
fn chain_link_through_dot_dot_after_the_local_real_root_hides_contents() {
    assert_unfollowable_chain_hides_contents(
        LinkedRoot::Local,
        ChainLinkText::RealRootThroughParent,
    );
}

// @kotowari[REQ-cli-023]
#[test]
fn chain_link_through_dot_dot_after_the_remote_real_root_hides_contents() {
    assert_unfollowable_chain_hides_contents(
        LinkedRoot::Remote,
        ChainLinkText::RealRootThroughParent,
    );
}

// @kotowari[REQ-cli-023]
#[test]
fn chain_link_through_an_alias_of_the_local_root_dir_hides_contents() {
    assert_unfollowable_chain_hides_contents(LinkedRoot::Local, ChainLinkText::AliasOfRoot);
}

// @kotowari[REQ-cli-023]
#[test]
fn chain_link_through_an_alias_of_the_remote_root_dir_hides_contents() {
    assert_unfollowable_chain_hides_contents(LinkedRoot::Remote, ChainLinkText::AliasOfRoot);
}

// @kotowari[EX-cli-040]
#[test]
fn link_text_with_a_dot_component_still_shows_changed_contents() {
    let env = CliEnv::new(
        &[("target.txt", "left body\n")],
        &[("target.txt", "right body\n")],
    );
    place_symlink(&env.local_dir, "link.txt", "./target.txt");
    place_symlink(&env.remote_dir, "link.txt", "./target.txt");
    let output = env.cmd_with("diff").arg("link.txt").output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(
        body.contains("left body") && body.contains("right body"),
        "{output:?}"
    );
}
