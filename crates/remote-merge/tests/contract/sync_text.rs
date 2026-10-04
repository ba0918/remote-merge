#![cfg(unix)]
//! sync のテキスト出力の形（docs/ir/cli/sync.md の REQ-cli-102）の契約テスト。
//!
//! 関数呼び出しで sync し、--format text と同じ `format_sync_text` で整形した行を確かめる。
//! スキップと失敗の理由の文言は確かめない。

use remote_merge::service::output::format_sync_text;

use super::sync_support::{args, fixture};

/// 先頭の印とパスで行を探す（印の後の空白の数は確かめない）
fn has_line(text: &str, badge: &str, path: &str) -> bool {
    text.lines().any(|line| {
        let mut words = line.split_whitespace();
        words.next() == Some(badge) && words.next() == Some(path)
    })
}

/// スキップと失敗の行は、パスの後に括弧で囲んだ理由を添える
fn has_line_with_reason(text: &str, badge: &str, path: &str) -> bool {
    text.lines().any(|line| {
        let rest = line.trim_start().strip_prefix(badge).map(str::trim_start);
        rest.and_then(|rest| rest.strip_prefix(path))
            .is_some_and(|reason| reason.starts_with(" (") && reason.ends_with(')'))
    })
}

// @kotowari[REQ-cli-102]
#[test]
fn text_has_the_heading_a_line_per_target_the_files_and_the_summary() {
    let fixture = fixture();
    for path in ["a.txt", "b.txt"] {
        fixture.write("local", path, "incoming\n");
        fixture.write("develop", path, "develop old\n");
        fixture.write("staging", path, "staging old\n");
    }
    fixture.make_unreadable("staging", "b.txt");

    let (output, _) = fixture.sync(args(&["."], &["develop", "staging"]));
    let text = format_sync_text(&output);

    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Sync: local \u{2192} develop, staging", "{text}");
    assert!(lines.contains(&"[develop] success"), "{text}");
    assert!(lines.contains(&"[staging] partial"), "{text}");
    assert!(has_line(&text, "ok", "a.txt"), "{text}");
    assert!(has_line_with_reason(&text, "FAILED", "b.txt"), "{text}");
    assert_eq!(
        lines.last(),
        Some(&"Summary: 1/2 servers successful, 3 files merged, 1 files failed"),
        "{text}"
    );
}

// @kotowari[REQ-cli-102]
#[test]
fn dry_run_text_shows_planned_files_and_skipped_files_with_a_reason() {
    let fixture = fixture();
    fixture.write("local", "a.txt", "incoming\n");
    fixture.write("develop", "a.txt", "develop old\n");
    fixture.write("local", ".env", "A=local\n");
    fixture.write("develop", ".env", "A=develop\n");
    let mut args = args(&["."], &["develop"]);
    args.dry_run = true;
    args.force = false;

    let (output, _) = fixture.sync(args);
    let text = format_sync_text(&output);

    assert!(
        text.lines().any(|line| line == "[develop] success"),
        "{text}"
    );
    assert!(has_line(&text, "plan", "a.txt"), "{text}");
    assert!(has_line_with_reason(&text, "skip", ".env"), "{text}");
    assert_eq!(fixture.read("develop", "a.txt"), "develop old\n");
}

// @kotowari[REQ-cli-102]
#[test]
fn the_summary_adds_the_deleted_count_when_files_were_deleted() {
    let fixture = fixture();
    fixture.write("local", "a.txt", "incoming\n");
    fixture.write("develop", "a.txt", "develop old\n");
    fixture.write("develop", "extra.txt", "only on develop\n");
    let mut args = args(&["."], &["develop"]);
    args.delete = true;

    let (output, _) = fixture.sync(args);
    let text = format_sync_text(&output);

    assert_eq!(
        text.lines().last(),
        Some("Summary: 1/1 servers successful, 1 files merged, 1 files deleted"),
        "{text}"
    );
    assert!(!fixture.exists("develop", "extra.txt"));
}
