#![cfg(unix)]
//! 走査の一覧に載るもの（docs/ir/scan/listing.md の REQ-scan-006・007）と、ディレクトリを指す
//! symlink の配下の列挙（docs/ir/scan/directory-links.md の REQ-scan-002）の契約テスト。
//!
//! 関数呼び出しで status を実行する組み方はリモートの経路を通らないため、実行ファイルを試験 SSH
//! サーバに対して `status --left local --right develop --all --format json` で起動する。
//! 左右の root_dir に同じ構成を置き、確かめたい項目が "equal" で出ることを、左のローカルの経路と
//! 右のリモートの経路の両方がその項目を一覧に載せた証拠にする（片側が載せなければ "left_only" か
//! "right_only" になる）。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use super::common::{gen_config, place_files, place_symlink, TestDirs};

/// 標準出力の JSON の "files" を "path" から "status" への対応にする
fn statuses_by_path(output: &Output) -> BTreeMap<String, String> {
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("status output is not JSON ({error}): {output:?}"));
    json["files"]
        .as_array()
        .unwrap_or_else(|| panic!("files missing: {json}"))
        .iter()
        .map(|file| {
            (
                file["path"].as_str().unwrap().to_string(),
                file["status"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// `config` を書き、隔離を確かめてから全件表示の status を JSON で起動する
///
/// `--config` を必ず渡し、作業ディレクトリを一時ディレクトリの下にする。渡さないと実行ファイルは
/// 作業ディレクトリの ".remote-merge.toml" を読み、テストが書いた設定の外に接続しうる。
/// 環境変数は全て消し、HOME・XDG の変数を一時ディレクトリの下に向け、PATH だけを引き継ぐ。
fn status_over_ssh(dirs: &mut TestDirs, config: &str, local_root: &Path) -> Output {
    let config_path = dirs.temp.path().join("scan-listing-config.toml");
    fs::write(&config_path, config).unwrap();
    dirs.assert_isolated_config_at(&config_path, local_root);
    let home: PathBuf = dirs.temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
    cmd.env_clear();
    cmd.env("HOME", &home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("XDG_DATA_HOME", dirs.temp.path().join("xdg-data"));
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.current_dir(&home);
    cmd.arg("--config").arg(&config_path);
    cmd.args([
        "status", "--left", "local", "--right", "develop", "--all", "--format", "json",
    ]);
    cmd.stdin(Stdio::null());
    cmd.output().expect("failed to execute status")
}

/// 左右の root_dir を `local_root`・`remote_root` にした設定で status を起動し、一覧を返す
fn listed_over_ssh(
    dirs: &mut TestDirs,
    local_root: &Path,
    remote_root: &Path,
) -> BTreeMap<String, String> {
    let config = gen_config(local_root, remote_root, None, dirs.server_port());
    let output = status_over_ssh(dirs, &config, local_root);
    statuses_by_path(&output)
}

/// 左右の root_dir に同じ symlink を置く
fn place_symlink_on_both_sides(dirs: &TestDirs, link: &str, target: &str) {
    place_symlink(&dirs.local_dir, link, target);
    place_symlink(&dirs.remote_dir, link, target);
}

// @kotowari[REQ-scan-006]
#[test]
fn status_lists_symlinks_themselves_with_their_target_text_over_ssh() {
    let same = [
        ("target.txt", "target\n"),
        ("a.txt", "same\n"),
        ("b.txt", "same\n"),
    ];
    let mut dirs = TestDirs::new_2way(&same, &same);
    for root in [&dirs.local_dir, &dirs.remote_dir] {
        // "dirlink" の先を中身のないディレクトリにして、配下のファイルの扱いに触れない
        fs::create_dir(root.join("emptydir")).unwrap();
    }
    place_symlink_on_both_sides(&dirs, "link", "target.txt");
    place_symlink_on_both_sides(&dirs, "dangling", "missing.txt");
    place_symlink_on_both_sides(&dirs, "dirlink", "emptydir");
    // リンク先の文字列だけが違い、リンク先の中身は同じ
    place_symlink(&dirs.local_dir, "retarget", "a.txt");
    place_symlink(&dirs.remote_dir, "retarget", "b.txt");

    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let statuses = listed_over_ssh(&mut dirs, &local_root, &remote_root);
    for path in ["link", "dangling", "dirlink"] {
        assert_eq!(
            statuses.get(path).map(String::as_str),
            Some("equal"),
            "{path}: {statuses:?}"
        );
    }
    assert_eq!(
        statuses.get("retarget").map(String::as_str),
        Some("modified"),
        "{statuses:?}"
    );
}

/// 一時ディレクトリの中に実在するディレクトリ（"f.txt" と "sub/g.txt" を置く）と、それを指す
/// symlink を左右それぞれに作り、symlink のパスを返す
fn linked_roots(dirs: &TestDirs) -> (PathBuf, PathBuf) {
    let files = [("f.txt", "same\n"), ("sub/g.txt", "same\n")];
    let temp = dirs.temp.path();
    let mut roots = Vec::new();
    for side in ["local", "remote"] {
        let target = temp.join(format!("{side}-target"));
        place_files(&target, &files);
        let root = temp.join(format!("{side}-root"));
        std::os::unix::fs::symlink(&target, &root).unwrap();
        roots.push(root);
    }
    (roots[0].clone(), roots[1].clone())
}

fn with_trailing_slash(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}/", path.display()))
}

fn only_the_linked_files_as_equal() -> BTreeMap<String, String> {
    ["f.txt", "sub/g.txt"]
        .into_iter()
        .map(|path| (path.to_string(), "equal".to_string()))
        .collect()
}

// @kotowari[REQ-scan-007]
#[test]
fn status_lists_the_files_below_a_root_dir_that_is_a_directory_symlink_over_ssh() {
    let mut dirs = TestDirs::new_2way(&[], &[]);
    let (local_root, remote_root) = linked_roots(&dirs);
    assert_eq!(
        listed_over_ssh(&mut dirs, &local_root, &remote_root),
        only_the_linked_files_as_equal()
    );
}

// @kotowari[REQ-scan-007]
#[test]
fn a_trailing_slash_on_a_symlinked_root_dir_does_not_change_the_list_over_ssh() {
    let mut dirs = TestDirs::new_2way(&[], &[]);
    let (local_root, remote_root) = linked_roots(&dirs);
    assert_eq!(
        listed_over_ssh(
            &mut dirs,
            &with_trailing_slash(&local_root),
            &with_trailing_slash(&remote_root)
        ),
        only_the_linked_files_as_equal()
    );
}

// @kotowari[EX-scan-003]
#[test]
fn status_lists_files_inside_a_directory_link_over_ssh() {
    let mut dirs = TestDirs::new_2way(&[], &[]);
    // 共有のディレクトリは試験サーバの home の下で、どちらの root_dir の外に置く
    let shared = dirs.temp.path().join("shared");
    place_files(&shared, &[("alpha.txt", "alpha\n"), ("beta.txt", "beta\n")]);
    let shared_text = shared.display().to_string();
    place_symlink_on_both_sides(&dirs, "linked", &shared_text);

    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let statuses = listed_over_ssh(&mut dirs, &local_root, &remote_root);
    for path in ["linked/alpha.txt", "linked/beta.txt"] {
        assert_eq!(
            statuses.get(path).map(String::as_str),
            Some("equal"),
            "{path}: {statuses:?}"
        );
    }
}
