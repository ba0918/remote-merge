#![cfg(unix)]
//! エージェントの経路でも、root_dir の外を指す途中の symlink 越しに読み書きでき
//! （docs/ir/merge/symlink.md の REQ-merge-004）、".." や絶対パスをリンク先に持つ symlink を
//! 作れて（REQ-merge-023）、ローカルの経路と同じ結果になることの契約テスト。
//!
//! 書き込み先のサーバは sudo = true にする。sudo のサーバでは SSH の経路に切り替えないので、
//! エージェントが拒めば書き込みは失敗し、エージェントがその書き込みを行ったことが分かる。

use std::fs;
use std::path::Path;

use super::agent_support::{AgentFixture, Setup, json};
use super::server_faults::Faults;

const SOURCE_TO_LOCAL: [&str; 4] = ["--left", "fixture", "--right", "local"];
const LOCAL_TO_REMOTE: [&str; 4] = ["--left", "local", "--right", "fixture"];

/// merge の結果のうち、経路で一致させるもの
#[derive(Debug, PartialEq)]
struct Outcome {
    exit_code: Option<i32>,
    merged: Vec<String>,
    backed_up: bool,
    failed: usize,
}

fn outcome(output: &std::process::Output) -> Outcome {
    let result = json(output);
    let merged = result["merged"].as_array().cloned().unwrap_or_default();
    Outcome {
        exit_code: output.status.code(),
        merged: merged
            .iter()
            .map(|entry| entry["path"].as_str().unwrap().to_string())
            .collect(),
        backed_up: merged.iter().all(|entry| entry["backup"].is_string()),
        failed: result["failed"].as_array().map_or(0, Vec::len),
    }
}

/// 書き込み先を sudo = true のサーバにした fixture。エージェントの経路でしか書けない
async fn sudo_fixture() -> AgentFixture {
    AgentFixture::new(
        Setup {
            sudo: true,
            ..Setup::default()
        },
        Faults {
            fake_sudo: true,
            ..Faults::default()
        },
    )
    .await
}

fn assert_written_by_the_agent(fixture: &AgentFixture, request: &str) {
    let requests = fixture.server.faults().agent_requests();
    assert!(requests.iter().any(|r| r == request), "{requests:?}");
}

/// `target_root` の "shared" を root_dir の外のディレクトリ `outside` を指す symlink にし、
/// その先に古い "a.txt" を置く
fn shared_link_outside(target_root: &Path, outside: &Path) {
    fs::create_dir_all(outside).unwrap();
    fs::write(outside.join("a.txt"), "old\n").unwrap();
    std::os::unix::fs::symlink(outside, target_root.join("shared")).unwrap();
}

// @kotowari[REQ-merge-004]
#[tokio::test(flavor = "multi_thread")]
async fn the_agent_writes_through_an_intermediate_link_outside_the_root_like_the_local_route() {
    let local_route = {
        let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
        let outside = fixture.home.path().join("outside-local");
        shared_link_outside(&fixture.local(), &outside);
        fixture.put("shared/a.txt", None, Some("new\n"));
        let output = fixture.run_with_sides(&["merge", "shared/a.txt"], &SOURCE_TO_LOCAL);
        assert_eq!(fs::read_to_string(outside.join("a.txt")).unwrap(), "new\n");
        outcome(&output)
    };
    let agent_route = {
        let fixture = sudo_fixture().await;
        let outside = fixture.home.path().join("outside-remote");
        shared_link_outside(&fixture.remote(), &outside);
        fixture.put("shared/a.txt", Some("new\n"), None);
        let output = fixture.run_with_sides(&["merge", "shared/a.txt"], &LOCAL_TO_REMOTE);
        assert_eq!(
            fs::read_to_string(outside.join("a.txt")).unwrap(),
            "new\n",
            "{output:?}"
        );
        assert!(
            fs::symlink_metadata(fixture.remote().join("shared"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_written_by_the_agent(&fixture, "WriteFile");
        outcome(&output)
    };

    assert_eq!(
        local_route,
        Outcome {
            exit_code: Some(0),
            merged: vec!["shared/a.txt".into()],
            backed_up: true,
            failed: 0,
        }
    );
    assert_eq!(agent_route, local_route);
}

/// リンク先が ".." で始まるものと絶対パスのものの、書き込み先にない symlink
const LINKS: [(&str, &str); 2] = [("up", "../elsewhere/x.txt"), ("abs", "/etc/hostname")];

fn put_links(source_root: &Path) {
    for (name, target) in LINKS {
        std::os::unix::fs::symlink(target, source_root.join(name)).unwrap();
    }
}

fn assert_links_created(target_root: &Path, output: &std::process::Output) {
    for (name, target) in LINKS {
        let link = target_root.join(name);
        assert_eq!(
            fs::read_link(&link).ok().as_deref(),
            Some(Path::new(target)),
            "{name}: {output:?}"
        );
    }
}

// @kotowari[REQ-merge-023]
#[tokio::test(flavor = "multi_thread")]
async fn the_agent_creates_links_to_parent_and_absolute_targets_like_the_local_route() {
    let names: Vec<&str> = LINKS.iter().map(|(name, _)| *name).collect();
    let mut args = vec!["merge"];
    args.extend(&names);

    let local_route = {
        let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
        put_links(&fixture.remote());
        let output = fixture.run_with_sides(&args, &SOURCE_TO_LOCAL);
        assert_links_created(&fixture.local(), &output);
        outcome(&output)
    };
    let agent_route = {
        let fixture = sudo_fixture().await;
        put_links(&fixture.local());
        let output = fixture.run_with_sides(&args, &LOCAL_TO_REMOTE);
        assert_links_created(&fixture.remote(), &output);
        assert_written_by_the_agent(&fixture, "Symlink");
        outcome(&output)
    };

    assert_eq!(local_route.exit_code, Some(0), "{local_route:?}");
    assert_eq!(local_route.failed, 0, "{local_route:?}");
    assert_eq!(agent_route, local_route);
}
