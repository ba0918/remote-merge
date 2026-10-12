#![cfg(unix)]
//! パスの辿り方（docs/ir/paths/resolution.md）の契約テストのうち、ローカルの経路のもの。
//!
//! 一時ディレクトリに例の状態を作り、ローカルのパスの確認の結果か文言を確かめる。

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use remote_merge_core::path_resolution::ResolvedPath;

/// 確認の root にする一時ディレクトリ。パスは symlink を含まない実パスにそろえる
struct Root {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

impl Root {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(directory.path()).unwrap();
        Self {
            _directory: directory,
            path,
        }
    }

    fn real(&self, rel: &str) -> String {
        format!("{}/{rel}", self.path.display())
    }

    fn dir(&self, rel: &str) -> &Self {
        fs::create_dir_all(self.path.join(rel)).unwrap();
        self
    }

    fn file(&self, rel: &str) -> &Self {
        fs::write(self.path.join(rel), "content\n").unwrap();
        self
    }

    fn link(&self, rel: &str, target: &str) -> &Self {
        symlink(target, self.path.join(rel)).unwrap();
        self
    }

    /// 実パスがちょうど `total` バイトになるディレクトリを root の下に作り、root からの相対パスを返す
    fn dir_of_length(&self, total: usize) -> String {
        // root の後の "/" と名前を足していき、残りが一つの名前に収まれば最後の名前にする
        let mut remaining = total - self.path.as_os_str().len() - 1;
        let mut names = Vec::new();
        while remaining > 201 {
            names.push("d".repeat(200));
            remaining -= 201;
        }
        names.push("d".repeat(remaining));
        let rel = names.join("/");
        self.dir(&rel);
        assert_eq!(self.real(&rel).len(), total);
        rel
    }

    fn inspect(&self, rel: &str) -> anyhow::Result<ResolvedPath> {
        inspect(&self.path, rel)
    }

    /// 確認がエラーで止まったときの文言
    fn error(&self, rel: &str) -> String {
        match self.inspect(rel) {
            Ok(result) => panic!("{rel}: expected an error, got {result:?}"),
            Err(error) => error.to_string(),
        }
    }
}

fn inspect(root: &Path, rel: &str) -> anyhow::Result<ResolvedPath> {
    remote_merge_engine::local_io::inspect_local_path(
        root,
        &remote_merge_core::relative_path::RelPath::parse(rel).unwrap(),
    )
}

fn symlink_result(link_target: &str, real_path: String) -> ResolvedPath {
    ResolvedPath::Symlink {
        link_target: link_target.into(),
        real_path,
    }
}

// @kotowari[REQ-paths-001, EX-paths-001]
#[test]
fn ex_paths_001_dots_and_doubled_slashes_in_a_link_target_are_ignored() {
    let root = Root::new();
    root.dir("dir")
        .file("dir/file.txt")
        .link("link", "dir/./file.txt")
        .link("link2", "dir//file.txt");

    for (link, target) in [("link", "dir/./file.txt"), ("link2", "dir//file.txt")] {
        assert_eq!(
            root.inspect(link).unwrap(),
            symlink_result(target, root.real("dir/file.txt")),
            "{link}"
        );
    }
}

// @kotowari[EX-paths-002]
#[test]
fn ex_paths_002_an_empty_path_follows_a_root_dir_that_is_a_symlink() {
    let base = Root::new();
    base.dir("real").link("root", "real");

    assert_eq!(
        inspect(&base.path.join("root"), "").unwrap(),
        ResolvedPath::Exists {
            real_path: base.real("real")
        }
    );
}

// @kotowari[REQ-paths-002, EX-paths-006]
#[test]
fn ex_paths_006_a_missing_path_tells_whether_its_parent_exists() {
    let root = Root::new();

    assert_eq!(
        root.inspect("a.txt").unwrap(),
        ResolvedPath::Missing {
            real_parent: root.path.display().to_string(),
            parent_exists: true,
        }
    );
    assert_eq!(
        root.inspect("missing/a.txt").unwrap(),
        ResolvedPath::Missing {
            real_parent: root.real("missing"),
            parent_exists: false,
        }
    );
}

