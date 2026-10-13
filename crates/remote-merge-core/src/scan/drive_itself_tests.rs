//! 起点そのものの状態だけを集める起点（`Start::itself`）の単体テスト。比べる流れが名前で指定した
//! パスと、変更の直前の書き込み先の状態を集めるのに使う（docs/ir/compare/flow.md の REQ-compare-001、
//! docs/ir/compare/reading.md の REQ-compare-015）
use super::super::outcome::{LinkPoint, ListedKind, Listing, ScanScope, Start};
use super::super::raw::{Depth, LinkTarget, Unreached};
use super::super::test_support::*;

fn itself(paths: &[&str]) -> ScanScope {
    ScanScope::Starts(paths.iter().map(|path| Start::itself(*path, 0)).collect())
}

fn kinds(listing: &Listing) -> Vec<(String, ListedKind)> {
    listing
        .entries
        .iter()
        .map(|entry| (entry.path.clone(), entry.kind.clone()))
        .collect()
}

// @kotowari[REQ-compare-001]
#[test]
fn the_entry_itself_is_listed_without_what_is_below_it() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"a.txt", plain(file(b""), None))
        .with(b"d", plain(dir(b"", 2), None))
        .with(b"l", link_answer(LinkTarget::Directory, None))
        .with(b"gone", unreached(Unreached::Missing));
    let request = scope(itself(&["a.txt", "d", "l", "gone"]), 9);

    let listing = run(&mut fake, &exclude(&[]), &request).unwrap();

    assert_eq!(
        kinds(&listing),
        [
            ("a.txt".to_string(), ListedKind::File),
            ("d".to_string(), ListedKind::Directory),
            (
                "l".to_string(),
                ListedKind::Symlink {
                    target: "t".into(),
                    points_to: LinkPoint::Directory
                }
            ),
        ]
    );
    assert!(fake.asked.iter().all(|q| q.depth == Depth::Itself));
}

// @kotowari[REQ-compare-001, REQ-scan-021]
#[test]
fn an_entry_of_another_kind_itself_is_listed_as_that_kind() {
    let mut other = file(b"");
    other.kind = super::super::raw::RawKind::Other;
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"pipe", plain(other, None));
    let listing = run(&mut fake, &exclude(&[]), &scope(itself(&["pipe"]), 9)).unwrap();
    assert_eq!(kinds(&listing), [("pipe".to_string(), ListedKind::Other)]);
}
