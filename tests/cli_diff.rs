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
