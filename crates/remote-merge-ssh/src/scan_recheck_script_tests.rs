//! もう一度尋ねるスクリプトを sh で動かす単体テスト
use super::*;

// @kotowari[REQ-scan-014]
#[cfg(unix)]
#[test]
fn the_second_question_sees_every_kind_of_name() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for (dir, child) in [
        ("empty", None),
        ("hidden", Some(".h")),
        ("dots", Some("..x")),
        ("star", Some("*")),
        ("bracket", Some(".[!.]*")),
    ] {
        std::fs::create_dir(root.join(dir)).unwrap();
        if let Some(child) = child {
            std::fs::write(root.join(dir).join(child), "").unwrap();
        }
    }
    let printed: Vec<Vec<u8>> = ["empty", "hidden", "dots", "star", "bracket"]
        .iter()
        .map(|d| format!("./{d}").into_bytes())
        .collect();
    let refs: Vec<&[u8]> = printed.iter().map(Vec::as_slice).collect();
    let script = recheck_script(root.to_str().unwrap().as_bytes(), &refs);
    let output = crate::scan_test_shell::shell_command(&script)
        .output()
        .unwrap();
    let checks = read_recheck(&output.stdout, &output.stderr)
        .unwrap()
        .unwrap();
    let names: Vec<(String, Vec<Vec<u8>>)> = checks
        .into_iter()
        .map(|c| (String::from_utf8(c.printed).unwrap(), c.names))
        .collect();
    assert_eq!(
        names,
        [
            ("./empty".to_string(), vec![]),
            ("./hidden".to_string(), vec![b".h".to_vec()]),
            ("./dots".to_string(), vec![b"..x".to_vec()]),
            ("./star".to_string(), vec![b"*".to_vec()]),
            ("./bracket".to_string(), vec![b".[!.]*".to_vec()]),
        ]
    );
}

#[test]
fn directories_to_ask_again_are_split_so_that_each_command_line_fits_the_limit() {
    let printed: Vec<Vec<u8>> = (0..300)
        .map(|i| format!("./dir/number{i:04}").into_bytes())
        .collect();
    let refs: Vec<&[u8]> = printed.iter().map(Vec::as_slice).collect();
    let limit = 4096;
    let ranges = split_recheck(b"/srv", &refs, limit);
    assert!(ranges.len() > 1);
    assert_eq!(ranges.first().unwrap().start, 0);
    assert_eq!(ranges.last().unwrap().end, refs.len());
    for range in ranges {
        let script = recheck_script(b"/srv", &refs[range]);
        assert!(crate::scan_script::wrapped_len(&script) <= limit);
    }
}

#[test]
fn any_directory_name_is_asked_again_as_it_is() {
    use std::os::unix::ffi::OsStrExt;
    let temp = tempfile::tempdir().unwrap();
    let names: [&[u8]; 6] = [
        b"it's",
        b"nl\nx",
        b"bad\xff",
        b"sp ace",
        b"$HOME`x`",
        b"back\\slash",
    ];
    for name in names {
        let dir = temp.path().join(std::ffi::OsStr::from_bytes(name));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("in"), "").unwrap();
    }
    let printed: Vec<Vec<u8>> = names.iter().map(|n| [b"./", *n].concat()).collect();
    let refs: Vec<&[u8]> = printed.iter().map(Vec::as_slice).collect();
    let script = recheck_script(temp.path().to_str().unwrap().as_bytes(), &refs);
    assert!(script.is_ascii());
    let output = crate::scan_test_shell::shell_command(&script)
        .output()
        .unwrap();
    let checks = read_recheck(&output.stdout, &output.stderr)
        .unwrap()
        .unwrap();
    let read: Vec<(Vec<u8>, Vec<Vec<u8>>)> =
        checks.into_iter().map(|c| (c.printed, c.names)).collect();
    let expected: Vec<(Vec<u8>, Vec<Vec<u8>>)> = printed
        .iter()
        .map(|p| (p.clone(), vec![b"in".to_vec()]))
        .collect();
    assert_eq!(read, expected);
}
