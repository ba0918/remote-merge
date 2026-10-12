//! 読めて検索できないディレクトリと、exclude の下の多くの読めないディレクトリについて、二つの
//! 集め方の走査が同じになることを確かめる（REQ-scan-017）
use std::fs;

use remote_merge_core::scan::outcome::ScanScope;
use remote_merge_core::scan::this_machine::MachineCollector;

use crate::scan_test_shell::*;

/// root_dir の "d" に `names` を置き、検索の権限をなくして（mode 0644）二つの集め方で走査する
fn compare_unsearchable(names: &[&[u8]], patterns: &[&str]) -> Option<(String, String)> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let d = root.join("d");
    fs::create_dir_all(&d).unwrap();
    for name in names {
        fs::write(d.join(OsStr::from_bytes(name)), "x").unwrap();
    }
    fs::set_permissions(&d, fs::Permissions::from_mode(0o644)).unwrap();
    let applies = fs::metadata(d.join("x-not-there")).is_err() && !permissions_apply_root();
    let rule = exclude(patterns);
    let machine = scan_with(
        &mut MachineCollector::new(root.to_path_buf()),
        &rule,
        ScanScope::Whole,
    );
    let shell = scan_with(&mut Shell(root.to_path_buf()), &rule, ScanScope::Whole);
    fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
    applies.then_some((machine, shell))
}

/// root で動いているか（root には権限が効かない）
fn permissions_apply_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

// @kotowari[REQ-scan-017, REQ-scan-014, REQ-config-044]
#[test]
fn a_readable_directory_without_search_permission_agrees_whatever_it_holds() {
    let sets: [&[&[u8]]; 7] = [
        &[],
        &[b".h"],
        &[b"*"],
        &[b".[!.]*", b"..?*"],
        &[b"[x]"],
        &[b"a\nb", b"t\tb", b"bad\xff", b"..x"],
        &[b"cache", b"*"],
    ];
    for names in sets {
        for patterns in [&[][..], &["cache"][..], &["c?che", "\\*"][..]] {
            let Some((machine, shell)) = compare_unsearchable(names, patterns) else {
                eprintln!("note: running as root; permissions do not apply");
                return;
            };
            assert_eq!(shell, machine, "{names:?} {patterns:?}");
        }
    }
}

// @kotowari[REQ-scan-017, REQ-config-044]
#[test]
fn many_unreadable_directories_below_an_excluded_directory_agree() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dirs: Vec<_> = (0..800)
        .map(|i| {
            root.join("cache")
                .join(format!("locked_directory_number_{i:04}"))
        })
        .collect();
    for dir in &dirs {
        fs::create_dir_all(dir).unwrap();
        fs::set_permissions(dir, fs::Permissions::from_mode(0o000)).unwrap();
    }
    let rule = exclude(&["c?che"]);
    let machine = scan_with(
        &mut MachineCollector::new(root.to_path_buf()),
        &rule,
        ScanScope::Whole,
    );
    let shell = scan_with(&mut Shell(root.to_path_buf()), &rule, ScanScope::Whole);
    for dir in &dirs {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(machine.starts_with("Ok("), "{machine}");
    assert_eq!(shell, machine);
}
