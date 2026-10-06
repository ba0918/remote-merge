#![cfg(unix)]
//! ローカル・SSH・エージェントのパスの確認が同じ結果を返すこと（docs/ir/paths/routes.md）の契約テスト。
//!
//! 同じ一時ディレクトリに状態を作り、三つの経路で同じパスを確かめる。
//! - ローカル: 製品のローカルの確認
//! - SSH: 製品が組み立てるスクリプトをこのマシンの sh で動かし、製品の読み取りを通したもの
//! - エージェント: Dispatcher に InspectPath を送り、応答を通信の形式で往復させて製品の変換を通したもの。
//!   作業ディレクトリを選ぶ相対の root_dir では、エージェントのプロセスを起動して尋ねる

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use remote_merge::agent::client::AgentClient;
use remote_merge::agent::dispatch::Dispatcher;
use remote_merge::agent::protocol::{
    deserialize_response, serialize_response, AgentRequest, AgentResponse,
};
use remote_merge::agent::server::MetadataConfig;
use remote_merge::runtime::path_route::resolved_from_agent;
use remote_merge::runtime::remote_path::{build_inspect_path_command, parse_inspect_path_output};
use remote_merge_core::path_resolution::ResolvedPath;

/// 経路の答え。エラーは文言で比べる
type Outcome = Result<ResolvedPath, String>;

fn local(root: &Path, rel: &str) -> Outcome {
    remote_merge_engine::local_io::inspect_local_path(root, rel).map_err(|error| error.to_string())
}

/// SSH のスクリプトを、PATH を `path_env` にした sh で動かす
fn ssh_with_path(root: &Path, rel: &str, path_env: &OsStr) -> Outcome {
    run_script(root.to_str().unwrap(), rel, path_env, None)
}

/// SSH のスクリプトを sh で動かす。`cwd` があれば、その作業ディレクトリで動かす
fn run_script(root: &str, rel: &str, path_env: &OsStr, cwd: Option<&Path>) -> Outcome {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(build_inspect_path_command(root, rel))
        .env("PATH", path_env);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{rel}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    parse_inspect_path_output(&output.stdout).map_err(|error| error.to_string())
}

fn ssh(root: &Path, rel: &str) -> Outcome {
    ssh_with_path(root, rel, &std::env::var_os("PATH").unwrap_or_default())
}

fn agent(root: &Path, rel: &str) -> Outcome {
    let mut dispatcher = Dispatcher::new(root.to_path_buf(), MetadataConfig::default());
    let responses = dispatcher
        .dispatch(AgentRequest::InspectPath { path: rel.into() })
        .unwrap();
    let [response] = responses.as_slice() else {
        panic!("{responses:?}");
    };
    let wire = serialize_response(response).unwrap();
    match deserialize_response(&wire).unwrap() {
        AgentResponse::PathInspection { result } => {
            resolved_from_agent(result).map_err(|error| error.to_string())
        }
        other => panic!("{other:?}"),
    }
}

/// 作業ディレクトリ `cwd` で、root_dir を `root` として起動したエージェントのプロセスに尋ねる
fn agent_process(cwd: &Path, root: &str, rel: &str) -> Outcome {
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
    let mut client =
        AgentClient::connect(child.stdout.take().unwrap(), child.stdin.take().unwrap()).unwrap();
    let inspection = client.inspect_path(rel).unwrap();
    drop(client);
    let _ = child.kill();
    let _ = child.wait();
    resolved_from_agent(inspection).map_err(|error| error.to_string())
}

/// 三つの経路の答え。食い違えば説明を返す
fn disagreement(root: &Path, rel: &str) -> Option<String> {
    let (local, ssh, agent) = (local(root, rel), ssh(root, rel), agent(root, rel));
    (local != ssh || local != agent)
        .then(|| format!("{rel:?}: local {local:?}, ssh {ssh:?}, agent {agent:?}"))
}

/// 相対の root_dir `root` を作業ディレクトリ `cwd` で確かめた三つの経路の答え。食い違えば説明を返す。
/// ローカルの root_dir は設定を読む段階で絶対パスになる（REQ-config-040）ので、ローカルには
/// `cwd` につないだ絶対パスを渡す
fn relative_root_disagreement(cwd: &Path, root: &str, rel: &str) -> Option<String> {
    let local = local(&cwd.join(root), rel);
    let path_env = std::env::var_os("PATH").unwrap_or_default();
    let ssh = run_script(root, rel, &path_env, Some(cwd));
    let agent = agent_process(cwd, root, rel);
    (local != ssh || local != agent)
        .then(|| format!("{cwd:?} {rel:?}: local {local:?}, ssh {ssh:?}, agent {agent:?}"))
}

struct Base {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

impl Base {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(directory.path()).unwrap();
        Self {
            _directory: directory,
            path,
        }
    }

