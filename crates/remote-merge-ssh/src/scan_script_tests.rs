use super::*;

#[cfg(unix)]
#[test]
fn any_bytes_are_given_to_the_shell_as_they_are() {
    let value = b"a b\tc\nd'e\\f%g\xffh\n\n";
    let script = format!("{}printf '%s' \"$v\"", assign("v", value));
    assert!(script.is_ascii());
    let output = std::process::Command::new("sh")
        .args(["-c", &script])
        .output()
        .unwrap();
    assert_eq!(output.stdout, value);
}

#[test]
fn prunable_patterns_become_find_tests_on_the_path_from_the_root_dir() {
    let prunable = Prunable {
        names: vec!["node_modules".into()],
        paths: vec!["a/b".into()],
    };
    let (setup, expression) = prune_expression(&prunable);
    assert!(setup.contains("n0=") && setup.contains("p0="));
    assert_eq!(
        expression,
        "\\( -name \"$n0\" -o -path \"$p0\" \\) -prune -o "
    );
    assert_eq!(prune_expression(&Prunable::default()).1, "");
}

#[test]
fn the_script_never_lets_find_follow_links_below_a_start() {
    let script = build(b"/srv/app", &[], &Prunable::default());
    assert!(!script.contains("find -L"));
    assert!(script.contains(" -mindepth 1 "));
}

#[test]
fn questions_are_split_so_that_each_command_line_fits_the_limit() {
    let paths: Vec<Vec<u8>> = (0..200)
        .map(|i| format!("dir{i:03}/x").into_bytes())
        .collect();
    let questions: Vec<ScriptQuestion> = paths
        .iter()
        .map(|path| ScriptQuestion {
            path,
            descend: true,
        })
        .collect();
    let limit = 4096;
    let ranges = split_for_command_line(b"/srv/app", &questions, &Prunable::default(), limit);
    assert!(ranges.len() > 1);
    assert_eq!(ranges.first().unwrap().start, 0);
    assert_eq!(ranges.last().unwrap().end, questions.len());
    for pair in ranges.windows(2) {
        assert_eq!(pair[0].end, pair[1].start);
    }
    for range in ranges {
        let script = build(b"/srv/app", &questions[range], &Prunable::default());
        assert!(crate::login_shell::wrap_for_login_shell(&script).len() <= limit);
    }
}

#[cfg(unix)]
#[test]
fn a_root_dir_that_starts_with_a_dash_is_not_read_as_an_option() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("-dash")).unwrap();
    std::fs::write(temp.path().join("-dash/a.txt"), "a").unwrap();
    let questions = [ScriptQuestion {
        path: b"",
        descend: true,
    }];
    let script = build(b"-dash", &questions, &Prunable::default());
    let output = std::process::Command::new("sh")
        .args(["-c", &script])
        .current_dir(temp.path())
        .output()
        .unwrap();
    let answers = crate::scan_output::parse(&output.stdout, &output.stderr, &questions)
        .unwrap()
        .answers;
    let entries = &answers[0].subtree.as_ref().unwrap().entries;
    assert_eq!(entries[0].path, b"a.txt");
}
