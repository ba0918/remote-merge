#![cfg(unix)]
//! --with-permissions の merge が、読み込み元の権限として走査したツリーの値を写すこと
//! （docs/ir/merge/permissions.md の REQ-merge-036）と、権限を写せなかったファイルを、中身を書いた
//! ことを添えて失敗として出すこと（REQ-merge-037）の契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::agent_support::{AgentFixture, Setup, failure};
use super::common::stdout_json;
use super::server_faults::Faults;

const FILE: &str = "a.txt";
/// 中身は書いたが権限を写せなかった失敗の error が含む語
const CONTENT_WRITTEN: &str = "content written";

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
    assert_eq!(
        stdout_json(&output)["merged"].as_array().map(Vec::len),
        Some(1)
    );
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

// --hunks の merge でも、--with-permissions は読み込み元の権限を写す（REQ-merge-014・036）
// @kotowari[REQ-merge-014, REQ-merge-036]
#[tokio::test(flavor = "multi_thread")]
async fn req_merge_014_a_hunk_merge_with_permissions_copies_the_source_mode() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put(FILE, Some("local\n"), Some("remote\n"));
    fs::set_permissions(
        fixture.remote().join(FILE),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    fs::set_permissions(
        fixture.local().join(FILE),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();

    let output = fixture.run_with_sides(
        &["merge", FILE, "--hunks", "0", "--with-permissions"],
        &["--left", "fixture", "--right", "local"],
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        stdout_json(&output)["merged"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(mode_of(&fixture.local().join(FILE)), 0o600);
}

// @kotowari[REQ-merge-037, EX-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_permission_copy_reports_the_written_content_with_exit_code_2() {
    let fixture = AgentFixture::new(
        Setup::default(),
        Faults {
            refuse_agent: Some("Chmod"),
            ..Faults::default()
        },
    )
    .await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt", "--with-permissions"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = stdout_json(&output);
    assert!(
        failure(&result["failed"], "a.txt").contains(CONTENT_WRITTEN),
        "{result}"
    );
    assert_eq!(fixture.read_remote("a.txt"), "local\n");
    assert!(fixture.server.faults().count("Chmod") > 0);
}

// @kotowari[EX-merge-051]
#[tokio::test(flavor = "multi_thread")]
async fn a_setuid_source_mode_is_not_copied_and_the_written_content_is_reported() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("run.sh", Some("#!/bin/sh\necho new\n"), Some("#!/bin/sh\n"));
    let local = fixture.local().join("run.sh");
    let remote = fixture.remote().join("run.sh");
    fs::set_permissions(&local, fs::Permissions::from_mode(0o4755)).unwrap();
    fs::set_permissions(&remote, fs::Permissions::from_mode(0o644)).unwrap();

    let output = fixture.run(&["merge", "run.sh", "--with-permissions"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = stdout_json(&output);
    let error = failure(&result["failed"], "run.sh");
    assert!(
        error.contains(CONTENT_WRITTEN) && error.contains("setuid"),
        "{error}"
    );
    assert_eq!(fixture.read_remote("run.sh"), "#!/bin/sh\necho new\n");
    assert_eq!(mode_of(&remote), 0o644);
}
