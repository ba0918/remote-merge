#![cfg(unix)]
//! 削除・権限の変更・子の一覧が三つの経路で同じ結果になること（docs/ir/paths/operations.md）の
//! 契約テスト。
//!
//! 経路ごとに同じ状態の一時ディレクトリを作り、同じ操作をして、成否と変わったものを比べる。
//! - ローカル: 製品のローカルの操作
//! - SSH: 偽サーバ（このマシンのシェルでコマンドを実行する）につないだ製品の実行時処理
//! - エージェント: エージェントのプロセスを起動し、通信の形式で要求する

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use remote_merge::agent::client::AgentClient;
use remote_merge::runtime::CoreRuntime;
use remote_merge::side::Side;
use remote_merge::tree::{FileNode, NodeKind};
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::drive::scan;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::{ScanError, ScanFailure, ScanRequest, ScanScope};

use super::listing_support::{agent_children, UnreadableLink};
use remote_merge_core::machine_ops::PermissionMode;
use remote_merge_core::op_failure::{OpError, OpFailure, Operation};
use remote_merge_core::relative_path::EntryPath;

use super::server_faults::Faults;
use super::ssh_server::TestServer;

pub(super) const MAX_ENTRIES: usize = 10_000;

const SSH_CONFIG: &str = "REMOTE_MERGE_OPERATIONS_CONFIG";
const SSH_REQUEST: &str = "REMOTE_MERGE_OPERATIONS_REQUEST";
const SSH_RESULT: &str = "REMOTE_MERGE_OPERATIONS_RESULT";
/// SSH の経路の操作を受け持つテストのプロセスで動かすテスト
const SSH_SERVER_TEST: &str = "path_operations_agree::a_listing_with_an_unreadable_link_target_stops_with_an_error_naming_the_link";

/// SSH の経路の操作を受け持つプロセスなら、`SSH_REQUEST` の操作（"remove\tパス"、
/// "chmod\tパス\t8 進の権限"、"list\tパス"、走査の "scan"、失敗の種類だけを返す
/// "failure\t操作\tパス"）を行って結果の文字列を `SSH_RESULT` に書き、true を返す
fn serve_ssh_request() -> bool {
    let Ok(config) = std::env::var(SSH_CONFIG) else {
        return false;
    };
    let request = std::env::var(SSH_REQUEST).unwrap();
    let config =
        remote_merge::config::load_config_from_paths(Some(Path::new(&config)), None).unwrap();
    let mut runtime = CoreRuntime::new(config);
    runtime.connect("fixture").unwrap();
    let side = Side::Remote("fixture".into());
    let fields: Vec<&str> = request.split('\t').collect();
    let result = match fields.as_slice() {
        ["remove", path] => operation_text(runtime.remove_file(&side, path)),
        ["chmod", path, mode] => {
            operation_text(runtime.chmod_file(&side, path, u32::from_str_radix(mode, 8).unwrap()))
        }
        ["failure", operation, path] => failure_kind_text(match *operation {
            "read" => runtime.read_file_bytes(&side, path, false).map(|_| ()),
            "remove" => runtime.remove_file(&side, path),
            "chmod" => runtime.chmod_file(&side, path, 0o644),
            "write" => runtime.write_file(&side, path, "x"),
            "symlink" => runtime.create_symlink(&side, path, "t"),
            other => panic!("{other}"),
        }),
        ["list", path] => listing_text(
            runtime
                .list_children(&side, path)
                .map_err(|e| e.to_string()),
        ),
        ["tree"] => super::scan_routes_metadata::tree_json(
            runtime
                .fetch_tree_recursive(&side, MAX_ENTRIES, true)
                .map_err(|e| e.to_string()),
        ),
        ["scan"] => super::scan_times_agree::times_text(
            runtime
                .fetch_tree_recursive(&side, MAX_ENTRIES, true)
                .map(|tree| tree.nodes)
                .map_err(|e| e.to_string()),
        ),
        other => panic!("{other:?}"),
    };
    fs::write(std::env::var(SSH_RESULT).unwrap(), result).unwrap();
    true
}

