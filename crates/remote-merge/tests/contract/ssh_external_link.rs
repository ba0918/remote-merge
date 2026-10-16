#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::process::Command;

use tempfile::TempDir;

use super::ssh_server::TestServer;

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
            paths: vec!["linked/file.txt".into()],
            left: Some("local".into()),
            right: Some("fixture".into()),
            ref_server: None,
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
            hunks: None,
            encodings: Default::default(),
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

// @kotowari[EX-merge-007, EX-merge-058]
#[tokio::test(flavor = "multi_thread")]
async fn external_parent_links_are_backed_up_and_updated_locally_and_through_the_agent() {
    std::thread::spawn(verify_local_external_link)
        .join()
        .unwrap();
    // ファイルを直接指定しても root 全体を指定しても、書き込み先のディレクトリへの symlink の中を比べて書く
    for requested in ["linked/file.txt", "."] {
        // 配置先とその祖先はグループやその他が書き込めると Agent が実行されないため、umask によらず決める
        let home = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let local = home.path().join("local");
        let remote = home.path().join("remote");
        let shared = home.path().join("shared");
        fs::create_dir_all(local.join("linked")).unwrap();
        fs::create_dir_all(&remote).unwrap();
        fs::create_dir_all(&shared).unwrap();
        // 大きさを変え、root 全体の比較で大きさと更新時刻だけで同じと判定される回を作らない
        fs::write(local.join("linked/file.txt"), "incoming contents\n").unwrap();
        let actual = shared.join("file.txt");
        fs::write(&actual, "existing\n").unwrap();
        symlink(&shared, remote.join("linked")).unwrap();

        let agent_dir = home.path().join("agent");
        let binary = agent_dir.join("remote-merge-testuser/remote-merge");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(binary.parent().unwrap())
            .unwrap();
        symlink(env!("CARGO_BIN_EXE_remote-merge"), &binary).unwrap();
        let agent_binary = binary.clone();
        let server = TestServer::filesystem_with_agent(home.path()).await;
        let config_path = home.path().join("config.toml");
        fs::write(&config_path, format!(
            "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = true\ndeploy_dir = {:?}\n[backup]\nenabled = true\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
            local.to_str().unwrap(), server.port(), remote.to_str().unwrap(), agent_dir.to_str().unwrap(),
        )).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
            .env("HOME", home.path())
            .env("XDG_CACHE_HOME", home.path().join("cache"))
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_DATA_HOME", home.path().join("data"))
            .env("REMOTE_MERGE_AGENT_BINARY", agent_binary)
            .args([
                "merge", requested, "--left", "local", "--right", "fixture", "--format", "json",
            ])
            .arg("--config")
            .arg(&config_path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{requested}: {output:?}");
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
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
        let backup_root = home.path().join("data/remote-merge/backups");
        assert_old_content_was_backed_up(&backup_root);
    }
}
