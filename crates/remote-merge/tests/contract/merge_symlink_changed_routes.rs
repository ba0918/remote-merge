#![cfg(unix)]
//! 比べた後に書き込み先の symlink が替わったファイルを merge しない（docs/ir/merge/concurrency.md の
//! REQ-merge-033）ことが、ローカル・SSH・エージェントの三つの経路で同じ結果になることの契約テスト。
//!
//! 比べてから書くまでの間に書き込み先を変えるため、製品の実行時処理で比べ、書き込み先を変えてから
//! `execute_single_merge` を呼ぶ。製品の実行時処理は自分の tokio のランタイムを持つので、テストの
//! ランタイムの外のスレッドで動かす。リモートの経路は偽サーバを動かすテストのプロセスから、同じテストを
//! 子のプロセスとして起動して行う。

use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use remote_merge::config::load_config_from_paths;
use remote_merge::merge::executor::MergeDirection;
use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use remote_merge::service::merge_flow::{execute_single_merge, MergeContext, SingleMergeResult};
use remote_merge::service::types::{FileStatus, FileStatusKind};
use remote_merge::side::Side;
use tempfile::TempDir;

use super::agent_support::{AgentFixture, Setup};
use super::server_faults::Faults;

const LINK: &str = "link.txt";
/// 比べた後に書き込み先に置かれた通常ファイルの中身
const THIRD_PARTY: &str = "third party\n";
/// 比べた後に書き込み先の symlink を作り直したリンク先
const REPOINTED: &str = "c.txt";
/// 比べるときの UTF-8 でないリンク先（0xff は UTF-8 にならないバイト）
const NON_UTF8_COMPARED: &[u8] = b"\xff.txt";
/// 比べた後に作り直した UTF-8 でないリンク先。UTF-8 でないバイトだけが比べたときと違う
const NON_UTF8_REPOINTED: &[u8] = b"\xfe.txt";
/// 前に空白があるリンク先
const LEADING_SPACE: &str = " a.txt";
/// "\" を含むリンク先
const BACKSLASH: &str = "sub\\b.txt";

const CHILD_CONFIG: &str = "REMOTE_MERGE_SYMLINK_CHANGE_CONFIG";
const CHILD_ROOT: &str = "REMOTE_MERGE_SYMLINK_CHANGE_ROOT";
const CHILD_CHANGE: &str = "REMOTE_MERGE_SYMLINK_CHANGE_KIND";
const CHILD_RESULT: &str = "REMOTE_MERGE_SYMLINK_CHANGE_RESULT";

/// 書き込み先の symlink の始めの状態と、比べた後、merge の前にそれに起こす変化
#[derive(Debug, Clone, Copy)]
enum Change {
    /// b.txt を指す symlink が通常ファイルに替わる
    ToRegularFile,
    /// b.txt を指す symlink が別のリンク先を指すように作り直される
    Repointed,
    /// ないファイルを指す（リンク切れの）symlink のまま変わらない
    UnchangedDangling,
    /// UTF-8 でないリンク先を指す symlink のまま変わらない
    UnchangedNonUtf8,
    /// 前に空白があるリンク先を指す symlink のまま変わらない
    UnchangedLeadingSpace,
    /// "\" を含むリンク先を指す symlink のまま変わらない
    UnchangedBackslash,
    /// b.txt を指す symlink のまま変わらず、親ディレクトリ（root_dir）が探索だけできる（0300）ように
    /// なる
    ParentSearchOnly,
    /// UTF-8 でないリンク先を指す symlink が、UTF-8 でないバイトだけが違う別のリンク先を指すように
    /// 作り直される
    RepointedNonUtf8,
}

