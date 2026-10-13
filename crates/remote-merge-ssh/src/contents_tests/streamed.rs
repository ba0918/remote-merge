//! ハッシュを取る道具がないサーバで、中身をまとめて尋ね、受けながら手元で SHA-256 を取る
//! （docs/ir/cli/status.md の REQ-cli-095）

use std::fs;

use remote_merge_core::op_failure::OpFailure;

use super::super::*;
use super::paths;
use crate::contents_stream::RawDigests;
use crate::scan_test_shell::shell_command;

const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const BOUNDARY: &str = "0123456789abcdef0123456789abcdef";

/// スクリプトを動かし、ログインシェルの出力に見立てた前置きの後に、標準出力を `chunk` バイトずつ渡す
fn over_shell(chunk: usize) -> impl FnMut(&str, &mut RawDigests) -> Result<(), String> {
    move |script: &str, digests: &mut RawDigests| {
        let output = shell_command(script).output().map_err(|e| e.to_string())?;
        for piece in b"login banner\n@RMRA"
            .chunks(3)
            .chain(output.stdout.chunks(chunk))
        {
            digests.feed(piece);
        }
        Ok(())
    }
}

// @kotowari[REQ-cli-095, REQ-paths-020]
#[test]
fn contents_are_hashed_piece_by_piece_and_failures_keep_their_kind() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("a.txt"), b"abc").unwrap();
    let tricky = b"\0@Bffff\0x\0".to_vec();
    fs::write(root.join("t"), &tricky).unwrap();
    let fulls: Vec<String> = paths(&root, &["a.txt", "missing", "t"])
        .into_iter()
        .map(|(_, full)| full)
        .collect();

    for chunk in [1, 2, 7, 4096] {
        let digests = digest_all(&fulls, BOUNDARY, 65536, over_shell(chunk)).unwrap();
        assert_eq!(
            digests,
            vec![
                Ok(ABC.to_string()),
                Err(OpFailure::NotFound),
                Ok(remote_merge_core::machine_ops::sha256_hex(&tricky)),
            ],
            "{chunk}"
        );
    }
}

#[test]
fn many_paths_are_asked_in_commands_within_the_length_limit() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let names: Vec<String> = (0..40).map(|i| format!("{}{i}", "n".repeat(100))).collect();
    for name in &names {
        fs::write(root.join(name), b"abc").unwrap();
    }
    let rels: Vec<&str> = names.iter().map(String::as_str).collect();
    let fulls: Vec<String> = paths(&root, &rels).into_iter().map(|(_, f)| f).collect();
    let mut commands = 0;
    let mut run = over_shell(4096);
    let counting = |script: &str, digests: &mut RawDigests| {
        commands += 1;
        run(script, digests)
    };

    let digests = digest_all(&fulls, BOUNDARY, 4096, counting).unwrap();

    assert!(commands > 1);
    assert!(digests.iter().all(|digest| *digest == Ok(ABC.to_string())));
}

#[test]
fn an_answer_cut_short_or_without_its_marker_cannot_be_read() {
    let mut cut = RawDigests::new(BOUNDARY);
    cut.feed(b"@RMRAW\0@P\0ab");
    assert!(cut.finish(1).is_err());
    let mut unmarked = RawDigests::new(BOUNDARY);
    unmarked.feed(b"abc");
    assert!(unmarked.finish(0).is_err());
}

#[test]
fn unhashed_paths_are_filled_in_order_even_when_a_path_is_asked_twice() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let asked = paths(&root, &["a", "a", "b"]);
    let hashes = vec![
        ("a".to_string(), Ok(None)),
        ("a".to_string(), Ok(None)),
        ("b".to_string(), Ok(Some("h".to_string()))),
    ];

    let filled = fill_unhashed(&asked, hashes, |fulls: &[String]| {
        assert_eq!(fulls.len(), 2);
        Ok::<_, String>(vec![Ok("d1".to_string()), Ok("d2".to_string())])
    })
    .unwrap();

    assert_eq!(
        filled,
        vec![
            ("a".to_string(), Ok("d1".to_string())),
            ("a".to_string(), Ok("d2".to_string())),
            ("b".to_string(), Ok("h".to_string())),
        ]
    );
}
