#![cfg(unix)]
//! エージェントを使わない SSH の経路の書き込みが、書き込み先をその場で書き換えること
//! （docs/ir/merge/concurrency.md の REQ-merge-039）の契約テスト。

use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::process::{Command, Stdio};

use remote_merge_ssh::tree_parser::build_write_text_command;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::Faults;

// @kotowari[REQ-merge-039, EX-merge-046]
#[test]
fn ex_merge_046_a_link_swapped_in_during_an_ssh_write_keeps_its_target_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let dest = root.path().join("a.txt");
    let victim = root.path().join("victim.txt");
    fs::write(&dest, "old\n").unwrap();
    fs::write(&victim, "victim\n").unwrap();
    let mut writer = Command::new("/bin/sh")
        .arg("-c")
        .arg(build_write_text_command(dest.to_str().unwrap()))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = writer.stdin.take().unwrap();
    stdin.write_all(b"first half ").unwrap();
    stdin.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));

    fs::remove_file(&dest).unwrap();
    symlink(&victim, &dest).unwrap();
    stdin.write_all(b"second half\n").unwrap();
    drop(stdin);
    writer.wait().unwrap();

    assert_eq!(fs::read_to_string(&victim).unwrap(), "victim\n");
    assert!(fs::symlink_metadata(&dest)
        .unwrap()
        .file_type()
        .is_symlink());
}

// @kotowari[REQ-merge-039, EX-merge-050]
#[tokio::test(flavor = "multi_thread")]
async fn ex_merge_050_an_ssh_write_keeps_the_mode_and_the_hard_links_of_an_existing_file() {
    let setup = Setup {
        agent_disabled: true,
        ..Setup::default()
    };
    let fixture = AgentFixture::new(setup, Faults::default()).await;
    fixture.put(
        "dir/a.txt",
        Some("new\n"),
        Some("a much longer old content\n"),
    );
    let dir = fixture.remote().join("dir");
    fs::set_permissions(dir.join("a.txt"), fs::Permissions::from_mode(0o640)).unwrap();
    fs::hard_link(dir.join("a.txt"), dir.join("other-name.txt")).unwrap();
    let names_before = names_in(&dir);

    let output = fixture.run(&["merge", "dir/a.txt", "--force"]);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        json(&output)["merged"].as_array().map(Vec::len),
        Some(1),
        "{output:?}"
    );
    let mode = fs::metadata(dir.join("a.txt"))
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(mode, 0o640);
    assert_eq!(
        fs::read_to_string(dir.join("other-name.txt")).unwrap(),
        "new\n"
    );
    assert_eq!(names_in(&dir), names_before);
}

fn names_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}