    /// 実パスがちょうど `total` バイトになるディレクトリを作り、その実パスを返す
    fn dir_of_length(&self, name: &str, total: usize) -> String {
        let mut remaining = total - self.path.as_os_str().len() - 1 - name.len() - 1;
        let mut names = vec![name.to_string()];
        while remaining > 201 {
            names.push("d".repeat(200));
            remaining -= 201;
        }
        names.push("d".repeat(remaining));
        let rel = names.join("/");
        fs::create_dir_all(self.path.join(&rel)).unwrap();
        let real = format!("{}/{rel}", self.path.display());
        assert_eq!(real.len(), total);
        real
    }
}

/// 例の状態を一つの root の下にまとめて作り、確かめるパスを返す
fn example_states(base: &Base) -> Vec<(PathBuf, String)> {
    let root = base.path.join("root");
    let at = |rel: &str| root.join(rel);
    for dir in ["dir", "a", "b", "locked"] {
        fs::create_dir_all(at(dir)).unwrap();
    }
    for file in ["dir/file.txt", "file.txt", "regular", "locked/a.txt"] {
        fs::write(at(file), "content\n").unwrap();
    }
    let links: Vec<(String, String)> = vec![
        // EX-paths-001・EX-paths-002
        ("link1".into(), "dir/./file.txt".into()),
        ("link2".into(), "dir//file.txt".into()),
        // EX-paths-004
        ("slash_file".into(), "file.txt/".into()),
        // EX-paths-007
        ("a/l".into(), "../b/missing".into()),
        // EX-paths-008: 長い実ディレクトリの先に存在しない名前をつなぐ
        ("deep".into(), base.dir_of_length("deep4060", 4060)),
        ("too_long".into(), format!("deep/{}", "n".repeat(40))),
        // EX-paths-009: "/" で終わり、それを除くと 4095 バイトになるリンク先
        (
            "long_slash".into(),
            format!(
                "../{}/",
                base.dir_of_length("deep4095", 4095)
                    .strip_prefix(&format!("{}/", base.path.display()))
                    .unwrap()
            ),
        ),
        // EX-paths-010
        ("dot_run".into(), format!("{}file.txt", "./".repeat(2040))),
        // EX-paths-013
        ("inner".into(), "file.txt/".into()),
        ("chain_slash".into(), "inner".into()),
        // EX-paths-014
        ("new_slash".into(), "newdir/".into()),
        // EX-paths-016
        ("gone_dir".into(), "gone".into()),
        // EX-paths-017
        ("missing_dotdot".into(), "missing/../file.txt".into()),
        // EX-paths-018
        ("loop".into(), "loop".into()),
        // 途中の祖先の symlink がディレクトリを指す
        ("linkdir".into(), "dir".into()),
    ];
    for (name, target) in &links {
        symlink(target, at(name)).unwrap();
    }
    // EX-paths-011・EX-paths-012: 40 個と 41 個の連なったリンク
    for (prefix, count) in [("c", 40), ("e", 41)] {
        for index in 1..=count {
            let next = if index == count {
                "file.txt".to_string()
            } else {
                format!("{prefix}{}", index + 1)
            };
            symlink(next, at(&format!("{prefix}{index}"))).unwrap();
        }
    }
    // EX-paths-023
    symlink(OsStr::from_bytes(b"name\xff"), at("not_utf8")).unwrap();
    // EX-paths-002: root_dir 自身が symlink。EX-paths-005: root_dir 自身が通常ファイル
    symlink(&root, base.path.join("root_link")).unwrap();
    fs::write(base.path.join("root_file"), "content\n").unwrap();

    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(at("locked"), fs::Permissions::from_mode(0o600)).unwrap();

    let mut cases: Vec<(PathBuf, String)> = [
        "link1",
        "link2",
        "regular/a.txt",
        "slash_file/a.txt",
        "a.txt",
        "missing/a.txt",
        "a/l",
        "too_long",
        "long_slash",
        "dot_run",
        "c1",
        "e1",
        "chain_slash",
        "new_slash",
        "locked/a.txt",
        "gone_dir/a.txt",
        "missing_dotdot",
        "loop",
        "not_utf8",
        "linkdir/file.txt",
        "linkdir/new/a.txt",
        "dir",
        "",
    ]
    .into_iter()
    .map(|rel| (root.clone(), rel.to_string()))
    .collect();
    cases.push((base.path.join("root_link"), String::new()));
    cases.push((base.path.join("root_file"), "a.txt".into()));
    cases
}

