#![cfg(unix)]
//! --with-permissions の merge が、読み込み元の権限として走査したツリーの値を写すこと
//! （docs/ir/merge/permissions.md の REQ-merge-036）の契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::Faults;

const FILE: &str = "a.txt";

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// リモートの読み込み元の 0o600 の "a.txt" を、新しいファイルの権限を 0o664 にしたローカルへ
/// --with-permissions で merge し、作られたファイルの権限を返す
async fn copy_new_file_from_remote(setup: Setup) -> u32 {
    let fixture = AgentFixture::new(setup, Faults::default()).await;
    let mut config = fs::read_to_string(&fixture.config).unwrap();
    config.push_str("[defaults]\nfile_permissions = \"0o664\"\n");
    fs::write(&fixture.config, config).unwrap();
    fixture.put(FILE, None, Some("remote\n"));
    fs::set_permissions(
        fixture.remote().join(FILE),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();

    let output = fixture.run_with_sides(
        &["merge", FILE, "--with-permissions"],
        &["--left", "fixture", "--right", "local"],
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(json(&output)["merged"].as_array().map(Vec::len), Some(1));
    let created = fixture.local().join(FILE);
    assert_eq!(fs::read_to_string(&created).unwrap(), "remote\n");
    // 読み込み元をどちらの経路で走査したか
    let requests = fixture.server.faults().agent_requests();
    assert_eq!(
        requests.iter().any(|request| request == "ListTree"),
        !setup.agent_disabled,
        "{requests:?}"
    );
    mode_of(&created)
}

// @kotowari[REQ-merge-036, EX-merge-043]
#[tokio::test(flavor = "multi_thread")]
async fn ex_merge_043_a_new_file_from_a_remote_source_gets_the_source_mode_on_both_remote_routes() {
    let ssh_route = copy_new_file_from_remote(Setup {
        agent_disabled: true,
        ..Setup::default()
    })
    .await;
    let agent_route = copy_new_file_from_remote(Setup::default()).await;

    assert_eq!([ssh_route, agent_route], [0o600, 0o600], "ssh, agent");
}