impl Change {
    fn name(self) -> &'static str {
        match self {
            Self::ToRegularFile => "file",
            Self::Repointed => "repointed",
            Self::UnchangedDangling => "dangling",
            Self::UnchangedNonUtf8 => "non-utf8",
            Self::UnchangedLeadingSpace => "leading-space",
            Self::UnchangedBackslash => "backslash",
            Self::ParentSearchOnly => "parent-search-only",
            Self::RepointedNonUtf8 => "repointed-non-utf8",
        }
    }

    fn from_name(name: &str) -> Self {
        match name {
            "file" => Self::ToRegularFile,
            "repointed" => Self::Repointed,
            "dangling" => Self::UnchangedDangling,
            "non-utf8" => Self::UnchangedNonUtf8,
            "leading-space" => Self::UnchangedLeadingSpace,
            "backslash" => Self::UnchangedBackslash,
            "parent-search-only" => Self::ParentSearchOnly,
            "repointed-non-utf8" => Self::RepointedNonUtf8,
            other => panic!("{other}"),
        }
    }

    /// 比べるときの書き込み先の link.txt のリンク先
    fn destination_link(self) -> PathBuf {
        match self {
            Self::ToRegularFile | Self::Repointed | Self::ParentSearchOnly => "b.txt".into(),
            Self::UnchangedDangling => "missing.txt".into(),
            Self::UnchangedNonUtf8 | Self::RepointedNonUtf8 => non_utf8_path(NON_UTF8_COMPARED),
            Self::UnchangedLeadingSpace => LEADING_SPACE.into(),
            Self::UnchangedBackslash => BACKSLASH.into(),
        }
    }

    fn apply(self, root: &Path) {
        let link = root.join(LINK);
        match self {
            Self::ToRegularFile => {
                fs::remove_file(&link).unwrap();
                fs::write(&link, THIRD_PARTY).unwrap();
            }
            Self::Repointed => {
                fs::remove_file(&link).unwrap();
                symlink(REPOINTED, &link).unwrap();
            }
            Self::RepointedNonUtf8 => {
                fs::remove_file(&link).unwrap();
                symlink(non_utf8_path(NON_UTF8_REPOINTED), &link).unwrap();
            }
            Self::ParentSearchOnly => set_mode(root, 0o300),
            Self::UnchangedDangling
            | Self::UnchangedNonUtf8
            | Self::UnchangedLeadingSpace
            | Self::UnchangedBackslash => {}
        }
    }
}

/// 一つの経路の merge の結果。失敗は文言を持つ
#[derive(Debug)]
enum Outcome {
    Merged,
    Skipped(String),
    Failed(String),
}

impl Outcome {
    fn to_text(&self) -> String {
        match self {
            Self::Merged => "merged".into(),
            Self::Skipped(reason) => format!("skipped: {reason}"),
            Self::Failed(error) => format!("failed: {error}"),
        }
    }

    fn from_text(text: &str) -> Self {
        if let Some(reason) = text.strip_prefix("skipped: ") {
            return Self::Skipped(reason.into());
        }
        if let Some(error) = text.strip_prefix("failed: ") {
            return Self::Failed(error.into());
        }
        assert_eq!(text, "merged");
        Self::Merged
    }

    /// 比べたときから書き込み先が変わったとして failed に出たか
    fn failed_as_changed(&self) -> bool {
        matches!(self, Self::Failed(error) if error.contains("changed"))
    }
}

fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn non_utf8_path(bytes: &[u8]) -> PathBuf {
    std::ffi::OsStr::from_bytes(bytes).into()
}

/// 読み込み元の link.txt は a.txt を、書き込み先の link.txt は `change` の始めのリンク先を指す状態を作る
fn put_links(source_root: &Path, destination_root: &Path, change: Change) {
    for root in [source_root, destination_root] {
        for name in ["a.txt", "b.txt", REPOINTED] {
            fs::write(root.join(name), format!("{name}\n")).unwrap();
        }
    }
    symlink("a.txt", source_root.join(LINK)).unwrap();
    symlink(change.destination_link(), destination_root.join(LINK)).unwrap();
}