fn operation_text(result: anyhow::Result<()>) -> String {
    match result {
        Ok(()) => "ok".into(),
        Err(error) => format!("error: {error}"),
    }
}

/// 操作の失敗の種類（成功なら "ok"）
fn failure_kind_text(result: anyhow::Result<()>) -> String {
    match result {
        Ok(()) => "ok".into(),
        Err(error) => match remote_merge_core::op_failure::failure_of(&error) {
            Some(failure) => format!("{failure:?}"),
            None => format!("untyped: {error:#}"),
        },
    }
}

fn operation_outcome(text: String) -> Result<(), String> {
    match text.strip_prefix("error: ") {
        Some(error) => Err(error.to_string()),
        None => Ok(()),
    }
}

fn listing_text(listing: Result<ChildListing, String>) -> String {
    format!("{:?}", entries(listing))
}

/// 一つの経路の root_dir と、その外のディレクトリ
pub(super) struct RouteDir {
    pub(super) root: PathBuf,
    outside: PathBuf,
}

impl RouteDir {
    fn new(base: &Path, name: &str, state: impl Fn(&Path, &Path)) -> Self {
        let root = base.join(name).join("root");
        let outside = base.join(name).join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        state(&root, &outside);
        Self { root, outside }
    }
}

/// 三つの経路の root_dir と、SSH の経路の偽サーバ
pub(super) struct Routes {
    home: tempfile::TempDir,
    _server: TestServer,
    config: PathBuf,
    pub(super) local: RouteDir,
    pub(super) ssh: RouteDir,
    pub(super) agent: RouteDir,
}

impl Routes {
    pub(super) async fn new(state: impl Fn(&Path, &Path)) -> Self {
        let home = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let local = RouteDir::new(home.path(), "local", &state);
        let ssh = RouteDir::new(home.path(), "ssh", &state);
        let agent = RouteDir::new(home.path(), "agent", &state);
        let server =
            TestServer::filesystem_with_faults(home.path(), false, Faults::default()).await;
        let config = home.path().join("config.toml");
        fs::write(
            &config,
            format!(
                "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = false\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
                local.root.to_str().unwrap(),
                server.port(),
                ssh.root.to_str().unwrap(),
            ),
        )
        .unwrap();
        Self {
            home,
            _server: server,
            config,
            local,
            ssh,
            agent,
        }
    }

    /// SSH の経路の設定に exclude を足す。ローカルとエージェントの経路には呼び出し側が渡す
    pub(super) fn with_exclude(self, exclude: &[String]) -> Self {
        let mut config = fs::read_to_string(&self.config).unwrap();
        config.push_str(&format!("[filter]\nexclude = {exclude:?}\n"));
        fs::write(&self.config, config).unwrap();
        self
    }

    /// 製品の実行時処理を SSH でつないで `request` の操作を行い、結果の文字列を返す。
    /// 実行時処理は HOME の known_hosts を読み書きするので、HOME を一時ディレクトリにした
    /// テストのプロセスで動かす（[`serve_ssh_request`]）
    pub(super) fn over_ssh(&self, request: &str) -> String {
        let result = self.home.path().join("ssh-result");
        let output = Command::new(std::env::current_exe().unwrap())
            .env("HOME", self.home.path())
            .env("XDG_CACHE_HOME", self.home.path().join("cache"))
            .env("XDG_CONFIG_HOME", self.home.path().join("config"))
            .env(SSH_CONFIG, &self.config)
            .env(SSH_REQUEST, request)
            .env(SSH_RESULT, &result)
            .args(["--exact", SSH_SERVER_TEST])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        fs::read_to_string(result).unwrap()
    }

    /// エージェントのプロセスを起動して `operation` を行う
    pub(super) fn over_agent<T>(&self, operation: impl FnOnce(&mut Agent) -> T) -> T {
        let mut agent = Agent::start(&self.agent.root, self.home.path());
        let result = operation(&mut agent);
        agent.stop();
        result
    }
}

