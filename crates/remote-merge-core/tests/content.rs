use remote_merge_core::content::{Content, Encoding, Interpretation, interpret};
use remote_merge_core::diff::engine::{HunkDirection, PairScript};
use std::collections::HashSet;

#[test]
fn selected_changes_keep_destination_alias_bytes_and_full_terminators() {
    let Content::Text(left) = interpret(
        b"\x87\x54\nnew\r\n",
        Interpretation::Text(Encoding::ShiftJis),
    )
    .unwrap() else {
        panic!()
    };
    let Content::Text(right) =
        interpret(b"\xfa\x4a\nold\n", Interpretation::Text(Encoding::ShiftJis)).unwrap()
    else {
        panic!()
    };
    assert_eq!(left.line(0), right.line(0));
    let script = PairScript::new(&left, &right);
    assert_eq!(
        script.apply(&HashSet::from([0]), HunkDirection::LeftToRight),
        b"\xfa\x4a\nnew\r\n"
    );
    assert_eq!(
        script.apply(&HashSet::new(), HunkDirection::LeftToRight),
        right.raw()
    );
    assert_eq!(
        script.apply(&HashSet::from([0]), HunkDirection::RightToLeft),
        b"\x87\x54\nold\n"
    );
}

#[test]
fn selected_final_newline_and_destination_bom_are_preserved() {
    let Content::Text(left) = interpret(b"A\n", Interpretation::Unspecified).unwrap() else {
        panic!()
    };
    let Content::Text(right) = interpret(b"\xef\xbb\xbfA", Interpretation::Unspecified).unwrap()
    else {
        panic!()
    };
    let script = PairScript::new(&left, &right);
    assert_eq!(
        script.apply(&HashSet::from([0]), HunkDirection::LeftToRight),
        b"\xef\xbb\xbfA\n"
    );
}

#[test]
fn explicit_legacy_codecs_do_not_switch_on_utf8_bom_bytes() {
    assert!(interpret(b"\xef\xbb\xbfA", Interpretation::Text(Encoding::EucJp)).is_err());
    match interpret(b"\xef\xbb\xbfA", Interpretation::Text(Encoding::ShiftJis)) {
        Ok(Content::Text(text)) => {
            assert!(text.bom().is_empty());
            assert_ne!(text.text(), "A");
        }
        Err(error) => assert_eq!(error.encoding, Encoding::ShiftJis),
        Ok(Content::Binary) => panic!("a BOM without NUL does not select binary"),
    }
}

#[test]
fn repeated_lines_retain_direction_specific_full_line_alignment() {
    for (old, new) in [
        (b"A\nA\r\nB".as_slice(), b"A\r\nA\nB\n".as_slice()),
        (b"x\nx\ny\n", b"x\ny\nx\n"),
    ] {
        let Content::Text(left) = interpret(old, Interpretation::Unspecified).unwrap() else {
            panic!()
        };
        let Content::Text(right) = interpret(new, Interpretation::Unspecified).unwrap() else {
            panic!()
        };
        for (source, target) in [(&left, &right), (&right, &left)] {
            let script = PairScript::new(source, target);
            let remote_merge_core::diff::engine::DiffResult::Modified { hunks, .. } =
                script.display()
            else {
                panic!()
            };
            assert_eq!(
                script.apply(&(0..hunks.len()).collect(), HunkDirection::LeftToRight),
                source.raw()
            );
            assert_eq!(
                script.apply(&HashSet::new(), HunkDirection::LeftToRight),
                target.raw()
            );
        }
    }
}

#[test]
fn explicit_japanese_codecs_decode_strictly_without_switching() {
    for (bytes, codec) in [
        (b"\xa4\xa2\r\n".as_slice(), Encoding::EucJp),
        (b"\x82\xa0\r\n".as_slice(), Encoding::ShiftJis),
    ] {
        let Content::Text(text) = interpret(bytes, Interpretation::Text(codec)).unwrap() else {
            panic!("text expected")
        };
        assert_eq!(text.text(), "あ\r\n");
        assert_eq!(text.raw_line(0), bytes);
        assert!(interpret(&bytes[..1], Interpretation::Text(codec)).is_err());
    }
    assert_eq!(
        "WINDOWS-31J".parse::<Interpretation>().unwrap(),
        Interpretation::Text(Encoding::ShiftJis)
    );
    assert!("utf-16".parse::<Interpretation>().is_err());
}

#[test]
fn unspecified_invalid_tail_is_binary_but_explicit_utf8_fails() {
    let mut bytes = vec![b'a'; 8192];
    bytes.push(0xff);
    assert!(matches!(
        interpret(&bytes, Interpretation::Unspecified).unwrap(),
        Content::Binary
    ));
    assert!(interpret(&bytes, Interpretation::Text(Encoding::Utf8)).is_err());
}

// 先頭 8KB の境目をまたぐ文字があっても、UTF-8 の正しさは中身の全体で確かめるのでテキストになる
#[test]
fn a_multibyte_character_across_the_first_8_kb_is_still_text() {
    let content = format!("{}あ", "a".repeat(8191));
    assert!(matches!(
        interpret(content.as_bytes(), Interpretation::Unspecified).unwrap(),
        Content::Text(_)
    ));
}

#[test]
fn nul_boundary_and_bom_preserve_original_line_spans() {
    assert!(matches!(
        interpret(b"valid text", Interpretation::Binary).unwrap(),
        Content::Binary
    ));
    let mut bytes = vec![b'a'; 8192];
    bytes.push(0);
    assert!(matches!(
        interpret(&bytes, Interpretation::Unspecified).unwrap(),
        Content::Text(_)
    ));
    bytes[8191] = 0;
    assert!(matches!(
        interpret(&bytes, Interpretation::Text(Encoding::Utf8)).unwrap(),
        Content::Binary
    ));
    let Content::Text(text) =
        interpret(b"\xef\xbb\xbfA\r\nB", Interpretation::Unspecified).unwrap()
    else {
        panic!("text expected")
    };
    assert_eq!(text.bom(), b"\xef\xbb\xbf");
    assert_eq!(text.text(), "A\r\nB");
    assert_eq!(text.raw_line(0), b"A\r\n");
    assert_eq!(text.raw_line(1), b"B");
    let Content::Text(empty) = interpret(b"", Interpretation::Unspecified).unwrap() else {
        panic!("text expected")
    };
    assert_eq!(empty.line_count(), 0);
}
