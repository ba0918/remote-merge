#![cfg(unix)]
//! merge が書き込み先に作ったディレクトリの権限が、ローカル・SSH・エージェントの三つの経路で
//! 同じになること（docs/ir/paths/operations.md の REQ-paths-018、docs/ir/merge の REQ-merge-012）の
//! 契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::Faults;

const SHARED: &str = "shared";
const FILE: &str = "shared/new/a.txt";

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// 新しいディレクトリの権限（defaults.dir_permissions）を 0o775 にした fixture
async fn fixture_with_dir_permissions_775(setup: Setup) -> AgentFixture {
    let fixture = AgentFixture::new(setup, Faults::default()).await;
    let mut config = fs::read_to_string(&fixture.config).unwrap();
    config.push_str("[defaults]\ndir_permissions = \"0o775\"\n");
    fs::write(&fixture.config, config).unwrap();
    fixture
}

/// `shared` を 0o2775 にした書き込み先 `target_root` へ、ない親ディレクトリの下のファイルを
/// `sides` の向きで merge し、作られた `shared/new` の権限を返す
fn merge_into_setgid_parent(fixture: &AgentFixture, target_root: &Path, sides: &[&str]) -> u32 {
    let shared = target_root.join(SHARED);
    fs::create_dir(&shared).unwrap();
    fs::set_permissions(&shared, fs::Permissions::from_mode(0o2775)).unwrap();
    assert_eq!(mode_of(&shared), 0o2775, "setgid could not be set up");

    let output = fixture.run_with_sides(&["merge", FILE], sides);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json(&output)["merged"].as_array().map(Vec::len), Some(1));
    assert_eq!(fs::read_to_string(target_root.join(FILE)).unwrap(), "new\n");
    mode_of(&target_root.join("shared/new"))
}

// @kotowari[REQ-paths-018, REQ-merge-012, EX-merge-052]
#[tokio::test(flavor = "multi_thread")]
async fn a_directory_merge_creates_under_a_setgid_parent_keeps_setgid_on_all_three_routes() {
    let local_route = {
        let fixture = fixture_with_dir_permissions_775(Setup::default()).await;
        fixture.put(FILE, None, Some("new\n"));
        let target = fixture.local();
        merge_into_setgid_parent(
            &fixture,
            &target,
            &["--left", "fixture", "--right", "local"],
        )
    };
    let ssh_route = {
        let setup = Setup {
            agent_disabled: true,
            ..Setup::default()
        };
        let fixture = fixture_with_dir_permissions_775(setup).await;
        fixture.put(FILE, Some("new\n"), None);
        let target = fixture.remote();
        merge_into_setgid_parent(
            &fixture,
            &target,
            &["--left", "local", "--right", "fixture"],
        )
    };
    let agent_route = {
        let fixture = fixture_with_dir_permissions_775(Setup::default()).await;
        fixture.put(FILE, Some("new\n"), None);
        let target = fixture.remote();
        let mode = merge_into_setgid_parent(
            &fixture,
            &target,
            &["--left", "local", "--right", "fixture"],
        );
        let requests = fixture.server.faults().agent_requests();
        assert!(requests.iter().any(|r| r == "WriteFile"), "{requests:?}");
        mode
    };

    assert_eq!(
        [local_route, ssh_route, agent_route],
        [0o2775, 0o2775, 0o2775],
        "local, ssh, agent"
    );
}
