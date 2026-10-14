#![cfg(unix)]
//! エージェントの配置先の扱い（docs/ir/ssh/ の REQ-ssh-014〜018）の契約テスト

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::ssh_agent_deploy_support::DeployFixture;

// @kotowari[REQ-ssh-014, EX-ssh-023]
#[tokio::test(flavor = "multi_thread")]
async fn nothing_in_a_symlinked_deploy_directory_is_run() {
    let fixture = DeployFixture::new().await;
    fixture.plant_binary(0o700, None);
    let target = fixture.deploy_target_dir();
    let real = target.with_file_name("planted-dir");
    fs::rename(&target, &real).unwrap();
    std::os::unix::fs::symlink(&real, &target).unwrap();

    let merge = fixture.merge(None);

    fixture.assert_stopped_naming(&merge, &target);
}

// @kotowari[REQ-ssh-005]
#[tokio::test(flavor = "multi_thread")]
async fn an_unsafe_deploy_directory_stops_naming_its_path() {
    let fixture = DeployFixture::new().await;
    fixture.plant_binary(0o700, None);
    let target = fixture.deploy_target_dir();
    fs::rename(&target, target.with_file_name("planted-dir")).unwrap();
    std::os::unix::fs::symlink("planted-dir", &target).unwrap();

    let merge = fixture.merge(None);

    fixture.assert_stopped_naming(&merge, &target);
}

// @kotowari[REQ-ssh-017, EX-ssh-027]
#[tokio::test(flavor = "multi_thread")]
async fn nothing_in_a_deploy_directory_under_a_parent_writable_by_everyone_is_run() {
    let fixture = DeployFixture::new().await;
    fixture.plant_binary(0o700, None);
    let parent = fixture.deploy_target_dir().parent().unwrap().to_path_buf();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o777)).unwrap();

    let merge = fixture.merge(None);

    fixture.assert_stopped_naming(&merge, &fixture.deploy_target_dir());
}

// @kotowari[REQ-ssh-017, EX-ssh-032]
#[tokio::test(flavor = "multi_thread")]
async fn nothing_in_a_deploy_directory_reached_through_a_link_placed_where_everyone_can_write_is_run()
 {
    let fixture = DeployFixture::new().await;
    let home = fixture.home.path();
    // agent -> between/link2 -> ../good と辿り、途中のリンクだけが誰でも書き込める場所にある
    let between = home.join("between");
    fs::create_dir(&between).unwrap();
    fs::set_permissions(&between, fs::Permissions::from_mode(0o777)).unwrap();
    fs::create_dir(home.join("good")).unwrap();
    fs::set_permissions(home.join("good"), fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink("../good", between.join("link2")).unwrap();
    std::os::unix::fs::symlink("between/link2", home.join("agent")).unwrap();
    fixture.plant_binary(0o700, None);

    let merge = fixture.merge(None);

    fixture.assert_stopped_naming(&merge, &fixture.deploy_target_dir());
}

// @kotowari[REQ-ssh-018, EX-ssh-028]
#[tokio::test(flavor = "multi_thread")]
async fn an_own_deploy_directory_writable_by_the_group_is_repaired_with_a_warning() {
    let fixture = DeployFixture::new().await;
    fixture.plant_binary(0o775, None);
    let binary = fixture.deploy_target_dir().join("remote-merge");
    let planted = fs::read(&binary).unwrap();

    let merge = fixture.merge_placing_this_binary();

    assert!(merge.status.success(), "{merge:?}");
    assert_eq!(fs::read(fixture.target()).unwrap(), b"local updated\n");
    assert!(
        !fixture.marker().exists(),
        "a binary in a writable deploy directory was run"
    );
    let mode = fs::metadata(fixture.deploy_target_dir())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    assert_ne!(
        fs::read(&binary).ok(),
        Some(planted),
        "the planted binary was kept"
    );
    let stderr = String::from_utf8_lossy(&merge.stderr);
    assert!(
        stderr.contains(fixture.deploy_target_dir().to_str().unwrap()),
        "{stderr}"
    );
}

// 配置先を直したことの警告は、記録の細かさの指定によらず標準エラーに出す（F51 と同じ）
// @kotowari[REQ-ssh-018, EX-ssh-028]
#[tokio::test(flavor = "multi_thread")]
async fn the_repaired_deploy_directory_is_warned_about_at_any_log_level() {
    let fixture = DeployFixture::new().await;
    fixture.plant_binary(0o775, None);

    let merge = fixture.merge_with(
        Some(Path::new(env!("CARGO_BIN_EXE_remote-merge"))),
        &["--log-level", "error"],
    );

    assert!(merge.status.success(), "{merge:?}");
    let stderr = String::from_utf8_lossy(&merge.stderr);
    assert!(
        stderr.contains(fixture.deploy_target_dir().to_str().unwrap()),
        "{stderr}"
    );
}

// @kotowari[REQ-ssh-015, EX-ssh-024]
#[tokio::test(flavor = "multi_thread")]
async fn a_missing_deploy_directory_is_created_owner_only() {
    let fixture = DeployFixture::new().await;

    let merge = fixture.merge_placing_this_binary();

    assert!(merge.status.success(), "{merge:?}");
    let mode = fs::metadata(fixture.deploy_target_dir())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

// @kotowari[REQ-ssh-016, EX-ssh-025]
#[tokio::test(flavor = "multi_thread")]
async fn a_planted_binary_that_only_matches_the_version_is_not_run_as_the_agent() {
    let fixture = DeployFixture::new().await;
    fixture.plant_binary(0o700, Some("agent"));

    let merge = fixture.merge_placing_this_binary();

    assert!(merge.status.success(), "{merge:?}");
    assert_eq!(fs::read(fixture.target()).unwrap(), b"local updated\n");
    assert!(
        !fixture.marker().exists(),
        "a binary differing from the local agent was run as the agent"
    );
}
