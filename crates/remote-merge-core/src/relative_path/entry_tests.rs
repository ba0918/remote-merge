use std::path::Path;

use proptest::prelude::*;

use super::*;

/// root_dir 全体を指す入力の形。どれも項目にはならない
const ROOT_FORMS: [&str; 8] = ["/", "//", ".", "./", "", "./.", ".//.", "././"];

#[test]
fn every_form_naming_the_whole_root_dir_is_refused_as_an_entry() {
    for input in ROOT_FORMS {
        assert_eq!(
            EntryPath::parse(input),
            Err(RelPathError::RootDirItself(input.to_string())),
            "{input:?}"
        );
        let rel = RelPath::parse(input).unwrap();
        assert_eq!(
            EntryPath::try_from(&rel),
            Err(RelPathError::RootDirItself(input.to_string())),
            "{input:?}"
        );
    }
}

#[test]
fn the_refusal_names_the_root_dir_and_the_value_as_typed() {
    assert_eq!(
        EntryPath::parse("./").unwrap_err().to_string(),
        "root_dir itself is not an entry: ./"
    );
}

#[test]
fn an_entry_keeps_the_rules_of_the_relative_path() {
    assert_eq!(
        EntryPath::parse("/etc/passwd"),
        Err(RelPathError::Absolute("/etc/passwd".into()))
    );
    assert_eq!(
        EntryPath::parse("a/../b"),
        Err(RelPathError::Traversal("a/../b".into()))
    );
}

#[test]
fn an_entry_names_one_path_under_the_root_dir() {
    let entry = EntryPath::parse("dir/a.txt/").unwrap();
    assert_eq!(entry.as_str(), "dir/a.txt");
    assert_eq!(entry.as_typed(), "dir/a.txt/");
    assert_eq!(entry.as_rel().as_typed(), "dir/a.txt/");
    let (parent, name) = entry.parent_and_name();
    assert_eq!((parent.as_str(), name), ("dir", "a.txt"));
    assert_eq!(EntryPath::parse("./a").unwrap().as_typed(), "./a");
}

#[test]
fn entries_are_listed_as_they_were_typed() {
    let entries = [
        EntryPath::parse("b/").unwrap(),
        EntryPath::parse("a").unwrap(),
    ];
    assert_eq!(entries_as_typed(&entries), ["b/", "a"]);
}

#[test]
fn the_parents_of_an_entry_never_include_the_root_dir_itself() {
    let typed = |path: &str| -> Vec<String> {
        EntryPath::parse(path)
            .unwrap()
            .parent_entries()
            .iter()
            .map(|entry| entry.as_typed().to_string())
            .collect()
    };
    assert_eq!(typed("./sub/f.txt"), ["./sub"]);
    assert_eq!(typed("a/b/c"), ["a", "a/b"]);
    assert_eq!(typed("a/b/c/"), ["a", "a/b"]);
    assert_eq!(typed("f"), Vec::<String>::new());
    assert_eq!(typed("./f"), Vec::<String>::new());
}

/// "/" で区切った要素から組み立てた、RelPath::parse を通りうる文字列
fn relative_inputs() -> impl Strategy<Value = String> {
    let element = prop_oneof![
        Just(String::new()),
        Just(".".to_string()),
        Just("..".to_string()),
        "[a-z.]{1,3}",
    ];
    (any::<bool>(), proptest::collection::vec(element, 0..5)).prop_map(|(leading, elements)| {
        let joined = elements.join("/");
        if leading {
            format!("/{joined}")
        } else {
            joined
        }
    })
}

proptest! {
    // 項目になった値は、root_dir とつないでも root_dir 自身を指さない
    #[test]
    fn an_entry_never_joins_back_to_the_root_dir_itself(input in relative_inputs()) {
        prop_assume!(RelPath::parse(&input).is_ok());
        let root = Path::new("/srv/root");
        if let Ok(entry) = EntryPath::parse(&input) {
            let joined_path = root.join(entry.as_str());
            let joined: Vec<_> = joined_path.components().collect();
            let itself: Vec<_> = root.components().collect();
            prop_assert_ne!(joined, itself, "{:?}", input);
        }
    }
}