// @kotowari[EX-paths-007]
#[test]
fn ex_paths_007_a_dangling_terminal_symlink_resolves_to_the_missing_name_it_points_at() {
    let root = Root::new();
    root.dir("a").dir("b").link("a/l", "../b/missing");

    assert_eq!(
        root.inspect("a/l").unwrap(),
        symlink_result("../b/missing", root.real("b/missing"))
    );
}

// @kotowari[EX-paths-003]
#[test]
fn ex_paths_003_an_ancestor_that_became_a_regular_file_is_reported_with_its_real_path() {
    let root = Root::new();
    root.file("dir");

    assert_eq!(
        root.inspect("dir/a.txt").unwrap(),
        ResolvedPath::AncestorNotDirectory {
            real_path: root.real("dir"),
            regular_file: true,
        }
    );
}

// @kotowari[EX-paths-004]
#[test]
fn ex_paths_004_an_ancestor_link_whose_target_ends_in_a_slash_still_reports_the_regular_file() {
    let root = Root::new();
    root.file("file.txt").link("l", "file.txt/");

    assert_eq!(
        root.inspect("l/a.txt").unwrap(),
        ResolvedPath::AncestorNotDirectory {
            real_path: root.real("file.txt"),
            regular_file: true,
        }
    );
}

// @kotowari[EX-paths-005]
#[test]
fn ex_paths_005_a_root_dir_that_is_a_regular_file_stops_the_inspection() {
    let base = Root::new();
    base.file("root");
    let root = Root {
        _directory: tempfile::tempdir().unwrap(),
        path: base.path.join("root"),
    };

    assert_eq!(root.error("a.txt"), "cannot resolve path: not a directory");
}

// @kotowari[REQ-paths-003, EX-paths-008]
#[test]
fn ex_paths_008_a_real_path_that_grows_past_the_limit_behind_a_link_stops_the_inspection() {
    let root = Root::new();
    let deep = root.dir_of_length(4060);
    root.link("deep", &deep)
        .link("link", &format!("deep/{}", "n".repeat(40)));

    assert_eq!(root.error("link"), "cannot resolve path: path too long");
}

// @kotowari[EX-paths-009]
#[test]
fn ex_paths_009_the_trailing_slash_of_a_link_target_is_not_counted_in_the_length() {
    let root = Root::new();
    let deep = root.dir_of_length(4095);
    root.link("link", &format!("{deep}/"));

    assert_eq!(
        root.inspect("link").unwrap(),
        symlink_result(&format!("{deep}/"), root.real(&deep))
    );
}

// @kotowari[EX-paths-010]
#[test]
fn ex_paths_010_a_long_link_target_with_a_short_real_path_is_followed() {
    let root = Root::new();
    let target = format!("{}file.txt", "./".repeat(2040));
    root.file("file.txt").link("link", &target);
    assert!(root.path.as_os_str().len() + 1 + target.len() > 4096);

    assert_eq!(
        root.inspect("link").unwrap(),
        symlink_result(&target, root.real("file.txt"))
    );
}

// @kotowari[REQ-paths-003]
#[test]
fn req_paths_003_a_name_longer_than_255_bytes_stops_the_inspection_even_when_missing() {
    let root = Root::new();

    assert_eq!(
        root.error(&format!("missing/{}", "n".repeat(256))),
        "cannot resolve path: path too long"
    );
    assert!(root.inspect(&"n".repeat(255)).is_ok());
}

/// root の下に "l1" から順に次を指す `count` 個のリンクを作り、最後のリンクは "file.txt" を指す
fn chain_of_links(root: &Root, count: usize) {
    root.file("file.txt");
    for index in 1..=count {
        let next = if index == count {
            "file.txt".to_string()
        } else {
            format!("l{}", index + 1)
        };
        root.link(&format!("l{index}"), &next);
    }
}

