use super::diff_support::{DiffFixture, args, entry, json, lines_of, sha256};
use super::merge_support::{args as merge_args, fixture};
use remote_merge::cli::diff::execute_diff;
use remote_merge_core::content::{Encoding, Interpretation};
use remote_merge_engine::treat::content::Overrides;
mod config;
mod raw;

fn encodings(left: Encoding, right: Encoding) -> Overrides {
    Overrides {
        left: Some(Interpretation::Text(left)),
        right: Some(Interpretation::Text(right)),
        reference: None,
    }
}

fn diff(
    fixture: &super::merge_support::Fixture,
    paths: &[&str],
    encodings: Overrides,
    reference: bool,
) -> (serde_json::Value, i32) {
    let mut arguments = args(paths);
    arguments.encodings = encodings;
    arguments.ref_server = reference.then(|| "staging".into());
    let (output, code) =
        execute_diff(arguments, fixture.config(), fixture.runtime_targets()).unwrap();
    (json(&output), code)
}

// @kotowari[REQ-content-001, REQ-content-007, EX-content-001]
#[test]
fn req_content_001_euc_jp_diff_uses_strict_decoded_lines() {
    let fixture = DiffFixture::new(&[("a", b"\xa4\xa2\n")], &[("a", b"\xa4\xa4\n")]);
    let mut args = args(&["a"]);
    args.encodings.left = Some(Interpretation::Text(Encoding::EucJp));
    args.encodings.right = args.encodings.left;
    let (output, code) = fixture.run(args);
    assert_eq!(code, 1);
    let json = json(&output);
    let file = entry(&json, "a");
    assert_eq!(file["encodings"]["left"], "euc-jp");
    assert_eq!(
        lines_of(file),
        [
            ("removed".into(), "あ".into()),
            ("added".into(), "い".into())
        ]
    );
}

// @kotowari[REQ-merge-045, REQ-merge-046]
#[test]
fn req_merge_046_cp932_hunks_retain_destination_equal_alias_bytes() {
    let fixture = super::merge_support::fixture();
    fixture.write_bytes("local", "a", b"\x87\x54\nnew\r\n");
    fixture.write_bytes("develop", "a", b"\xfa\x4a\nold\n");
    let mut args = super::merge_support::args(&["a"]);
    args.hunks = Some(vec![0]);
    args.encodings.left = Some(Interpretation::Text(Encoding::ShiftJis));
    args.encodings.right = args.encodings.left;
    let (_, code) = fixture.merge_json(args);
    assert_eq!(code, 0);
    assert_eq!(fixture.read_bytes("develop", "a"), b"\xfa\x4a\nnew\r\n");
}

