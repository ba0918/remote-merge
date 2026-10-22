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
        .with(b"gone", unreached_answer(Unreached::Missing));
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

// 名前で指定したパスには exclude を当てない（docs/ir/cli/diff-strategy.md の REQ-cli-081）
// @kotowari[REQ-cli-081, REQ-compare-001]
#[test]
fn an_excluded_entry_itself_is_still_listed() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"app.log", plain(file(b""), None))
        .with(b"logs/a.txt", plain(file(b""), None))
        .with(b"target", plain(dir(b"", 2), None));
    let request = scope(itself(&["app.log", "logs/a.txt", "target"]), 9);

    let listing = run(&mut fake, &exclude(&["*.log", "logs", "target"]), &request).unwrap();

    assert_eq!(
        kinds(&listing),
        [
            ("app.log".to_string(), ListedKind::File),
            ("logs/a.txt".to_string(), ListedKind::File),
            ("target".to_string(), ListedKind::Directory),
        ]
    );
}

// 名前で指定した起点のうち、ディレクトリとディレクトリへのリンクは器で件数に数えず、ファイルは数える。
// 上限を超えたら打ち切りにし、超えた起点は載せない
// @kotowari[REQ-scan-010, REQ-cli-021, REQ-cli-084]
#[test]
fn named_directories_and_directory_links_are_not_counted_but_files_are() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"d", plain(dir(b"", 2), None))
        .with(b"l", link_answer(LinkTarget::Directory, None))
        .with(b"a.txt", plain(file(b""), None))
        .with(b"b.txt", plain(file(b""), None));

    let fits = run(
        &mut fake,
        &exclude(&[]),
        &scope(itself(&["d", "l", "a.txt"]), 1),
    )
    .unwrap();
    let over = run(
        &mut fake,
        &exclude(&[]),
        &scope(itself(&["a.txt", "b.txt"]), 1),
    )
    .unwrap();

    assert!(!fits.truncated, "{fits:?}");
    assert_eq!(fits.counted, 1);
    assert_eq!(kinds(&fits).len(), 3);
    assert!(over.truncated, "{over:?}");
    assert_eq!(kinds(&over).len(), 1);
}
