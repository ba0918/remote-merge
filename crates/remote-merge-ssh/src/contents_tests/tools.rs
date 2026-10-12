//! 道具を絞った PATH での大きさとハッシュ

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use remote_merge_core::op_failure::{OpFailure, Tool};

use super::super::*;
use super::{paths, run};

/// `bin` だけを PATH にして動かす
pub(super) fn run_with_only(bin: &Path) -> impl FnMut(&str) -> Result<Vec<u8>, String> + '_ {
    move |script: &str| run(&format!("PATH='{}'; export PATH\n{script}", bin.display()))
}

/// `tools` だけを置いた PATH。どれも、スクリプトを動かすマシンの /usr/bin か /bin にあるものを
/// 起動する小さなスクリプトにする（どちらにあるかはマシンで違う）
pub(super) fn bin_with(tools: &[&str]) -> tempfile::TempDir {
    let bin = tempfile::tempdir().unwrap();
    for tool in tools {
        let path = bin.path().join(tool);
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nfor d in /usr/bin /bin; do [ -x \"$d/{tool}\" ] && exec \"$d/{tool}\" \"$@\"; done\nexit 127\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

#[test]
fn without_stat_c_the_size_is_read_from_ls_and_without_both_it_is_a_missing_tool() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("a"), b"abc").unwrap();
    let asked = paths(&root, &["a"]);

    let with_ls = bin_with(&["ls", "cat", "awk"]);
    let answers = read_all(&asked, Some(10), 65536, run_with_only(with_ls.path())).unwrap();
    assert_eq!(answers[0].1, Ok(b"abc".to_vec()));

    let without = bin_with(&["cat"]);
    let answers = read_all(&asked, Some(10), 65536, run_with_only(without.path())).unwrap();
    assert_eq!(answers[0].1, Err(OpFailure::ToolMissing(Tool::Stat)));
}

#[test]
fn shasum_gives_the_same_hash_when_sha256sum_is_missing() {
    if !Path::new("/usr/bin/shasum").exists()
        || std::env::var_os("REMOTE_MERGE_TEST_SCAN_IMAGE").is_some()
    {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("a.txt"), b"abc").unwrap();
    let only_shasum = bin_with(&["cat", "shasum"]);

    let hashes = hash_all(
        &paths(&root, &["a.txt"]),
        65536,
        run_with_only(only_shasum.path()),
    )
    .unwrap();

    assert_eq!(
        hashes[0].1,
        Ok(Some(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_string()
        ))
    );
}
