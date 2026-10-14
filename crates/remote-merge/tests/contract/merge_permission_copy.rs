#![cfg(unix)]
//! --with-permissions の merge が、読み込み元の権限として走査したツリーの値を写すこと
//! （docs/ir/merge/permissions.md の REQ-merge-036）の契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::agent_support::{AgentFixture, Setup, json};
use super::server_faults::Faults;

const FILE: &str = "a.txt";

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// リモートの読み込み元の 0o600 の "a.txt" を、新しいファイルの権限を 0o664 にしたローカルへ
/// --with-permissions で merge し、作られたファイルの権限を返す
async fn copy_new_file_from_remote() -> u32 {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
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
    assert!(
        requests.iter().any(|request| request == "ListTree"),
        "{requests:?}"
    );
    mode_of(&created)
}

// @kotowari[REQ-merge-036, EX-merge-043]
#[tokio::test(flavor = "multi_thread")]
async fn ex_merge_043_a_new_file_from_a_remote_source_gets_the_source_mode() {
    assert_eq!(copy_new_file_from_remote().await, 0o600);
}
