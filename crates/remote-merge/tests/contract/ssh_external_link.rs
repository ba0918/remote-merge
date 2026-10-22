#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;

use tempfile::TempDir;

use super::agent_support::{AgentFixture, Setup, json};
use super::args_support::merge_args;
use super::server_faults::Faults;

fn verify_local_external_link() {
    use remote_merge::cli::merge::{MergeArgs, MergeCommandOutput, execute_merge};
    use remote_merge::config::load_config_from_paths;
    use remote_merge::runtime::RuntimeTargets;

    let home = TempDir::new().unwrap();
    let local = home.path().join("local");
    let remote = home.path().join("remote");
    let shared = home.path().join("shared");
    fs::create_dir_all(local.join("linked")).unwrap();
    fs::create_dir_all(&remote).unwrap();
    fs::create_dir_all(&shared).unwrap();
    fs::write(local.join("linked/file.txt"), "incoming\n").unwrap();
    let actual = shared.join("file.txt");
    fs::write(&actual, "existing\n").unwrap();
    symlink(&shared, remote.join("linked")).unwrap();
    let config_path = home.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = true\n",
        local.to_str().unwrap(), remote.to_str().unwrap(),
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let backup_root = home.path().join("backups");
    let targets = RuntimeTargets::production()
        .with_local("fixture", &remote)
        .with_backup_store(Some(backup_root.clone()));
    let result = execute_merge(
        MergeArgs {
            right: Some("fixture".into()),
            ..merge_args(&["linked/file.txt"])
        },
        config,
        targets,
    )
    .unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("merge did not report a file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert!(output.merged[0].backup.is_some(), "{output:?}");
    assert_eq!(fs::read(actual).unwrap(), b"incoming\n");
    assert_eq!(fs::read_link(remote.join("linked")).unwrap(), shared);
    assert_old_content_was_backed_up(&backup_root);
}

fn assert_old_content_was_backed_up(backup_root: &std::path::Path) {
    let saved = walkdir::WalkDir::new(backup_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() == "content")
        .map(|entry| fs::read(entry.path()).unwrap())
        .collect::<Vec<_>>();
    assert!(
        saved.iter().any(|bytes| bytes == b"existing\n"),
        "backup did not preserve the original file: {saved:?}"
    );
}

// @kotowari[EX-merge-007]
#[test]
fn an_external_parent_link_is_backed_up_and_updated_locally() {
    verify_local_external_link();
}

// @kotowari[EX-merge-007, EX-merge-058]
#[tokio::test(flavor = "multi_thread")]
async fn an_external_parent_link_is_backed_up_and_updated_through_the_agent() {
    // ファイルを直接指定しても root 全体を指定しても、書き込み先のディレクトリへの symlink の中を比べて書く
    for requested in ["linked/file.txt", "."] {
        let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
        let (local, remote) = (fixture.local(), fixture.remote());
        let shared = fixture.home.path().join("shared");
        fs::create_dir_all(local.join("linked")).unwrap();
        fs::create_dir_all(&shared).unwrap();
        // 大きさを変え、root 全体の比較で大きさと更新時刻だけで同じと判定される回を作らない
        fs::write(local.join("linked/file.txt"), "incoming contents\n").unwrap();
        let actual = shared.join("file.txt");
        fs::write(&actual, "existing\n").unwrap();
        symlink(&shared, remote.join("linked")).unwrap();

        let output = fixture.run(&["merge", requested]);
        assert!(output.status.success(), "{requested}: {output:?}");
        let result = json(&output);
        assert_eq!(result["merged"][0]["path"], "linked/file.txt", "{result}");
        assert!(result["merged"][0]["backup"].as_str().is_some(), "{result}");
        assert!(
            result["skipped"]
                .as_array()
                .unwrap()
                .iter()
                .all(|entry| entry["path"] != "linked"),
            "{result}"
        );
        assert_eq!(fs::read(&actual).unwrap(), b"incoming contents\n");
        assert_eq!(fs::read_link(remote.join("linked")).unwrap(), shared);
        let backup_root = fixture.home.path().join("data/remote-merge/backups");
        assert_old_content_was_backed_up(&backup_root);
    }
}