// @kotowari[REQ-content-008, EX-content-010]
#[test]
fn req_content_008_whole_file_previews_explain_no_encoding_conversion() {
    let fixture = super::merge_support::fixture();
    fixture.append_config("[content.defaults]\nlocal='euc-jp'\ndevelop='utf-8'\n");
    fixture.write_bytes("local", "a", b"\xa4\xa2\n");
    fixture.write("develop", "a", "old\n");
    let mut args = super::merge_support::args(&["a"]);
    args.dry_run = true;
    args.encodings.left = Some(Interpretation::Text(Encoding::EucJp));
    args.encodings.right = Some(Interpretation::Text(Encoding::Utf8));
    let (json, text) = fixture.merge_json_and_text(args);
    assert!(
        json["merged"][0]["copy_note"]
            .as_str()
            .unwrap()
            .contains("without encoding conversion")
    );
    assert!(text.contains("without encoding conversion"));
    let mut sync = super::merge_support::sync_args(&["a"]);
    sync.dry_run = true;
    let result =
        remote_merge::cli::sync::execute_sync(sync, fixture.config(), fixture.runtime_targets())
            .unwrap();
    let remote_merge::cli::sync::SyncCommandOutput::Result(output) = result.output else {
        panic!("sync result expected")
    };
    assert!(
        remote_merge::service::output::format_sync_text(&output)
            .contains("without encoding conversion")
    );
    let json = serde_json::to_value(output).unwrap();
    assert!(
        json["targets"][0]["merged"][0]["copy_note"]
            .as_str()
            .unwrap()
            .contains("without encoding conversion")
    );
    for command in ["merge", "sync"] {
        let help = std::process::Command::new(env!("CARGO_BIN_EXE_remote-merge"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(
            String::from_utf8(help.stdout)
                .unwrap()
                .contains("without encoding conversion")
        );
    }
}

// @kotowari[REQ-cli-061, EX-content-002]
#[test]
fn ex_content_002_unspecified_non_utf8_keeps_raw_hashes() {
    let fixture = DiffFixture::new(&[("a", b"\xa4\xa2\n")], &[("a", b"\xa4\xa4\n")]);
    let (output, code) = fixture.run(args(&["a"]));
    assert_eq!(code, 1);
    let value = json(&output);
    assert_eq!(entry(&value, "a")["binary"], true);
    assert_eq!(entry(&value, "a")["left_hash"], sha256(b"\xa4\xa2\n"));
}

// @kotowari[REQ-content-006, EX-content-003]
#[test]
fn ex_content_003_invalid_tail_is_an_error_before_file_limits() {
    let mut invalid = vec![b'a'; 8192];
    invalid.push(0xa4);
    let fixture = DiffFixture::new(
        &[("a-bad", &invalid), ("b-good", b"\xa4\xa2\n")],
        &[("a-bad", b"valid\n"), ("b-good", b"\xa4\xa4\n")],
    );
    let mut args = args(&[]);
    args.encodings = encodings(Encoding::EucJp, Encoding::EucJp);
    args.max_files = 1;
    args.max_lines = Some(1);
    let (output, code) = fixture.run(args);
    assert_eq!(code, 2);
    let value = json(&output);
    assert_eq!(value["files"][0]["path"], "b-good");
    assert_eq!(value["summary"]["files_with_changes"], 1);
    assert_eq!(value["errors"][0]["path"], "a-bad");
    let reason = value["errors"][0]["reason"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(reason.contains("left") && reason.contains("euc-jp") && reason.contains("invalid"));
    assert_eq!(value["files"][0]["truncated"], true);
}

// @kotowari[REQ-content-006]
#[test]
fn decoding_failures_name_each_failed_side_and_its_codec() {
    let fixture = DiffFixture::new(&[("a", b"\xa4")], &[("a", b"\x82")]);
    let mut args = args(&["a"]);
    args.encodings = encodings(Encoding::EucJp, Encoding::ShiftJis);
    let (output, code) = fixture.run(args);
    assert_eq!(code, 2);
    assert!(output.files.is_empty());
    assert_eq!(output.summary.files_with_changes, 0);
    let reasons = output
        .errors
        .iter()
        .map(|error| error.reason.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(reasons.contains("left") && reasons.contains("euc-jp"));
    assert!(reasons.contains("right") && reasons.contains("shift_jis"));
}

// @kotowari[REQ-content-003, REQ-content-005, EX-content-005]
#[test]
fn ex_content_005_bom_only_difference_has_no_hunks_and_writes_nothing() {
    let fixture = fixture();
    fixture.write_bytes("local", "a", b"A\n");
    fixture.write_bytes("develop", "a", b"\xef\xbb\xbfA\n");
    let (value, code) = diff(&fixture, &["a"], Overrides::default(), false);
    assert_eq!(code, 1);
    assert_eq!(entry(&value, "a")["byte_only_difference"], true);
    assert_eq!(entry(&value, "a")["hunks"], serde_json::json!([]));
    assert!(
        entry(&value, "a")["note"]
            .as_str()
            .unwrap()
            .contains("representation")
    );
    let before = fixture.modified("develop", "a");
    let mut args = merge_args(&["a"]);
    args.hunks = Some(vec![0]);
    let (_, code) = fixture.merge_json(args);
    assert_eq!(code, 0);
    assert_eq!(fixture.read_bytes("develop", "a"), b"\xef\xbb\xbfA\n");
    assert_eq!(fixture.modified("develop", "a"), before);
}

// @kotowari[REQ-content-005, REQ-content-007, EX-content-008]
#[test]
fn ex_content_008_equal_japanese_in_different_codecs_reports_raw_difference() {
    let fixture = fixture();
    fixture.write("local", "a", "あ\n");
    fixture.write_bytes("develop", "a", b"\xa4\xa2\n");
    let (value, code) = diff(
        &fixture,
        &["a"],
        encodings(Encoding::Utf8, Encoding::EucJp),
        false,
    );
    assert_eq!(code, 1);
    let file = entry(&value, "a");
    assert_eq!(
        file["encodings"],
        serde_json::json!({"left":"utf-8", "right":"euc-jp"})
    );
    assert_eq!(file["byte_only_difference"], true);
    assert_eq!(file["left_hash"], sha256("あ\n".as_bytes()));
    assert_eq!(file["right_hash"], sha256(b"\xa4\xa2\n"));
    assert_eq!(file["hunks"], serde_json::json!([]));
}

// @kotowari[REQ-content-002, REQ-merge-031, EX-content-004]
#[test]
fn ex_content_004_reference_codec_can_differ_and_conflicts_agree() {
    let fixture = fixture();
    fixture.write_bytes("local", "a", b"\xa4\xa2\n");
    fixture.write_bytes("develop", "a", b"\xa4\xa4\n");
    fixture.write("staging", "a", "う\n");
    let mut overrides = encodings(Encoding::EucJp, Encoding::EucJp);
    overrides.reference = Some(Interpretation::Text(Encoding::Utf8));
    let (value, code) = diff(&fixture, &["a"], overrides, true);
    assert_eq!(code, 1);
    let file = entry(&value, "a");
    assert_eq!(file["conflict_count"], 1);
    assert!(
        file["ref_hunks"][0]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line["content"] == "う")
    );
    for dry_run in [false, true] {
        let mut args = merge_args(&["a"]);
        args.encodings = overrides;
        args.ref_server = Some("staging".into());
        args.hunks = Some(vec![0]);
        args.dry_run = dry_run;
        assert!(
            fixture
                .merge_error(args)
                .to_string()
                .contains("three-way conflict")
        );
    }
    assert_eq!(fixture.read_bytes("develop", "a"), b"\xa4\xa4\n");
}

// @kotowari[REQ-content-006, EX-content-009, EX-content-013]
#[test]
fn ex_content_013_invalid_reference_keeps_left_right_diff_and_blocks_hunks() {
    let fixture = fixture();
    fixture.write("local", "a", "left\n");
    fixture.write("develop", "a", "right\n");
    fixture.write_bytes("staging", "a", b"\xa4");
    let overrides = Overrides {
        reference: Some(Interpretation::Text(Encoding::EucJp)),
        ..Default::default()
    };
    let (value, code) = diff(&fixture, &["a"], overrides, true);
    assert_eq!(code, 2);
    assert_eq!(value["summary"]["files_with_changes"], 1);
    let file = entry(&value, "a");
    assert!(!file["hunks"].as_array().unwrap().is_empty());
    assert!(file.get("ref_hunks").is_none() && file.get("conflict_count").is_none());
    assert_eq!(value["errors"][0]["path"], "a");
    let reason = value["errors"][0]["reason"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(reason.contains("ref") && reason.contains("euc-jp"));
    for dry_run in [false, true] {
        let mut args = merge_args(&["a"]);
        args.encodings = overrides;
        args.ref_server = Some("staging".into());
        args.hunks = Some(vec![0]);
        args.dry_run = dry_run;
        assert!(fixture.merge_error(args).to_string().contains("euc-jp"));
    }
    assert_eq!(fixture.read("develop", "a"), "right\n");
    let mut args = merge_args(&["a"]);
    args.encodings = overrides;
    args.ref_server = Some("staging".into());
    args.hunks = Some(vec![0]);
    args.force = true;
    assert_eq!(fixture.merge_json(args).1, 0);
}

// @kotowari[REQ-content-002, EX-content-011]
#[test]
fn ex_content_011_newline_only_and_body_changes_conflict() {
    let fixture = fixture();
    fixture.write("local", "a", "A\n");
    fixture.write("develop", "a", "B\r\n");
    fixture.write("staging", "a", "A\r\n");
    let (value, _) = diff(&fixture, &["a"], Overrides::default(), true);
    assert_eq!(entry(&value, "a")["conflict_count"], 1);
}

// @kotowari[REQ-content-002, REQ-cli-016, REQ-cli-099, EX-content-012]
#[test]
fn ex_content_012_binary_reference_uses_whole_file_conflict_in_diff_and_dry_run() {
    let fixture = fixture();
    fixture.write("local", "a", "left\n");
    fixture.write("develop", "a", "right\n");
    fixture.write_bytes("staging", "a", b"base\0");
    let (value, _) = diff(&fixture, &["a"], Overrides::default(), true);
    let file = entry(&value, "a");
    assert_eq!(file["conflict_count"], 1);
    assert_eq!(
        file["conflict_regions"],
        serde_json::json!([{"ref_range":{"whole_file":true}}])
    );
    assert_eq!(file["ref_hash"], sha256(b"base\0"));
    assert!(file.get("ref_hunks").is_none());
    let mut args = merge_args(&["a"]);
    args.hunks = Some(vec![0]);
    args.dry_run = true;
    args.ref_server = Some("staging".into());
    assert!(
        fixture
            .merge_error(args)
            .to_string()
            .contains("three-way conflict")
    );
    assert_eq!(fixture.read("develop", "a"), "right\n");
}

// @kotowari[REQ-merge-032, REQ-merge-046, EX-merge-069]
#[test]
fn ex_merge_069_euc_jp_display_numbers_select_only_the_second_change() {
    let fixture = fixture();
    let old = b"\xa4\xa2\n0\n1\n2\n3\n4\n5\n6\n\xa4\xa4\n";
    let new = b"\xa4\xa6\n0\n1\n2\n3\n4\n5\n6\n\xa4\xa8\n";
    fixture.write_bytes("local", "a", old);
    fixture.write_bytes("develop", "a", new);
    let overrides = encodings(Encoding::EucJp, Encoding::EucJp);
    let (value, _) = diff(&fixture, &["a"], overrides, false);
    assert_eq!(entry(&value, "a")["hunks"].as_array().unwrap().len(), 2);
    let index = entry(&value, "a")["hunks"][1]["index"].as_u64().unwrap() as usize;
    let mut args = merge_args(&["a"]);
    args.encodings = overrides;
    args.hunks = Some(vec![index]);
    assert_eq!(fixture.merge_json(args).1, 0);
    assert_eq!(
        fixture.read_bytes("develop", "a"),
        b"\xa4\xa6\n0\n1\n2\n3\n4\n5\n6\n\xa4\xa4\n"
    );
}

// @kotowari[REQ-merge-045, EX-merge-070]
#[test]
fn ex_merge_070_different_codecs_are_rejected_even_with_force_or_dry_run() {
    let fixture = fixture();
    fixture.write("local", "a", "あ\n");
    fixture.write_bytes("develop", "a", b"\xa4\xa4\n");
    for (force, dry_run) in [(true, false), (false, true), (true, true)] {
        let mut args = merge_args(&["a"]);
        args.encodings = encodings(Encoding::Utf8, Encoding::EucJp);
        args.hunks = Some(vec![0]);
        args.force = force;
        args.dry_run = dry_run;
        assert!(
            fixture
                .merge_error(args)
                .to_string()
                .contains("same content encoding")
        );
        assert_eq!(fixture.read_bytes("develop", "a"), b"\xa4\xa4\n");
    }
}

// @kotowari[REQ-merge-046, EX-merge-071]
#[test]
fn ex_merge_071_unselected_extension_characters_and_mixed_newlines_keep_raw_bytes() {
    let fixture = fixture();
    fixture.write_bytes("local", "a", b"new\n0\n1\n2\n3\n4\n5\n6\nsource\n");
    let destination = b"old\n0\n1\n2\n3\n4\n5\n6\n\xfa\x4a\r\n\x87\x54\n";
    fixture.write_bytes("develop", "a", destination);
    let mut args = merge_args(&["a"]);
    args.encodings = encodings(Encoding::ShiftJis, Encoding::ShiftJis);
    args.hunks = Some(vec![0]);
    assert_eq!(fixture.merge_json(args).1, 0);
    assert_eq!(
        fixture.read_bytes("develop", "a"),
        b"new\n0\n1\n2\n3\n4\n5\n6\n\xfa\x4a\r\n\x87\x54\n"
    );
}

// @kotowari[REQ-merge-047, EX-merge-072]
#[test]
fn ex_merge_072_absent_destination_inherits_source_bytes_but_empty_file_has_codec() {
    let fixture = fixture();
    fixture.append_config("[content.defaults]\nlocal='euc-jp'\ndevelop='utf-8'\n");
    fixture.write_bytes("local", "a", b"\xa4\xa2\n");
    let mut args = merge_args(&["a"]);
    args.hunks = Some(vec![0]);
    assert_eq!(fixture.merge_json(args).1, 0);
    assert_eq!(fixture.read_bytes("develop", "a"), b"\xa4\xa2\n");
    fixture.write("develop", "a", "");
    let mut args = merge_args(&["a"]);
    args.hunks = Some(vec![0]);
    assert!(
        fixture
            .merge_error(args)
            .to_string()
            .contains("same content encoding")
    );
    assert_eq!(fixture.read_bytes("develop", "a"), b"");
}

// @kotowari[REQ-content-003, REQ-merge-046, EX-merge-073]
#[test]
fn ex_merge_073_first_line_merge_keeps_destination_bom() {
    let fixture = fixture();
    fixture.write_bytes("local", "a", b"new\r\nend");
    fixture.write_bytes("develop", "a", b"\xef\xbb\xbfold\nend");
    let mut args = merge_args(&["a"]);
    args.hunks = Some(vec![0]);
    assert_eq!(fixture.merge_json(args).1, 0);
    assert_eq!(
        fixture.read_bytes("develop", "a"),
        b"\xef\xbb\xbfnew\r\nend"
    );
}
