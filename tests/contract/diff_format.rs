//! CLI diff の JSON とテキストの形と --max-lines（docs/ir/cli/diff-output.md の REQ-cli-053・054・
//! 058）の契約テスト。組み方は diff_support にある。

use std::collections::BTreeSet;

use remote_merge::service::output::format_multi_diff_text;
use remote_merge::service::types::MultiDiffOutput;

use super::diff_support::*;

// ── REQ-cli-053: JSON の形 ──

// @kotowari[REQ-cli-053]
#[test]
fn req_cli_053_json_has_files_and_summary_and_each_file_has_the_documented_keys() {
    let fixture = DiffFixture::new(&[("f.txt", b"keep\nold\n")], &[("f.txt", b"keep\nnew\n")]);

    let (output, _) = fixture.diff(&["f.txt"]);
    let json = json(&output);

    let top: BTreeSet<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(top, BTreeSet::from(["files", "summary"]), "{json}");
    assert!(json["summary"].is_object(), "{json}");

    let file = entry(&json, "f.txt");
    let keys: BTreeSet<&str> = file
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from(["path", "left", "right", "sensitive", "truncated", "hunks"]),
        "{json}"
    );
    assert_eq!(file["left"]["label"], "local");
    assert_eq!(
        file["left"]["root"],
        fixture.left.path().display().to_string()
    );
    assert_eq!(file["right"]["label"], "develop");
    assert_eq!(
        file["right"]["root"],
        format!("example.invalid:{}", fixture.right.path().display())
    );
    assert_eq!(file["sensitive"], false);
    assert_eq!(file["truncated"], false);
    for hunk in file["hunks"].as_array().unwrap() {
        assert!(hunk["index"].is_u64(), "{hunk}");
        assert!(hunk["left_start"].is_u64(), "{hunk}");
        assert!(hunk["right_start"].is_u64(), "{hunk}");
    }
    let kinds: BTreeSet<String> = lines_of(file).into_iter().map(|(kind, _)| kind).collect();
    assert_eq!(kinds, set(&["context", "added", "removed"]), "{json}");
    assert_eq!(
        lines_of(file),
        vec![
            ("context".to_string(), "keep".to_string()),
            ("removed".to_string(), "old".to_string()),
            ("added".to_string(), "new".to_string())
        ]
    );
}

// @kotowari[REQ-cli-053]
#[test]
fn req_cli_053_binary_hashes_and_note_appear_only_on_the_files_they_apply_to() {
    let fixture = DiffFixture::new(
        &[
            ("f.txt", b"old\n"),
            ("b.bin", b"left\0"),
            (".env", b"KEY=left\n"),
        ],
        &[
            ("f.txt", b"new\n"),
            ("b.bin", b"right\0"),
            (".env", b"KEY=right\n"),
        ],
    );

    let (output, _) = fixture.diff(&["f.txt", "b.bin", ".env"]);
    let json = json(&output);

    let has = |path: &str, key: &str| entry(&json, path).get(key).is_some();
    for key in ["binary", "left_hash", "right_hash"] {
        assert!(has("b.bin", key), "{key}: {json}");
        assert!(!has("f.txt", key), "{key}: {json}");
        assert!(!has(".env", key), "{key}: {json}");
    }
    assert!(has(".env", "note"), "{json}");
    assert!(!has("f.txt", "note"), "{json}");
    assert!(!has("b.bin", "note"), "{json}");
    assert!(json.get("errors").is_none(), "{json}");
}

// ── REQ-cli-054: 変更の行数で打ち切る ──

/// 文脈の 3 行の後に、削除 2 行と追加 3 行の変更がある "f.txt"
fn five_changes_after_context() -> DiffFixture {
    DiffFixture::new(
        &[("f.txt", b"a\nb\nc\nx\ny\n")],
        &[("f.txt", b"a\nb\nc\n1\n2\n3\n")],
    )
}

fn diff_with_max_lines(fixture: &DiffFixture, max_lines: Option<usize>) -> MultiDiffOutput {
    let mut args = args(&["f.txt"]);
    args.max_lines = max_lines;
    fixture.run(args).0
}

// @kotowari[REQ-cli-054]
#[test]
fn req_cli_054_change_lines_stop_at_the_limit_without_counting_context() {
    let fixture = five_changes_after_context();

    let output = diff_with_max_lines(&fixture, Some(3));
    let json = json(&output);
    let file = entry(&json, "f.txt");
    let lines = lines_of(file);

    assert_eq!(count_changes(&lines), 3, "{json}");
    assert!(
        lines.iter().any(|(kind, _)| kind == "context"),
        "context lines before the changes must still appear: {json}"
    );
    assert_eq!(
        lines.last().unwrap(),
        &("added".to_string(), "1".to_string()),
        "{json}"
    );
    assert_eq!(file["truncated"], true);
    assert!(
        format_multi_diff_text(&output).contains("... (output truncated)"),
        "{}",
        format_multi_diff_text(&output)
    );
}

// @kotowari[REQ-cli-054]
#[test]
fn req_cli_054_zero_or_no_limit_outputs_every_change_line() {
    let fixture = five_changes_after_context();

    for max_lines in [Some(0), None] {
        let output = diff_with_max_lines(&fixture, max_lines);
        let json = json(&output);
        let file = entry(&json, "f.txt");

        assert_eq!(count_changes(&lines_of(file)), 5, "{max_lines:?}: {json}");
        assert_eq!(file["truncated"], false, "{max_lines:?}");
        assert!(
            !format_multi_diff_text(&output).contains("(output truncated)"),
            "{max_lines:?}"
        );
    }
}

// ── REQ-cli-058: テキストの形 ──

// @kotowari[REQ-cli-058]
#[test]
fn req_cli_058_text_has_headers_hunk_lines_and_the_summary() {
    let fixture = DiffFixture::new(
        &[
            ("f.txt", b"keep\nold\n"),
            ("g.txt", b"left\n"),
            ("same.txt", b"same\n"),
        ],
        &[
            ("f.txt", b"keep\nnew\n"),
            ("g.txt", b"right\n"),
            ("same.txt", b"same\n"),
        ],
    );

    let (output, _) = fixture.diff(&["f.txt", "g.txt", "same.txt"]);
    let text = format_multi_diff_text(&output);
    let lines: Vec<&str> = text.lines().collect();

    let header = lines
        .iter()
        .position(|line| *line == "--- a/f.txt (local)")
        .unwrap_or_else(|| panic!("{text}"));
    assert_eq!(lines[header + 1], "+++ b/f.txt (develop)", "{text}");
    assert!(lines[header + 2].starts_with("@@"), "{text}");
    assert_eq!(
        &lines[header + 3..header + 6],
        [" keep", "-old", "+new"],
        "{text}"
    );
    assert!(lines.contains(&"--- a/g.txt (local)"), "{text}");
    assert!(lines.contains(&"+++ b/g.txt (develop)"), "{text}");
    assert_eq!(
        last_line(&text),
        "2 file(s) with changes out of 3 total",
        "{text}"
    );
}
