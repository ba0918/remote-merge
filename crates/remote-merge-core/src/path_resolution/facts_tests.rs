use super::super::ExistingKind;
use super::*;

fn book(entries: &[(&str, PathFact)]) -> FactBook {
    let mut book = FactBook::default();
    for (path, fact) in entries {
        book.insert(path.to_string(), fact.clone());
    }
    book
}

fn dir() -> PathFact {
    PathFact::Directory { searchable: true }
}

#[test]
fn with_no_facts_the_whole_chain_from_the_root_is_asked() {
    assert_eq!(
        next_step("/srv/app", "dir/a.txt", &FactBook::default()),
        Step::Ask(vec![
            "/".into(),
            "/srv".into(),
            "/srv/app".into(),
            "/srv/app/dir".into(),
            "/srv/app/dir/a.txt".into(),
        ])
    );
}

#[test]
fn facts_up_to_the_end_resolve_the_path() {
    let facts = book(&[
        ("/", dir()),
        ("/srv", dir()),
        ("/srv/a.txt", PathFact::RegularFile),
    ]);
    assert_eq!(
        next_step("/srv", "a.txt", &facts),
        Step::Done(Ok(ResolvedPath::Exists {
            real_path: "/srv/a.txt".into(),
            kind: ExistingKind::File,
        }))
    );
}

#[test]
fn a_symlink_asks_only_the_spliced_part_next() {
    let facts = book(&[
        ("/", dir()),
        ("/srv", dir()),
        ("/srv/l", PathFact::Symlink(Ok(b"/data/x".to_vec()))),
    ]);
    assert_eq!(
        next_step("/srv", "l/a.txt", &facts),
        Step::Ask(vec![
            "/data".into(),
            "/data/x".into(),
            "/data/x/a.txt".into()
        ])
    );
}

#[test]
fn a_directory_without_search_permission_stops_with_permission_denied() {
    let facts = book(&[
        ("/", dir()),
        ("/srv", dir()),
        ("/srv/locked", PathFact::Directory { searchable: false }),
    ]);
    assert_eq!(
        next_step("/srv", "locked/a.txt", &facts),
        Step::Done(Err(ResolveError::PermissionDenied))
    );
    assert_eq!(
        next_step("/srv", "locked", &facts),
        Step::Done(Ok(ResolvedPath::Exists {
            real_path: "/srv/locked".into(),
            kind: ExistingKind::Directory,
        }))
    );
}

#[test]
fn a_missing_or_unreadable_element_and_a_missing_readlink_end_the_walk() {
    let facts = book(&[
        ("/", dir()),
        ("/srv", dir()),
        ("/srv/gone", PathFact::Missing),
        ("/srv/odd", PathFact::Unreadable),
        (
            "/srv/l",
            PathFact::Symlink(Err(ResolveError::ReadlinkNotAvailable)),
        ),
    ]);
    assert_eq!(
        next_step("/srv", "gone/a.txt", &facts),
        Step::Done(Ok(ResolvedPath::Missing {
            real_parent: "/srv/gone".into(),
            parent_exists: false
        }))
    );
    assert_eq!(
        next_step("/srv", "odd/a.txt", &facts),
        Step::Done(Err(ResolveError::CannotRead))
    );
    assert_eq!(
        next_step("/srv", "l", &facts),
        Step::Done(Err(ResolveError::ReadlinkNotAvailable))
    );
}
