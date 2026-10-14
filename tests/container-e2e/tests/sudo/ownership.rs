//! sudo のエージェントが読むもの・作るものの権限と所有者

use std::fs;
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use super::support::*;

// root だけが読める権限 0 のファイルは、sudo で動くエージェントが走査して読める
// @kotowari[REQ-merge-014, EX-merge-056]
#[test]
fn ex_merge_056_a_source_mode_of_0_read_under_sudo_is_copied_to_the_destination() {
    let container = std::env::var("REMOTE_MERGE_E2E_CONTAINER").unwrap();
    let port = std::env::var("REMOTE_MERGE_E2E_PORT").unwrap();
    let home = TempDir::new().unwrap();
    let local = home.path().join("local/zero-mode/a.txt");
    fs::create_dir_all(local.parent().unwrap()).unwrap();
    fs::write(&local, "destination\n").unwrap();
    fs::set_permissions(&local, fs::Permissions::from_mode(0o644)).unwrap();
    let setup = docker(&container, "mkdir -p /tmp/agent/remote-merge-testuser && rm -rf /data/zero-mode && mkdir -p /data/zero-mode && printf 'source\\n' > /data/zero-mode/a.txt && chown root:root /data/zero-mode/a.txt && chmod 000 /data/zero-mode/a.txt");
    assert!(setup.status.success(), "{setup:?}");

    let output = run_with_sides(
        &home,
        &port,
        true,
        "/tmp/agent",
        &["merge", "zero-mode/a.txt", "--with-permissions"],
        &["--left", "fixture", "--right", "local"],
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::metadata(&local).unwrap().permissions().mode() & 0o7777,
        0
    );
    fs::set_permissions(&local, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(fs::read_to_string(&local).unwrap(), "source\n");
}

// @kotowari[REQ-merge-042, EX-merge-064]
#[test]
fn ex_merge_064_a_new_file_and_its_new_directory_belong_to_the_login_user_under_sudo() {
    let container = std::env::var("REMOTE_MERGE_E2E_CONTAINER").unwrap();
    let port = std::env::var("REMOTE_MERGE_E2E_PORT").unwrap();
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join("local/owned-new")).unwrap();
    fs::write(home.path().join("local/owned-new/a.txt"), "new\n").unwrap();
    let setup = docker(
        &container,
        "mkdir -p /data /tmp/agent/remote-merge-testuser && chown root:root /data && rm -rf /data/owned-new",
    );
    assert!(setup.status.success(), "{setup:?}");

    let output = run_with(
        &home,
        &port,
        true,
        "/tmp/agent",
        &["merge", "owned-new/a.txt"],
    );

    assert!(output.status.success(), "{output:?}");
    let owners = docker(
        &container,
        "stat -c '%U' /data/owned-new /data/owned-new/a.txt",
    );
    assert_eq!(
        String::from_utf8(owners.stdout).unwrap(),
        "testuser\ntestuser\n"
    );
}

// 4MB を超えて分けて送る新しいファイルも、sudo で動くエージェントは最初の分で O_EXCL で作って開いた
// ファイルに最後の分まで書き、ログインした利用者の持ち物にする
// @kotowari[REQ-merge-042]
#[test]
fn a_new_file_sent_in_several_parts_under_sudo_is_written_whole_and_belongs_to_the_login_user() {
    let container = std::env::var("REMOTE_MERGE_E2E_CONTAINER").unwrap();
    let port = std::env::var("REMOTE_MERGE_E2E_PORT").unwrap();
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join("local/owned-big")).unwrap();
    let content: Vec<u8> = (0..9 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
    fs::write(home.path().join("local/owned-big/a.bin"), &content).unwrap();
    let setup = docker(
        &container,
        "mkdir -p /data /tmp/agent/remote-merge-testuser && chown root:root /data && rm -rf /data/owned-big",
    );
    assert!(setup.status.success(), "{setup:?}");

    let output = run_with(
        &home,
        &port,
        true,
        "/tmp/agent",
        &["merge", "owned-big/a.bin"],
    );

    assert!(output.status.success(), "{output:?}");
    let owner = docker(&container, "stat -c '%U %s' /data/owned-big/a.bin");
    assert_eq!(
        String::from_utf8(owner.stdout).unwrap(),
        format!("testuser {}\n", content.len())
    );
    let copied = docker(&container, "cat /data/owned-big/a.bin");
    assert!(copied.stdout == content, "the copied content differs");
}