pub(super) struct Agent {
    child: Child,
    pub(super) client: AgentClient<std::process::ChildStdout, std::process::ChildStdin>,
}

impl Agent {
    fn start(root: &Path, cwd: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
            .arg("agent")
            .arg("--root")
            .arg(root)
            .current_dir(cwd)
            .env("RUST_LOG", "off")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let client =
            AgentClient::connect(child.stdout.take().unwrap(), child.stdin.take().unwrap())
                .unwrap();
        Self { child, client }
    }

    fn stop(mut self) {
        let _ = self.client.shutdown();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 三つの経路の結果。エラーは文言のまま持つ
struct Outcomes<T> {
    local: Result<T, String>,
    ssh: Result<T, String>,
    agent: Result<T, String>,
}

impl<T: std::fmt::Debug> Outcomes<T> {
    fn all(&self) -> [(&str, &Result<T, String>); 3] {
        [
            ("local", &self.local),
            ("ssh", &self.ssh),
            ("agent", &self.agent),
        ]
    }

    fn assert_all_ok(&self) {
        for (route, outcome) in self.all() {
            assert!(outcome.is_ok(), "{route}: {outcome:?}");
        }
    }

    /// どの経路でも失敗し、文言が `words` のどれも（大文字小文字を区別せずに）含む
    fn assert_all_fail_naming(&self, words: &[&str]) {
        for (route, outcome) in self.all() {
            let Err(error) = outcome else {
                panic!("{route} succeeded: {outcome:?}");
            };
            let error = error.to_lowercase();
            for word in words {
                assert!(
                    error.contains(&word.to_lowercase()),
                    "{route}: {error:?} lacks {word:?}"
                );
            }
        }
    }
}

/// ローカルの経路の失敗の文言（操作の失敗の型から一か所で作るもの）
fn local_error(operation: Operation, path: &EntryPath, failure: OpFailure) -> String {
    OpError::new(operation, path.as_typed(), failure).to_string()
}

fn remove_on_all_routes(routes: &Routes, path: &'static str) -> Outcomes<()> {
    let rel = &remote_merge_core::relative_path::EntryPath::parse(path).unwrap();
    let local = remote_merge_core::machine_ops::remove(&routes.local.root, rel)
        .map_err(|failure| local_error(Operation::Remove, rel, failure));
    let ssh = operation_outcome(routes.over_ssh(&format!("remove\t{path}")));
    let agent = routes.over_agent(|agent| agent.client.remove(rel).map_err(|e| e.to_string()));
    Outcomes { local, ssh, agent }
}

fn chmod_on_all_routes(routes: &Routes, path: &'static str, mode: u32) -> Outcomes<()> {
    let rel = &remote_merge_core::relative_path::EntryPath::parse(path).unwrap();
    let local = PermissionMode::new(mode)
        .map_err(|error| format!("cannot change permissions of {path}: {error}"))
        .and_then(|mode| {
            remote_merge_core::machine_ops::chmod(&routes.local.root, rel, mode)
                .map_err(|failure| local_error(Operation::Chmod, rel, failure))
        });
    let ssh = operation_outcome(routes.over_ssh(&format!("chmod\t{path}\t{mode:o}")));
    let agent = routes.over_agent(|agent| agent.client.chmod(rel, mode).map_err(|e| e.to_string()));
    Outcomes { local, ssh, agent }
}

/// 一覧の項目のうち、経路で一致させる値
#[derive(Debug, PartialEq)]
struct Entry {
    name: String,
    kind: String,
    size: Option<u64>,
    permissions: Option<u32>,
    mtime: Option<(i64, u32)>,
    link_is_dir: bool,
}

fn entries(listing: Result<ChildListing, String>) -> Result<Vec<Entry>, String> {
    let nodes = match listing? {
        ChildListing::Children(nodes) => nodes,
        other => return Err(format!("not children: {other:?}")),
    };
    let mut entries: Vec<Entry> = nodes.iter().map(entry).collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn entry(node: &FileNode) -> Entry {
    Entry {
        name: node.name.clone(),
        kind: match &node.kind {
            NodeKind::File => "file".into(),
            NodeKind::Directory => "directory".into(),
            NodeKind::Symlink { target } => format!("symlink to {target:?}"),
        },
        size: node.size,
        permissions: node.permissions,
        mtime: node
            .mtime
            .map(|mtime| (mtime.timestamp(), mtime.timestamp_subsec_nanos())),
        link_is_dir: node.link_is_dir,
    }
}

/// 三つの経路の一覧（比べる値の文字列）。ローカルの一覧はその値も返す
fn list_on_all_routes(routes: &Routes, path: &str) -> (Vec<Entry>, [String; 3]) {
    list_on_all_routes_excluding(routes, path, &[])
}

/// [`list_on_all_routes`] の、ローカルとエージェントの経路に exclude を渡すもの。SSH の経路の
/// exclude は [`Routes::with_exclude`] で設定に書く
fn list_on_all_routes_excluding(
    routes: &Routes,
    path: &str,
    exclude: &[String],
) -> (Vec<Entry>, [String; 3]) {
    let rel = &remote_merge_core::relative_path::RelPath::parse(path).unwrap();
    let local = remote_merge_engine::local_io::list_children(&routes.local.root, exclude, rel)
        .map_err(|error| error.to_string());
    let local_text = listing_text(local.clone());
    let ssh = routes.over_ssh(&format!("list\t{path}"));
    let agent =
        routes.over_agent(|agent| listing_text(agent_children(&mut agent.client, rel, exclude)));
    (entries(local).unwrap(), [local_text, ssh, agent])
}

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// 更新時刻をナノ秒まで決める（symlink はリンク自身）
pub(super) fn set_mtime(path: &Path, stamp: &str) {
    let status = Command::new("touch")
        .args(["-h", "-d", stamp])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

// @kotowari[REQ-paths-013, EX-paths-026]
#[tokio::test(flavor = "multi_thread")]
async fn removing_a_terminal_symlink_removes_only_the_link_on_all_three_routes() {
    let routes = Routes::new(|root, _| {
        fs::write(root.join("t.txt"), "target\n").unwrap();
        symlink("t.txt", root.join("l")).unwrap();
    })
    .await;

    remove_on_all_routes(&routes, "l").assert_all_ok();

    for dir in [&routes.local, &routes.ssh, &routes.agent] {
        assert!(
            dir.root.join("l").symlink_metadata().is_err(),
            "{:?}",
            dir.root
        );
        assert_eq!(
            fs::read_to_string(dir.root.join("t.txt")).unwrap(),
            "target\n"
        );
    }
}

// @kotowari[EX-paths-027]
#[tokio::test(flavor = "multi_thread")]
async fn removing_a_directory_fails_and_keeps_it_on_all_three_routes() {
    let routes = Routes::new(|root, _| {
        fs::create_dir(root.join("d")).unwrap();
        fs::write(root.join("d/inner.txt"), "inner\n").unwrap();
    })
    .await;

    remove_on_all_routes(&routes, "d").assert_all_fail_naming(&["remove", "d"]);

    for dir in [&routes.local, &routes.ssh, &routes.agent] {
        assert_eq!(
            fs::read_to_string(dir.root.join("d/inner.txt")).unwrap(),
            "inner\n"
        );
    }
}

// @kotowari[EX-paths-028]
#[tokio::test(flavor = "multi_thread")]
async fn changing_permissions_through_an_intermediate_symlink_outside_the_root_succeeds_on_all_three_routes(
) {
    let routes = Routes::new(|root, outside| {
        fs::write(outside.join("a.txt"), "shared\n").unwrap();
        fs::set_permissions(outside.join("a.txt"), fs::Permissions::from_mode(0o644)).unwrap();
        symlink(outside, root.join("shared")).unwrap();
    })
    .await;

    chmod_on_all_routes(&routes, "shared/a.txt", 0o640).assert_all_ok();

    for dir in [&routes.local, &routes.ssh, &routes.agent] {
        assert_eq!(
            mode_of(&dir.outside.join("a.txt")),
            0o640,
            "{:?}",
            dir.outside
        );
    }
}

// @kotowari[REQ-paths-015, EX-paths-031]
#[tokio::test(flavor = "multi_thread")]
async fn a_mode_with_setuid_is_refused_without_change_on_all_three_routes() {
    let routes = Routes::new(|root, _| {
        fs::write(root.join("a.sh"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(root.join("a.sh"), fs::Permissions::from_mode(0o644)).unwrap();
    })
    .await;

    chmod_on_all_routes(&routes, "a.sh", 0o4755).assert_all_fail_naming(&["permission", "a.sh"]);

    for dir in [&routes.local, &routes.ssh, &routes.agent] {
        assert_eq!(mode_of(&dir.root.join("a.sh")), 0o644, "{:?}", dir.root);
    }
}

// @kotowari[REQ-paths-018, EX-paths-035]
#[tokio::test(flavor = "multi_thread")]
async fn changing_a_setgid_directory_to_775_keeps_setgid_on_all_three_routes() {
    let routes = Routes::new(|root, _| {
        fs::create_dir(root.join("shared")).unwrap();
        fs::set_permissions(root.join("shared"), fs::Permissions::from_mode(0o2775)).unwrap();
    })
    .await;

    chmod_on_all_routes(&routes, "shared", 0o775).assert_all_ok();

    for dir in [&routes.local, &routes.ssh, &routes.agent] {
        assert_eq!(mode_of(&dir.root.join("shared")), 0o2775, "{:?}", dir.root);
    }
}

// @kotowari[REQ-paths-016, EX-paths-032]
#[tokio::test(flavor = "multi_thread")]
async fn changing_the_permissions_of_a_missing_path_names_the_operation_the_path_and_the_cause() {
    let routes = Routes::new(|_, _| {}).await;

    chmod_on_all_routes(&routes, "missing.txt", 0o644).assert_all_fail_naming(&[
        "permission",
        "missing.txt",
        "no such file",
    ]);
}

// @kotowari[REQ-paths-017, EX-paths-033]
#[tokio::test(flavor = "multi_thread")]
async fn listing_the_same_directory_gives_the_same_entries_on_all_three_routes() {
    let routes = Routes::new(|root, _| {
        fs::write(root.join("a.txt"), "some text\n").unwrap();
        fs::create_dir(root.join("s")).unwrap();
        fs::set_permissions(root.join("s"), fs::Permissions::from_mode(0o2755)).unwrap();
        symlink(" t ", root.join("l")).unwrap();
        fs::create_dir_all(root.join("v/vendor")).unwrap();
        set_mtime(&root.join("a.txt"), "@1700000000.123456789");
        set_mtime(&root.join("s"), "@1700000001.987654321");
        set_mtime(&root.join("l"), "@1700000002.000000123");
        set_mtime(&root.join("v"), "@1700000003.5");
        set_mtime(&root.join("v/vendor"), "@1700000004.25");
    })
    .await;

    let (local, [local_text, ssh, agent]) = list_on_all_routes(&routes, "");

    assert_eq!(ssh, local_text);
    assert_eq!(agent, local_text);
    let find = |name: &str| local.iter().find(|entry| entry.name == name).unwrap();
    assert_eq!(find("a.txt").mtime, Some((1_700_000_000, 123_456_789)));
    assert_eq!(find("s").permissions, Some(0o2755));
    assert_eq!(find("l").kind, "symlink to \" t \"");

    // エージェントの exclude はローカルと同じく、直下の子の root_dir からのパスに当てる
    let exclude = ["vendor/**".to_string()];
    let local = entries(
        remote_merge_engine::local_io::list_children(
            &routes.local.root,
            &exclude,
            &remote_merge_core::relative_path::RelPath::parse("v").unwrap(),
        )
        .map_err(|error| error.to_string()),
    );
    let agent = routes.over_agent(|agent| {
        entries(agent_children(
            &mut agent.client,
            &remote_merge_core::relative_path::RelPath::parse("v").unwrap(),
            &exclude,
        ))
    });
    assert_eq!(agent, local);
    assert_eq!(local.unwrap().len(), 1);
}

// @kotowari[REQ-paths-017, EX-paths-036]
#[tokio::test(flavor = "multi_thread")]
async fn a_path_pattern_exclude_does_not_hide_a_deeper_child_of_the_same_name_on_all_three_routes()
{
    let routes = Routes::new(|root, _| {
        fs::create_dir_all(root.join("vendor")).unwrap();
        fs::write(root.join("vendor/x.txt"), "x\n").unwrap();
        fs::create_dir_all(root.join("a/vendor")).unwrap();
        fs::write(root.join("a/vendor/y.txt"), "y\n").unwrap();
        set_mtime(&root.join("a/vendor"), "@1700000000.5");
    })
    .await;
    let exclude = ["vendor/**".to_string()];
    let routes = routes.with_exclude(&exclude);

    let (local, [local_text, ssh, agent]) = list_on_all_routes_excluding(&routes, "a", &exclude);

    assert_eq!(ssh, local_text);
    assert_eq!(agent, local_text);
    let names: Vec<&str> = local.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["vendor"]);
}

// @kotowari[REQ-paths-017]
#[tokio::test(flavor = "multi_thread")]
async fn an_entry_modified_before_1970_is_listed_with_the_same_time_on_all_three_routes() {
    let routes = Routes::new(|root, _| {
        fs::write(root.join("old.txt"), "old\n").unwrap();
        set_mtime(&root.join("old.txt"), "@-1");
    })
    .await;

    let (local, [local_text, ssh, agent]) = list_on_all_routes(&routes, "");

    assert_eq!(ssh, local_text);
    assert_eq!(agent, local_text);
    assert_eq!(local[0].mtime, Some((-1, 0)));
}

// @kotowari[REQ-paths-017, EX-paths-037]
#[tokio::test(flavor = "multi_thread")]
async fn ex_paths_037_children_named_with_a_tab_or_a_newline_are_listed_by_the_same_names_on_all_three_routes(
) {
    let routes = Routes::new(|root, _| {
        fs::create_dir(root.join("a")).unwrap();
        for name in ["x\ty.txt", "p\nq.txt"] {
            fs::write(root.join("a").join(name), "x\n").unwrap();
            set_mtime(&root.join("a").join(name), "@1700000000.5");
        }
    })
    .await;

    let (local, [local_text, ssh, agent]) = list_on_all_routes(&routes, "a");

    assert_eq!(ssh, local_text);
    assert_eq!(agent, local_text);
    let mut names: Vec<&str> = local.iter().map(|entry| entry.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["p\nq.txt", "x\ty.txt"]);
}

// @kotowari[EX-paths-034]
#[test]
fn a_listing_with_an_unreadable_link_target_stops_with_an_error_naming_the_link() {
    if serve_ssh_request() {
        return;
    }
    let request = ScanRequest {
        scope: ScanScope::Children(String::new()),
        max_entries: MAX_ENTRIES,
    };

    let result = scan(&mut UnreadableLink, &Exclude::default(), &request);

    let Err(ScanError::Failure(failure)) = result else {
        panic!("a link without its target must not be listed: {result:?}");
    };
    assert_eq!(
        failure,
        ScanFailure::UnreadableLinkTarget { path: "l".into() }
    );
    let error = failure.to_string();
    assert!(error.contains("link target"), "{error}");
    assert!(!error.contains("???"), "{error}");
}
