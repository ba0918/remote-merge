#![cfg(unix)]
//! root_dir 全体を指す値（"/"・"."・""・"//"・"./"）は、ローカルとエージェントのどの経路でも
//! 一つの項目への操作に届かず、root_dir の権限と中身を変えないことの契約テスト。
//!
//! 製品の項目の操作（runtime の比較先の書き込み・削除・権限の変更・symlink の作成・読み込み・
//! ハッシュ）は root_dir の下の項目の型（`EntryPath`）だけを受け取るので、文字列から型にする
//! ところで止まり、どの経路にも要求が届かない。エージェントの受け取り口は、通信で受け取った要求を
//! 拒むことを、起動したエージェントのプロセスで確かめる。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use remote_merge_core::relative_path::{EntryPath, RelPathError};

const ROOT_FORMS: [&str; 5] = ["/", ".", "", "//", "./"];

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

// どの経路の項目の操作も、root_dir 全体を指す値を項目の型にするところで
// "root_dir itself is not an entry: パス" の理由で止まる
// @kotowari[REQ-paths-019, EX-paths-040]
#[test]
fn an_entry_operation_never_receives_the_root_dir_itself() {
    for root in ROOT_FORMS {
        let refused = EntryPath::parse(root).unwrap_err();
        assert_eq!(refused, RelPathError::RootDirItself(root.to_string()));
        assert_eq!(
            refused.to_string(),
            format!("root_dir itself is not an entry: {root}")
        );
    }
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
