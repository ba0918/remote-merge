//! パスの確認の事実をまとめて尋ねるスクリプトを、このマシンの sh（または
//! REMOTE_MERGE_TEST_SCAN_IMAGE のコンテナ）で動かして確かめる

use std::cell::Cell;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

use remote_merge_core::path_resolution::{ResolveError, ResolvedPath};

use super::*;
use crate::scan_test_shell::shell_command;

fn run(script: &str) -> Result<Vec<u8>, String> {
    let output = shell_command(script).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    Ok(output.stdout)
}

fn resolve(root: &Path, rels: &[&str]) -> Vec<Result<ResolvedPath, ResolveError>> {
    let rels: Vec<String> = rels.iter().map(|rel| rel.to_string()).collect();
    resolve_all(root.to_str().unwrap(), &rels, 65536, run).unwrap()
}

fn real(root: &Path, rel: &str) -> String {
    format!("{}/{rel}", fs::canonicalize(root).unwrap().display())
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[test]
fn each_kind_of_element_is_answered_like_the_local_walk() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    fs::write(root.join("dir/a.txt"), "x").unwrap();
    fs::write(root.join("f"), "x").unwrap();
    symlink("dir", root.join("l")).unwrap();
    symlink("end\n", root.join("nl")).unwrap();

    let answers = resolve(
        &root,
        &["dir/a.txt", "f/a.txt", "missing/a", "l/a.txt", "nl", ""],
    );

    assert_eq!(
        answers,
        vec![
            Ok(ResolvedPath::Exists {
                real_path: real(&root, "dir/a.txt")
            }),
            Ok(ResolvedPath::AncestorNotDirectory {
                real_path: real(&root, "f"),
                regular_file: true
            }),
            Ok(ResolvedPath::Missing {
                real_parent: real(&root, "missing"),
                parent_exists: false
            }),
            Ok(ResolvedPath::Exists {
                real_path: real(&root, "dir/a.txt")
            }),
            Ok(ResolvedPath::Symlink {
                link_target: "end\n".into(),
                real_path: real(&root, "end\n")
            }),
            Ok(ResolvedPath::Exists {
                real_path: root.display().to_string()
            }),
        ]
    );
}

#[test]
fn a_directory_without_search_permission_stops_with_permission_denied() {
    if is_root() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::create_dir(root.join("locked")).unwrap();
    fs::write(root.join("locked/a.txt"), "x").unwrap();
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o600)).unwrap();

    let answers = resolve(&root, &["locked/a.txt", "locked"]);
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o700)).unwrap();

    assert_eq!(
        answers,
        vec![
            Err(ResolveError::PermissionDenied),
            Ok(ResolvedPath::Exists {
                real_path: real(&root, "locked")
            })
        ]
    );
}

#[test]
fn the_number_of_questions_is_the_symlink_levels_plus_one() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::create_dir_all(root.join("x/y")).unwrap();
    fs::write(root.join("x/y/a.txt"), "x").unwrap();
    symlink("x", root.join("l1")).unwrap();
    symlink("l1/y", root.join("l2")).unwrap();
    let asked = Cell::new(0);
    let counting = |script: &str| {
        asked.set(asked.get() + 1);
        run(script)
    };

    let rels = vec!["l2/a.txt".to_string(), "x/y/a.txt".to_string()];
    let answers = resolve_all(root.to_str().unwrap(), &rels, 65536, counting).unwrap();

    assert!(matches!(answers[0], Ok(ResolvedPath::Exists { .. })));
    // l2 と、その先の l1 の二段の symlink を辿る
    assert!(asked.get() <= 3, "{} questions", asked.get());
}

#[test]
fn without_readlink_only_a_path_meeting_a_symlink_stops() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("a.txt"), "x").unwrap();
    symlink("a.txt", root.join("l")).unwrap();
    // ls だけを置いた PATH（リンク先はスクリプトを動かすマシンの /bin/ls）
    let bin = tempfile::tempdir().unwrap();
    symlink("/bin/ls", bin.path().join("ls")).unwrap();
    let without_readlink = |script: &str| {
        let script = format!("PATH='{}'; export PATH\n{script}", bin.path().display());
        run(&script)
    };
    let rels = vec!["a.txt".to_string(), "l".to_string()];

    let answers = resolve_all(root.to_str().unwrap(), &rels, 65536, without_readlink).unwrap();

    assert!(matches!(answers[0], Ok(ResolvedPath::Exists { .. })));
    assert_eq!(answers[1], Err(ResolveError::ReadlinkNotAvailable));
}

#[test]
fn a_relative_root_dir_is_resolved_from_the_working_directory() {
    let temp = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(temp.path()).unwrap();
    fs::create_dir(home.join("app")).unwrap();
    fs::write(home.join("app/a.txt"), "x").unwrap();
    let in_home = |script: &str| {
        let script = format!("cd '{}' && {{ {script}\n}}", home.display());
        run(&script)
    };

    let answers = resolve_all("app", &["a.txt".to_string()], 65536, in_home).unwrap();

    assert_eq!(
        answers,
        vec![Ok(ResolvedPath::Exists {
            real_path: format!("{}/app/a.txt", home.display())
        })]
    );
}

#[test]
fn an_answer_that_does_not_match_the_questions_is_unreadable() {
    let rels = vec!["a".to_string()];
    for broken in [
        &b""[..],
        b"@RMFACTS\0@END\0",
        b"@RMFACTS\0@Q\0/zz\0D\0@END\0",
    ] {
        let failure =
            resolve_all("/srv", &rels, 65536, |_| Ok::<_, String>(broken.to_vec())).unwrap_err();
        assert!(
            matches!(failure, FactsFailure::Unreadable(_)),
            "{failure:?}"
        );
    }
}

#[test]
fn a_remote_without_ls_is_a_missing_tool() {
    let rels = vec!["a".to_string()];
    let failure = resolve_all("/srv", &rels, 65536, |_| {
        Ok::<_, String>(b"@RMFACTS\0@NOLS\0".to_vec())
    })
    .unwrap_err();
    assert!(matches!(failure, FactsFailure::NoLs), "{failure:?}");
}

// @kotowari[REQ-paths-008]
#[test]
fn a_link_whose_target_reads_like_the_no_ls_mark_is_still_a_link() {
    let temp = tempfile::tempdir().unwrap();
    symlink("@NOLS", temp.path().join("l")).unwrap();

    let resolved = resolve(temp.path(), &["l"]);

    let local = remote_merge_core::path_resolution::os::resolve_on_this_machine(temp.path(), "l");
    assert_eq!(resolved, vec![local]);
}
