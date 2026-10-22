use super::*;

fn parsed(input: &str) -> RelPath {
    RelPath::parse(input).unwrap()
}

fn refusal(input: &str) -> String {
    RelPath::parse(input).unwrap_err().to_string()
}

#[test]
fn a_plain_path_is_kept_as_it_was_typed() {
    let path = parsed("src/app.rs");
    assert_eq!(path.as_str(), "src/app.rs");
    assert_eq!(path.as_typed(), "src/app.rs");
    assert!(!path.is_root());
}

#[test]
fn an_absolute_path_is_refused_with_the_path_as_typed() {
    assert_eq!(
        refusal("/etc/passwd"),
        "absolute subpath not allowed: /etc/passwd"
    );
    assert_eq!(refusal("/a/"), "absolute subpath not allowed: /a/");
    assert_eq!(refusal("/."), "absolute subpath not allowed: /.");
}

#[test]
fn a_parent_element_is_refused_with_the_path_as_typed() {
    for input in [
        "../b.txt",
        "a/../b",
        "a/..",
        "src/../",
        "..",
        "./../x",
        "../etc/passwd",
        "src/..",
        "src/../etc/passwd",
    ] {
        assert_eq!(
            refusal(input),
            format!("path traversal not allowed: {input}"),
            "{input}"
        );
    }
}

#[test]
fn an_absolute_path_with_a_parent_element_is_refused_as_absolute() {
    assert_eq!(refusal("/a/../b"), "absolute subpath not allowed: /a/../b");
}

#[test]
fn names_that_only_contain_dots_are_not_parent_elements() {
    for input in ["a..b", "..a", "a/...", ".hidden", "a./b"] {
        assert_eq!(parsed(input).as_str(), input, "{input}");
    }
}

#[test]
fn slash_dot_dot_slash_and_empty_name_the_whole_root_dir() {
    for input in ["/", "//", ".", "./", ""] {
        let path = parsed(input);
        assert!(path.is_root(), "{input}");
        assert_eq!(path.as_str(), "", "{input}");
        assert_eq!(path.as_typed(), input, "{input}");
    }
}

#[test]
fn a_trailing_slash_names_the_same_path_and_is_remembered() {
    let plain = parsed("src");
    let slash = parsed("src/");
    assert_eq!(slash.as_str(), plain.as_str());
    assert_eq!(slash.as_typed(), "src/");
    assert_eq!(parsed("a/b//").as_str(), "a/b");
}

#[test]
fn the_path_to_resolve_keeps_the_trailing_slash_and_names_the_root_dir_by_an_empty_path() {
    assert_eq!(parsed("src/").path_to_resolve(), "src/");
    assert_eq!(parsed("a/./b").path_to_resolve(), "a/./b");
    for input in ["/", "//", ".", "./", ""] {
        assert_eq!(parsed(input).path_to_resolve(), "", "{input}");
    }
}

#[test]
fn the_root_dir_itself_is_a_relative_path_without_checking() {
    let root = RelPath::root();
    assert!(root.is_root());
    assert_eq!(root.as_str(), "");
    assert_eq!(root.as_typed(), "");
}

#[test]
fn a_path_splits_into_its_parent_and_its_last_name() {
    let path = parsed("a/b/c.txt/");
    let (parent, name) = path.parent_and_name();
    assert_eq!((parent.as_str(), name), ("a/b", "c.txt"));
    let path = parsed("c.txt");
    let (parent, name) = path.parent_and_name();
    assert!(parent.is_root());
    assert_eq!(name, "c.txt");
}

#[test]
fn dot_elements_inside_a_path_are_left_for_path_resolution() {
    assert_eq!(parsed("./src/app.rs").as_str(), "./src/app.rs");
    assert_eq!(parsed("a/./b").as_str(), "a/./b");
    assert!(!parsed("./src").is_root());
}

#[test]
fn a_checked_path_is_checked_again_with_the_same_result() {
    let path = parsed("src/");
    assert_eq!(RelPath::parse(path.as_str()).unwrap().as_str(), "src");
}

// "x" の下は "x/a" で、名前が "x" で始まるだけの "xy.txt" は下にない。そのものは下でなく、含まれる
#[test]
fn below_and_within_compare_whole_path_elements() {
    assert!(is_below("x/a", "x"));
    assert!(is_below("x/a/b", "x/a"));
    assert!(!is_below("x", "x"));
    assert!(!is_below("xy.txt", "x"));
    assert!(!is_below("x", "x/a"));

    assert!(is_within("x", "x"));
    assert!(is_within("x/a", "x"));
    assert!(!is_within("xy.txt", "x"));
    assert!(!is_within("y", "x"));
}

#[test]
fn normalized_drops_dot_and_empty_elements() {
    assert_eq!(normalized("./a//b/./"), "a/b");
    assert_eq!(normalized("a/b"), "a/b");
    assert_eq!(normalized("./"), "");
    assert_eq!(normalized(""), "");
}
