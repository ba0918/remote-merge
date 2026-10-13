//! 中身の終わりの区切りを、コマンドの引数（同じリモートの他の利用者がプロセスの一覧から見られる）に
//! 入れず、標準入力で渡すこと

use std::fs;

use super::super::*;
use super::paths;
use crate::contents_tests_support::fed;

const BOUNDARY: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn the_read_command_text_does_not_hold_the_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("a.txt"), b"abc").unwrap();

    let answers = read_all(
        &paths(&root, &["a.txt"]),
        None,
        BOUNDARY,
        65536,
        |script, input| {
            assert!(!script.contains(BOUNDARY), "{script}");
            fed(script, input)
        },
    )
    .unwrap();

    assert_eq!(answers[0].1, Ok(b"abc".to_vec()));
}

#[test]
fn the_streamed_command_text_does_not_hold_the_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("a.txt"), b"abc").unwrap();
    let fulls: Vec<String> = paths(&root, &["a.txt"])
        .into_iter()
        .map(|(_, full)| full)
        .collect();

    let digests = digest_all(&fulls, BOUNDARY, 65536, |script, input, digests| {
        assert!(!script.contains(BOUNDARY), "{script}");
        digests.feed(&fed(script, input)?);
        Ok::<(), String>(())
    })
    .unwrap();

    assert_eq!(
        digests,
        [Ok(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_string()
        )]
    );
}
