#![cfg(unix)]
//! root_dir 全体を指す値（"/"・"."・""・"//"・"./"）は、ローカル・SSH・エージェントのどの経路でも
//! 一つの項目への操作に届かず、root_dir の権限と中身を変えないことの契約テスト（ローカルの経路）。
//!
//! 項目の操作は root_dir の下の項目の型だけを受け取るので、文字列から型にするところで止まる。
//! SSH とエージェントの経路は、手元の known_hosts に触れないよう、製品の runtime の単体テスト
//! （side_io）で、経路に要求が届かないことを確かめる。エージェントの受け取り口は dispatch の
//! 単体テストが確かめる。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use remote_merge::side::Side;
use remote_merge_core::relative_path::RelPathError;

use super::agent_support::{AgentFixture, Setup};
use super::server_faults::Faults;

const ROOT_FORMS: [&str; 5] = ["/", ".", "", "//", "./"];
const REFUSAL: &str = "root_dir itself is not an entry";

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// 一つの項目への操作のそれぞれに root_dir 全体を渡したときの失敗の文言
fn refusals_for_each_operation(core: &mut CoreRuntime, side: &Side, root: &str) -> Vec<String> {
    let before = remote_merge::runtime::change_check::Before::Unknown;
    vec![
        core.chmod_file(side, root, 0o700),
        core.write_file(side, root, "x"),
        core.write_file_bytes(side, root, b"x", &before),
        core.remove_file(side, root),
        core.create_symlink(side, root, "x"),
        core.read_file_bytes(side, root, true).map(|_| ()),
        core.current_link_target(side, root).map(|_| ()),
    ]
    .into_iter()
    .map(|result| {
        format!(
            "{:#}",
            result.expect_err("an operation on root_dir itself must fail")
        )
    })
    .collect()
}

/// `side` を `target_root` の経路として、root_dir 全体を指す値をどの項目の操作にも渡せず、
/// root_dir の権限と中身が変わらないことを確かめる。CoreRuntime は自分の非同期の実行環境を持つので、
/// テストの実行環境の外のスレッドで動かす
async fn assert_root_dir_untouched(fixture: &AgentFixture, side: Side, target_root: PathBuf) {
    fs::set_permissions(&target_root, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(target_root.join("kept.txt"), "kept\n").unwrap();
    let config = load_config_from_paths(Some(&fixture.config), None).unwrap();
    let checked_side = side.clone();
    tokio::task::spawn_blocking(move || {
        let mut core = CoreRuntime::with_targets(config, RuntimeTargets::production());
        core.connect_if_remote(&checked_side).unwrap();
        for root in ROOT_FORMS {
            for message in refusals_for_each_operation(&mut core, &checked_side, root) {
                assert!(
                    message.contains(REFUSAL),
                    "{checked_side:?} {root:?}: {message}"
                );
            }
        }
    })
    .await
    .unwrap();

    assert_eq!(mode_of(&target_root), 0o755, "{side:?}");
    assert_eq!(
        fs::read_to_string(target_root.join("kept.txt")).unwrap(),
        "kept\n"
    );
}

// @kotowari[REQ-paths-019, EX-paths-040]
#[tokio::test(flavor = "multi_thread")]
async fn the_local_route_never_operates_on_the_root_dir_itself() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    assert_root_dir_untouched(&fixture, Side::Local, fixture.local()).await;
}

/// root_dir を `root` として起動したエージェントのプロセスに、一つの項目への要求を `path` で送った
/// ときに拒まれた理由
fn agent_refusals(root: &Path, path: &str) -> Vec<Option<RelPathError>> {
    use remote_merge::agent::client::AgentClient;
    use remote_merge::agent::protocol::AgentRequest;
    use std::process::{Command, Stdio};

    let mut child = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .arg("agent")
        .arg("--root")
        .arg(root)
        .env("RUST_LOG", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut client =
        AgentClient::connect(child.stdout.take().unwrap(), child.stdin.take().unwrap()).unwrap();
    let requests = [
        AgentRequest::Remove { path: path.into() },
        AgentRequest::Chmod {
            path: path.into(),
            mode: 0o700,
        },
        AgentRequest::WriteFile {
            path: path.into(),
            content: b"x".to_vec(),
            is_binary: false,
            more_to_follow: false,
        },
        AgentRequest::Symlink {
            path: path.into(),
            target: "x".into(),
        },
        AgentRequest::ReadEntryLink { path: path.into() },
    ];
    let refusals = requests
        .iter()
        .map(|request| {
            client
                .request(request)
                .err()
                .and_then(|error| error.downcast_ref::<RelPathError>().cloned())
        })
        .collect();
    drop(client);
    let _ = child.kill();
    let _ = child.wait();
    refusals
}

// @kotowari[REQ-paths-019, EX-paths-041]
#[test]
fn the_agent_refuses_requests_for_the_root_dir_itself_without_changing_it() {
    let root = tempfile::TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.path().join("kept.txt"), "kept\n").unwrap();

    for path in ROOT_FORMS {
        let expected = Some(RelPathError::RootDirItself(path.to_string()));
        for refusal in agent_refusals(root.path(), path) {
            assert_eq!(refusal, expected, "{path:?}");
        }
    }

    assert_eq!(mode_of(root.path()), 0o755);
    assert_eq!(
        fs::read_to_string(root.path().join("kept.txt")).unwrap(),
        "kept\n"
    );
}