/// `source` と `destination` を比べ、書き込み先の root_dir `destination_root` に `change` を起こしてから
/// link.txt を merge する
fn merge_after_change(
    core: &mut CoreRuntime,
    source: &Side,
    destination: &Side,
    destination_root: &Path,
    change: Change,
) -> Outcome {
    let source_tree = core.fetch_tree_recursive(source, 100, true).unwrap();
    let destination_tree = core.fetch_tree_recursive(destination, 100, true).unwrap();
    change.apply(destination_root);
    let session_id = if core.config.backup.enabled {
        core.reserve_backup_session(&[]).unwrap()
    } else {
        "unused".into()
    };
    let statuses = [FileStatus {
        path: LINK.into(),
        status: FileStatusKind::Modified,
        hunks: None,
        ref_badge: None,
    }];
    let expected = Default::default();
    let mut ctx = MergeContext {
        left: source,
        right: destination,
        left_tree: &source_tree,
        right_tree: &destination_tree,
        direction: MergeDirection::LeftToRight,
        core,
        with_permissions: false,
        force: false,
        statuses: &statuses,
        session_id: &session_id,
        expected_target_contents: &expected,
    };
    let outcome = match execute_single_merge(&mut ctx, LINK) {
        Ok(SingleMergeResult::Merged(_)) => Outcome::Merged,
        Ok(SingleMergeResult::Skipped(skipped)) => Outcome::Skipped(skipped.reason),
        Err(error) => Outcome::Failed(error.to_string()),
    };
    if core.config.backup.enabled {
        core.finish_backup_session(&session_id);
    }
    outcome
}

/// テストのランタイムの外のスレッドで `work` を動かす
fn off_test_runtime<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::spawn(work).join().unwrap()
}

/// ローカルの経路: 書き込み先の develop をこのマシンのディレクトリにして merge する
fn local_route(change: Change, backup: bool) -> (Outcome, TempDir) {
    let source = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    put_links(source.path(), destination.path(), change);
    let config_path = source.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = {backup}\n",
            source.path().display().to_string(),
            destination.path().display().to_string()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap());
    let root = destination.path().to_path_buf();
    let outcome = off_test_runtime(move || {
        let mut core = CoreRuntime::with_targets(config, targets);
        let outcome = merge_after_change(
            &mut core,
            &Side::Local,
            &Side::Remote("develop".into()),
            &root,
            change,
        );
        drop(core);
        outcome
    });
    (outcome, destination)
}

/// 子のプロセスなら、偽サーバの fixture へ merge した結果を書いて true を返す
fn serve_child() -> bool {
    let Ok(config) = std::env::var(CHILD_CONFIG) else {
        return false;
    };
    let root = PathBuf::from(std::env::var(CHILD_ROOT).unwrap());
    let change = Change::from_name(&std::env::var(CHILD_CHANGE).unwrap());
    let result = PathBuf::from(std::env::var(CHILD_RESULT).unwrap());
    let outcome = off_test_runtime(move || {
        let config = load_config_from_paths(Some(Path::new(&config)), None).unwrap();
        let mut core = CoreRuntime::new(config);
        let destination = Side::Remote("fixture".into());
        core.connect_if_remote(&destination).unwrap();
        let outcome = merge_after_change(&mut core, &Side::Local, &destination, &root, change);
        core.disconnect_all();
        drop(core);
        outcome
    });
    fs::write(result, outcome.to_text()).unwrap();
    true
}