// @kotowari[REQ-paths-008, EX-paths-019]
#[test]
fn ex_paths_019_every_example_state_gives_the_same_answer_on_all_three_routes() {
    let base = Base::new();
    let cases = example_states(&base);

    let disagreements: Vec<String> = cases
        .iter()
        .filter_map(|(root, rel)| disagreement(root, rel))
        .collect();

    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        base.path.join("root/locked"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    assert!(disagreements.is_empty(), "{disagreements:#?}");
    // findings 15〜17 の形（EX-paths-008・009・013・018）が比べた中にあり、例のとおりの答えになる
    let root = base.path.join("root");
    assert_eq!(
        local(&root, "too_long"),
        Err("cannot resolve path: path too long".into())
    );
    assert!(matches!(
        local(&root, "long_slash"),
        Ok(ResolvedPath::Symlink { .. })
    ));
    assert_eq!(
        local(&root, "chain_slash"),
        Err("cannot resolve path: not a directory".into())
    );
    assert_eq!(
        local(&root, "loop"),
        Err("cannot resolve path: too many levels of symbolic links".into())
    );
}

// @kotowari[REQ-paths-008]
#[test]
fn relative_root_dirs_give_the_same_answer_on_all_three_routes() {
    let base = Base::new();
    let mut disagreements = Vec::new();
    // 名前が改行で終わる作業ディレクトリと、名前が UTF-8 でない作業ディレクトリ
    for cwd_name in [OsStr::new("work\n"), OsStr::from_bytes(b"work\xff")] {
        let cwd = base.path.join(cwd_name);
        fs::create_dir_all(cwd.join("app")).unwrap();
        fs::write(cwd.join("app/a.txt"), "content\n").unwrap();
        symlink("loop", cwd.join("app/loop")).unwrap();
        for rel in ["a.txt", "loop"] {
            disagreements.extend(relative_root_disagreement(&cwd, "app", rel));
        }
    }

    assert!(disagreements.is_empty(), "{disagreements:#?}");
}

// @kotowari[REQ-paths-012, EX-paths-025]
#[test]
fn ex_paths_025_a_relative_root_dir_is_resolved_from_the_remote_working_directory() {
    let base = Base::new();
    let home = base.path.join("home/u");
    fs::create_dir_all(home.join("app")).unwrap();
    fs::write(home.join("app/a.txt"), "content\n").unwrap();
    let expected = Ok(ResolvedPath::Exists {
        real_path: format!("{}/app/a.txt", home.display()),
    });
    let path_env = std::env::var_os("PATH").unwrap_or_default();

    assert_eq!(run_script("app", "a.txt", &path_env, Some(&home)), expected);
    assert_eq!(agent_process(&home, "app", "a.txt"), expected);
}

// @kotowari[REQ-paths-009, EX-paths-020]
#[test]
fn ex_paths_020_without_readlink_only_paths_that_meet_a_symlink_stop() {
    let base = Base::new();
    fs::write(base.path.join("a.txt"), "content\n").unwrap();
    symlink("a.txt", base.path.join("l")).unwrap();
    let empty = tempfile::tempdir().unwrap();

    assert_eq!(
        ssh_with_path(&base.path, "a.txt", empty.path().as_os_str()),
        Ok(ResolvedPath::Exists {
            real_path: format!("{}/a.txt", base.path.display())
        })
    );
    assert_eq!(
        ssh_with_path(&base.path, "l", empty.path().as_os_str()),
        Err("cannot resolve path: readlink not available".into())
    );
}

/// リンク先を要素・"."・"//"・".."・連なったリンク・長いディレクトリへのリンクと、
/// 4096 バイト前後まで伸ばす並びから作る
fn generated_link_target() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::prelude::*;
    let element = prop::sample::select(vec![
        "file.txt", "dir", "missing", ".", "..", "", "l0", "l1", "l2", "deep", "near",
    ]);
    let elements = prop::collection::vec(element, 0..5);
    let run = prop::sample::select(vec![
        "./".to_string(),
        ".//".to_string(),
        "dir/../".to_string(),
        "missing/".to_string(),
        "x".repeat(200) + "/",
    ]);
    (elements, prop::option::of((run, 0usize..25)), any::<bool>()).prop_map(
        |(elements, run, trailing_slash)| {
            let mut target = elements.join("/");
            if let Some((run, count)) = run {
                target = format!("{}{target}", run.repeat(count));
            }
            if trailing_slash {
                target.push('/');
            }
            target.truncate(4000);
            if target.is_empty() {
                target.push('.');
            }
            target
        },
    )
}

// @kotowari[REQ-paths-008, EX-paths-019]
#[test]
fn generated_link_states_give_the_same_answer_on_all_three_routes() {
    use proptest::test_runner::{Config, FileFailurePersistence};

    let base = Base::new();
    let root = base.path.join("root");
    fs::create_dir_all(root.join("dir")).unwrap();
    fs::write(root.join("file.txt"), "content\n").unwrap();
    symlink(base.dir_of_length("long", 4070), root.join("deep")).unwrap();
    symlink(base.dir_of_length("short", 4000), root.join("near")).unwrap();
    let config = Config {
        cases: 64,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../proptest-regressions/tests/contract/path_routes_agree.txt"
        )))),
        ..Config::default()
    };
    let queries = ["l0", "l0/a.txt", "l1", "l2/file.txt", "l1/x/y"];
    proptest::proptest!(config, |(targets in proptest::collection::vec(generated_link_target(), 3), query in proptest::sample::select(queries.to_vec()))| {
        for (index, target) in targets.iter().enumerate() {
            let link = root.join(format!("l{index}"));
            let _ = fs::remove_file(&link);
            symlink(target, &link).unwrap();
        }
        let disagreement = disagreement(&root, query);
        proptest::prop_assert!(disagreement.is_none(), "{targets:?}: {disagreement:?}");
    });
}