// @kotowari[EX-paths-011]
#[test]
fn ex_paths_011_forty_links_in_a_row_are_followed() {
    let root = Root::new();
    chain_of_links(&root, 40);

    assert_eq!(
        root.inspect("l1").unwrap(),
        symlink_result("l2", root.real("file.txt"))
    );
}

// @kotowari[REQ-paths-004, EX-paths-012]
#[test]
fn ex_paths_012_a_forty_first_link_stops_the_inspection() {
    let root = Root::new();
    chain_of_links(&root, 41);

    assert_eq!(
        root.error("l1"),
        "cannot resolve path: too many levels of symbolic links"
    );
}

// @kotowari[EX-paths-018]
#[test]
fn ex_paths_018_a_terminal_symlink_pointing_at_itself_stops_the_inspection() {
    let root = Root::new();
    root.link("loop", "loop");

    assert_eq!(
        root.error("loop"),
        "cannot resolve path: too many levels of symbolic links"
    );
}

// @kotowari[REQ-paths-005, EX-paths-013]
#[test]
fn ex_paths_013_a_chain_ending_in_a_regular_file_with_a_trailing_slash_stops_the_inspection() {
    let root = Root::new();
    root.file("file.txt")
        .link("inner", "file.txt/")
        .link("link", "inner");

    assert_eq!(root.error("link"), "cannot resolve path: not a directory");
}

// @kotowari[EX-paths-014]
#[test]
fn ex_paths_014_a_trailing_slash_on_a_missing_link_target_does_not_require_a_directory() {
    let root = Root::new();
    root.link("l", "newdir/");

    assert_eq!(
        root.inspect("l").unwrap(),
        symlink_result("newdir/", root.real("newdir"))
    );
}

// @kotowari[REQ-paths-006, EX-paths-015]
#[test]
fn ex_paths_015_a_directory_without_search_permission_is_not_looked_into() {
    use std::os::unix::fs::PermissionsExt;

    let root = Root::new();
    root.dir("locked").file("locked/a.txt");
    fs::set_permissions(root.path.join("locked"), fs::Permissions::from_mode(0o600)).unwrap();
    // 権限を無視できる利用者（root）では検索できない状態を作れない
    let privileged = fs::symlink_metadata(root.path.join("locked/a.txt")).is_ok();

    let result = root.inspect("locked/a.txt");

    fs::set_permissions(root.path.join("locked"), fs::Permissions::from_mode(0o700)).unwrap();
    if privileged {
        return;
    }
    assert_eq!(
        result.unwrap_err().to_string(),
        "cannot resolve path: permission denied"
    );
}

// @kotowari[REQ-paths-007, EX-paths-016]
#[test]
fn ex_paths_016_an_ancestor_symlink_to_a_missing_target_stops_the_inspection() {
    let root = Root::new();
    root.link("dir", "gone");

    assert_eq!(
        root.error("dir/a.txt"),
        "cannot resolve path: dangling symbolic link"
    );
}

// @kotowari[EX-paths-017]
#[test]
fn ex_paths_017_a_parent_reference_in_the_missing_part_of_a_link_target_stops_the_inspection() {
    let root = Root::new();
    root.file("file.txt").link("link", "missing/../file.txt");

    assert_eq!(
        root.error("link"),
        "cannot resolve path: parent reference in missing part"
    );
}

// @kotowari[EX-paths-023]
#[test]
fn ex_paths_023_a_link_target_that_is_not_utf_8_stops_the_inspection() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let root = Root::new();
    symlink(OsStr::from_bytes(b"name\xff"), root.path.join("link")).unwrap();

    assert_eq!(root.error("link"), "cannot resolve path: non-UTF-8 path");
}