/// リモートの経路: 偽サーバの fixture を書き込み先にして、子のプロセスで merge する
async fn remote_route(
    setup: Setup,
    change: Change,
    backup: bool,
    test: &str,
) -> (Outcome, AgentFixture) {
    let fixture = AgentFixture::new(setup, Faults::default()).await;
    if !backup {
        fixture.disable_backup();
    }
    put_links(&fixture.local(), &fixture.remote(), change);
    let result = fixture.home.path().join("child-result.txt");
    let output = Command::new(std::env::current_exe().unwrap())
        .env("HOME", fixture.home.path())
        .env("XDG_CACHE_HOME", fixture.home.path().join("cache"))
        .env("XDG_CONFIG_HOME", fixture.home.path().join("config"))
        .env("XDG_DATA_HOME", fixture.home.path().join("data"))
        .env(
            "REMOTE_MERGE_AGENT_BINARY",
            env!("CARGO_BIN_EXE_remote-merge"),
        )
        .env(CHILD_CONFIG, &fixture.config)
        .env(CHILD_ROOT, fixture.remote())
        .env(CHILD_CHANGE, change.name())
        .env(CHILD_RESULT, &result)
        .args(["--exact", test])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let outcome = Outcome::from_text(&fs::read_to_string(result).unwrap());
    (outcome, fixture)
}

fn ssh_only() -> Setup {
    Setup {
        agent_disabled: true,
        ..Setup::default()
    }
}

/// `path` が通常ファイルならその中身。symlink かなければ None
fn regular_file_content(path: &Path) -> Option<String> {
    let is_file = path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_file());
    is_file.then(|| fs::read_to_string(path).unwrap())
}

/// エージェントの経路で、書き込み先の確認をエージェントが受け持ったこと
fn assert_agent_served(fixture: &AgentFixture) {
    let requests = fixture.server.faults().agent_requests();
    assert!(
        requests.iter().any(|r| r == "ReadEntryLink"),
        "{requests:?}"
    );
}

const EX_053: &str = "merge_symlink_changed_routes::ex_merge_053_a_destination_symlink_replaced_by_a_regular_file_after_comparison_fails_and_the_file_is_kept_on_all_three_routes";

// @kotowari[REQ-merge-033, EX-merge-053]
#[tokio::test(flavor = "multi_thread")]
async fn ex_merge_053_a_destination_symlink_replaced_by_a_regular_file_after_comparison_fails_and_the_file_is_kept_on_all_three_routes(
) {
    if serve_child() {
        return;
    }
    let (local, local_root) = local_route(Change::ToRegularFile, false);
    let (ssh, ssh_fixture) = remote_route(ssh_only(), Change::ToRegularFile, false, EX_053).await;
    let (agent, agent_fixture) =
        remote_route(Setup::default(), Change::ToRegularFile, false, EX_053).await;

    let routes = [
        (&local, local_root.path().to_path_buf()),
        (&ssh, ssh_fixture.remote()),
        (&agent, agent_fixture.remote()),
    ];
    let observed = routes.map(|(outcome, root)| {
        (
            outcome.failed_as_changed(),
            regular_file_content(&root.join(LINK)),
        )
    });
    let expected = (true, Some(THIRD_PARTY.to_string()));
    assert_eq!(
        observed,
        [expected.clone(), expected.clone(), expected],
        "local, ssh, agent: {local:?}, {ssh:?}, {agent:?}"
    );
    assert_agent_served(&agent_fixture);
}

const EX_054: &str = "merge_symlink_changed_routes::ex_merge_054_a_destination_symlink_repointed_after_comparison_fails_and_keeps_its_new_target_on_all_three_routes";

// @kotowari[REQ-merge-033, EX-merge-054]
#[tokio::test(flavor = "multi_thread")]
async fn ex_merge_054_a_destination_symlink_repointed_after_comparison_fails_and_keeps_its_new_target_on_all_three_routes(
) {
    if serve_child() {
        return;
    }
    let (local, local_root) = local_route(Change::Repointed, true);
    let (ssh, ssh_fixture) = remote_route(ssh_only(), Change::Repointed, true, EX_054).await;
    let (agent, agent_fixture) =
        remote_route(Setup::default(), Change::Repointed, true, EX_054).await;

    let routes = [
        (&local, local_root.path().to_path_buf()),
        (&ssh, ssh_fixture.remote()),
        (&agent, agent_fixture.remote()),
    ];
    let observed = routes.map(|(outcome, root)| {
        (
            outcome.failed_as_changed(),
            fs::read_link(root.join(LINK)).ok(),
        )
    });
    let expected = (true, Some(PathBuf::from(REPOINTED)));
    assert_eq!(
        observed,
        [expected.clone(), expected.clone(), expected],
        "local, ssh, agent: {local:?}, {ssh:?}, {agent:?}"
    );
    assert_agent_served(&agent_fixture);
}

