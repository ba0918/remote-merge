#![cfg(unix)]
//! merge が書き込み先に作ったディレクトリの権限が、ローカルとエージェントの二つの経路で
//! 同じになること（docs/ir/paths/operations.md の REQ-paths-018、docs/ir/merge の REQ-merge-012）の
//! 契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::agent_support::{AgentFixture, Setup};
use super::common::stdout_json;
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
    assert_eq!(
        stdout_json(&output)["merged"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(fs::read_to_string(target_root.join(FILE)).unwrap(), "new\n");
    mode_of(&target_root.join("shared/new"))
}

// @kotowari[REQ-paths-018, REQ-merge-012, EX-merge-052]
#[tokio::test(flavor = "multi_thread")]
async fn a_directory_merge_creates_under_a_setgid_parent_keeps_setgid_on_both_routes() {
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

    assert_eq!([local_route, agent_route], [0o2775, 0o2775], "local, agent");
}

const LINK: &str = "linked/new/link";

/// 新しいディレクトリの権限を `mode` にした fixture
async fn fixture_with_dir_permissions(setup: Setup, mode: &str) -> AgentFixture {
    let fixture = AgentFixture::new(setup, Faults::default()).await;
    let mut config = fs::read_to_string(&fixture.config).unwrap();
    config.push_str(&format!("[defaults]\ndir_permissions = \"{mode}\"\n"));
    fs::write(&fixture.config, config).unwrap();
    fixture
}

/// 読み込み元 `source_root` に "linked/new/link"（"x.txt" への symlink）を置き、それを `sides` の
/// 向きで書き込み先 `target_root` へ merge して、作られた "linked" と "linked/new" の権限を返す
fn merge_symlink_into_new_directories(
    fixture: &AgentFixture,
    source_root: &Path,
    target_root: &Path,
    sides: &[&str],
) -> [u32; 2] {
    let source = source_root.join(LINK);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(source.parent().unwrap().join("x.txt"), "x\n").unwrap();
    std::os::unix::fs::symlink("x.txt", &source).unwrap();

    let output = fixture.run_with_sides(&["merge", LINK], sides);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        stdout_json(&output)["merged"].as_array().map(Vec::len),
        Some(1)
    );
    let created = target_root.join(LINK);
    assert_eq!(fs::read_link(&created).unwrap(), Path::new("x.txt"));
    [
        mode_of(&target_root.join("linked")),
        mode_of(&target_root.join("linked/new")),
    ]
}

// umask の既定（0o755 か 0o775）と違う 0o750 にして、設定の権限が付いたことを見分ける
// @kotowari[REQ-merge-012]
#[tokio::test(flavor = "multi_thread")]
async fn a_symlink_merged_into_new_directories_gets_the_directory_permissions_on_both_routes() {
    let local_route = {
        let fixture = fixture_with_dir_permissions(Setup::default(), "0o750").await;
        merge_symlink_into_new_directories(
            &fixture,
            &fixture.remote(),
            &fixture.local(),
            &["--left", "fixture", "--right", "local"],
        )
    };
    let agent_route = {
        let fixture = fixture_with_dir_permissions(Setup::default(), "0o750").await;
        let modes = merge_symlink_into_new_directories(
            &fixture,
            &fixture.local(),
            &fixture.remote(),
            &["--left", "local", "--right", "fixture"],
        );
        let requests = fixture.server.faults().agent_requests();
        assert!(requests.iter().any(|r| r == "Symlink"), "{requests:?}");
        modes
    };

    assert_eq!([local_route, agent_route], [[0o750; 2]; 2], "local, agent");
}
