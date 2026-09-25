use std::fs;

use remote_merge::cli::diff::{execute_diff, DiffArgs};
use remote_merge::cli::status::{execute_status, StatusArgs};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::output::format_json;

fn load_pair(global: &str, project: &str) -> remote_merge::config::AppConfig {
    let dir = tempfile::tempdir().unwrap();
    let global_path = dir.path().join("global.toml");
    let project_path = dir.path().join("project.toml");
    fs::write(&global_path, global).unwrap();
    fs::write(&project_path, project).unwrap();
    load_config_from_paths(Some(&global_path), Some(&project_path)).unwrap()
}

// @kotowari[REQ-config-001, EX-config-001]
#[test]
fn project_server_and_ssh_settings_take_precedence() {
    let config = load_pair(
        r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/tmp/global"

[ssh]
timeout_sec = 15
"#,
        r#"
[servers.develop]
host = "dev-new.example.com"
user = "deploy-new"
root_dir = "/var/www/new-app"

[local]
root_dir = "/tmp/project"

[ssh]
timeout_sec = 30
"#,
    );

    assert_eq!(config.servers["develop"].host, "dev-new.example.com");
    assert_eq!(config.local.root_dir.to_str(), Some("/tmp/project"));
    assert_eq!(config.ssh.timeout_sec, 30);
}

// @kotowari[EX-config-002]
#[test]
fn global_only_server_remains_available_alongside_project_settings() {
    let config = load_pair(
        r#"
[local]
root_dir = "/tmp/global"
[servers.shared]
host = "shared.example.invalid"
user = "deploy"
root_dir = "/srv/shared"
[servers.develop]
host = "old.example.invalid"
user = "deploy"
root_dir = "/srv/old"
"#,
        r#"
[servers.develop]
host = "new.example.invalid"
user = "deploy"
root_dir = "/srv/new"
"#,
    );
    assert_eq!(config.servers["shared"].host, "shared.example.invalid");
    assert_eq!(
        config.servers["shared"].root_dir.to_str(),
        Some("/srv/shared")
    );
    assert_eq!(config.servers["develop"].host, "new.example.invalid");
}

// @kotowari[REQ-config-002]
#[test]
fn filter_patterns_from_both_levels_are_combined() {
    let config = load_pair(
        r#"
[local]
root_dir = "/tmp/global"
[filter]
exclude = ["*.log"]
include = ["src"]
sensitive = ["*.key"]
"#,
        r#"
[filter]
exclude = ["dist"]
include = ["config"]
sensitive = ["*.pem"]
"#,
    );

    for pattern in ["*.log", "dist"] {
        assert!(config.filter.exclude.iter().any(|value| value == pattern));
    }
    for pattern in ["src", "config"] {
        assert!(config.filter.include.iter().any(|value| value == pattern));
    }
    for pattern in ["*.key", "*.pem"] {
        assert!(config.filter.sensitive.iter().any(|value| value == pattern));
    }
}

// @kotowari[EX-config-003]
#[test]
fn status_excludes_files_matching_either_configuration_level() {
    let workspace = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    for path in ["ignored-global.txt", "ignored-project.txt", "visible.txt"] {
        fs::write(local.path().join(path), "new version\n").unwrap();
        fs::write(destination.path().join(path), "old\n").unwrap();
    }
    let global_path = workspace.path().join("global.toml");
    let project_path = workspace.path().join("project.toml");
    fs::write(&global_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[filter]\nexclude = [\"ignored-global.txt\"]\n",
        local.path().display().to_string(), destination.path().display().to_string(),
    )).unwrap();
    fs::write(
        &project_path,
        "[filter]\nexclude = [\"ignored-project.txt\"]\n",
    )
    .unwrap();
    let config = load_config_from_paths(Some(&global_path), Some(&project_path)).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_startup_directory(std::env::current_dir().unwrap());
    let result = execute_status(
        StatusArgs {
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            format: "json".into(),
            summary: false,
            all: true,
            checksum: false,
            verbose: 0,
            max_entries: None,
        },
        config,
        targets,
    )
    .unwrap();
    let paths: Vec<String> = result
        .output
        .files
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect();
    assert_eq!(paths, ["visible.txt"]);
}

// @kotowari[EX-config-004]
#[test]
fn global_sensitive_pattern_still_masks_diff_with_project_configuration() {
    let workspace = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    fs::write(
        local.path().join("private.key"),
        "left confidential material\n",
    )
    .unwrap();
    fs::write(
        destination.path().join("private.key"),
        "right confidential material\n",
    )
    .unwrap();
    let global_path = workspace.path().join("global.toml");
    let project_path = workspace.path().join("project.toml");
    fs::write(&global_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[filter]\nsensitive = [\"*.key\"]\n",
        local.path().display().to_string(), destination.path().display().to_string(),
    )).unwrap();
    fs::write(&project_path, "[filter]\nexclude = [\"*.log\"]\n").unwrap();
    let config = load_config_from_paths(Some(&global_path), Some(&project_path)).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_startup_directory(workspace.path().to_path_buf());
    let (output, _) = execute_diff(
        DiffArgs {
            paths: vec!["private.key".into()],
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            format: "json".into(),
            max_lines: None,
            max_files: 100,
            force: false,
            max_entries: None,
        },
        config,
        targets,
    )
    .unwrap();
    assert_eq!(output.files.len(), 1, "{output:?}");
    assert!(output.files[0].sensitive);
    let encoded = format_json(&output).unwrap();
    assert!(!encoded.contains("left confidential material"), "{encoded}");
    assert!(
        !encoded.contains("right confidential material"),
        "{encoded}"
    );
}