/// 変わっていない書き込み先の symlink が三つの経路で置き換えられ、a.txt を指すことを確かめる
async fn assert_unchanged_destination_is_replaced(change: Change, test: &str) {
    let (local, local_root) = local_route(change, false);
    let (ssh, ssh_fixture) = remote_route(ssh_only(), change, false, test).await;
    let (agent, agent_fixture) = remote_route(Setup::default(), change, false, test).await;

    let routes = [
        (&local, local_root.path().to_path_buf()),
        (&ssh, ssh_fixture.remote()),
        (&agent, agent_fixture.remote()),
    ];
    let observed = routes.map(|(outcome, root)| {
        (
            matches!(outcome, Outcome::Merged),
            fs::read_link(root.join(LINK)).ok(),
        )
    });
    let expected = (true, Some(PathBuf::from("a.txt")));
    assert_eq!(
        observed,
        [expected.clone(), expected.clone(), expected],
        "local, ssh, agent: {local:?}, {ssh:?}, {agent:?}"
    );
    assert_agent_served(&agent_fixture);
}

const DANGLING: &str = "merge_symlink_changed_routes::an_unchanged_dangling_destination_symlink_is_replaced_on_all_three_routes";

// @kotowari[REQ-merge-033]
#[tokio::test(flavor = "multi_thread")]
async fn an_unchanged_dangling_destination_symlink_is_replaced_on_all_three_routes() {
    if serve_child() {
        return;
    }
    assert_unchanged_destination_is_replaced(Change::UnchangedDangling, DANGLING).await;
}

const LEADING_SPACE_TEST: &str = "merge_symlink_changed_routes::an_unchanged_destination_symlink_whose_target_has_a_leading_space_is_replaced_on_all_three_routes";

// @kotowari[REQ-merge-033]
#[tokio::test(flavor = "multi_thread")]
async fn an_unchanged_destination_symlink_whose_target_has_a_leading_space_is_replaced_on_all_three_routes(
) {
    if serve_child() {
        return;
    }
    assert_unchanged_destination_is_replaced(Change::UnchangedLeadingSpace, LEADING_SPACE_TEST)
        .await;
}

const BACKSLASH_TEST: &str = "merge_symlink_changed_routes::an_unchanged_destination_symlink_whose_target_has_a_backslash_is_replaced_on_all_three_routes";

// @kotowari[REQ-merge-033]
#[tokio::test(flavor = "multi_thread")]
async fn an_unchanged_destination_symlink_whose_target_has_a_backslash_is_replaced_on_all_three_routes(
) {
    if serve_child() {
        return;
    }
    assert_unchanged_destination_is_replaced(Change::UnchangedBackslash, BACKSLASH_TEST).await;
}

const PARENT_SEARCH_ONLY: &str = "merge_symlink_changed_routes::an_unchanged_destination_symlink_whose_parent_became_search_only_is_replaced_on_all_three_routes";

