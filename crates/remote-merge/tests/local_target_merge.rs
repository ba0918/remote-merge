use std::fs;

use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
use tempfile::TempDir;

#[test]
fn substituted_develop_merges_locally_without_ssh() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "from local\n").unwrap();
    fs::write(develop.path().join("file.txt"), "from develop\n").unwrap();

    let config_path = local.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
[local]
root_dir = "{}"

[servers.develop]
host = "example.invalid"
user = "unused"
root_dir = "/not-used"

[backup]
enabled = false
"#,
            local.path().display()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production().with_local("develop", develop.path());
    let args = MergeArgs {
        paths: vec!["file.txt".into()],
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: false,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
    };

    let result = execute_merge(args, config, targets).unwrap();

    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "from local\n"
    );
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file merge output");
    };
    assert!(output.merged.iter().any(|file| file.path == "file.txt"));
}