// @kotowari[REQ-merge-033]
#[tokio::test(flavor = "multi_thread")]
async fn an_unchanged_destination_symlink_whose_parent_became_search_only_is_replaced_on_all_three_routes(
) {
    if serve_child() {
        return;
    }
    // 権限に関わらず読める利用者（root）では、読めない親ディレクトリを作れない
    let probe = TempDir::new().unwrap();
    set_mode(probe.path(), 0o300);
    let unreadable = fs::read_dir(probe.path()).is_err();
    set_mode(probe.path(), 0o700);
    if !unreadable {
        eprintln!("skipped: a directory with mode 0300 is readable by this user");
        return;
    }
    let change = Change::ParentSearchOnly;
    let (local, local_root) = local_route(change, false);
    let (ssh, ssh_fixture) = remote_route(ssh_only(), change, false, PARENT_SEARCH_ONLY).await;
    let (agent, agent_fixture) =
        remote_route(Setup::default(), change, false, PARENT_SEARCH_ONLY).await;

    let routes = [
        (&local, local_root.path().to_path_buf()),
        (&ssh, ssh_fixture.remote()),
        (&agent, agent_fixture.remote()),
    ];
    let observed = routes.map(|(outcome, root)| {
        // 一時ディレクトリを消せるよう、確かめる前に権限を戻す
        set_mode(&root, 0o700);
        (
            matches!(outcome, Outcome::Merged),
            fs::read_link(root.join(LINK)).ok(),
        )
    });
    let expected = (true, Some(PathBuf::from("a.txt")));
    assert_eq!(
        observed,
        [expected.clone(), expected.clone(), expected],
        "local, ssh, agent: {local:?}, {ssh:?}, {agent:?}"
    );
    assert_agent_served(&agent_fixture);
}

/// 書き込み先の symlink のリンク先が UTF-8 でないとき、三つの経路とも failed になり、書き込み先の
/// symlink は `expected_link` を指したまま残ることを確かめる
async fn assert_non_utf8_destination_is_kept(change: Change, expected_link: &[u8], test: &str) {
    let (local, local_root) = local_route(change, false);
    let (ssh, ssh_fixture) = remote_route(ssh_only(), change, false, test).await;
    let (agent, agent_fixture) = remote_route(Setup::default(), change, false, test).await;

    let routes = [
        (&local, local_root.path().to_path_buf()),
        (&ssh, ssh_fixture.remote()),
        (&agent, agent_fixture.remote()),
    ];
    let observed = routes.map(|(outcome, root)| {
        (
            matches!(outcome, Outcome::Failed(_)),
            fs::read_link(root.join(LINK)).ok(),
        )
    });
    let expected = (true, Some(non_utf8_path(expected_link)));
    assert_eq!(
        observed,
        [expected.clone(), expected.clone(), expected],
        "local, ssh, agent: {local:?}, {ssh:?}, {agent:?}"
    );
    assert_agent_served(&agent_fixture);
}

const REPOINTED_NON_UTF8: &str = "merge_symlink_changed_routes::ex_merge_055_a_destination_symlink_repointed_to_another_non_utf8_target_fails_and_keeps_its_new_target_on_all_three_routes";

// @kotowari[REQ-merge-033, EX-merge-055]
#[tokio::test(flavor = "multi_thread")]
async fn ex_merge_055_a_destination_symlink_repointed_to_another_non_utf8_target_fails_and_keeps_its_new_target_on_all_three_routes(
) {
    if serve_child() {
        return;
    }
    assert_non_utf8_destination_is_kept(
        Change::RepointedNonUtf8,
        NON_UTF8_REPOINTED,
        REPOINTED_NON_UTF8,
    )
    .await;
}

const NON_UTF8: &str = "merge_symlink_changed_routes::an_unchanged_destination_symlink_with_a_non_utf8_target_fails_and_is_kept_on_all_three_routes";

// @kotowari[REQ-merge-033]
#[tokio::test(flavor = "multi_thread")]
async fn an_unchanged_destination_symlink_with_a_non_utf8_target_fails_and_is_kept_on_all_three_routes(
) {
    if serve_child() {
        return;
    }
    assert_non_utf8_destination_is_kept(Change::UnchangedNonUtf8, NON_UTF8_COMPARED, NON_UTF8)
        .await;
}
