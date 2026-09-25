#[path = "contract/backup_cleanup.rs"]
mod backup_cleanup;
#[path = "contract/backup_location.rs"]
mod backup_location;
#[path = "contract/cli_results.rs"]
mod cli_results;
#[path = "contract/config_precedence.rs"]
mod config_precedence;
#[path = "contract/diagnostics.rs"]
mod diagnostics;
#[path = "contract/filters.rs"]
mod filters;
#[path = "contract/init.rs"]
mod init;
#[path = "contract/merge_paths.rs"]
mod merge_paths;
#[path = "contract/rollback_paths.rs"]
mod rollback_paths;
#[path = "contract/scan_limits.rs"]
mod scan_limits;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_compatibility.rs"]
mod ssh_compatibility;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_disconnect.rs"]
mod ssh_disconnect;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_external_link.rs"]
mod ssh_external_link;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_fallback.rs"]
mod ssh_fallback;
#[path = "contract/ssh_host_key.rs"]
mod ssh_host_key;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_reconnect.rs"]
mod ssh_reconnect;
#[cfg(feature = "test-utils")]
use common::ssh_server;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_sudo.rs"]
mod ssh_sudo;
#[path = "contract/status_results.rs"]
mod status_results;
#[path = "contract/tui_confirm.rs"]
mod tui_confirm;
#[path = "contract/tui_directory_links.rs"]
mod tui_directory_links;
#[path = "contract/tui_export.rs"]
mod tui_export;
#[path = "contract/tui_navigation.rs"]
mod tui_navigation;
#[path = "contract/tui_reference.rs"]
mod tui_reference;

// @kotowari[REQ-testing-002]
#[cfg(feature = "test-utils")]
#[test]
fn two_synchronous_cli_sessions_compare_and_write_only_their_own_remote_files() {
    use std::fs;

    let first = common::CliEnv::new(
        &[("same.txt", "first local long version\n")],
        &[("same.txt", "first remote\n")],
    );
    let second = common::CliEnv::new(
        &[("same.txt", "second local long version\n")],
        &[("same.txt", "second remote\n")],
    );

    for env in [&first, &second] {
        let config: toml::Value = fs::read_to_string(&env.config_path)
            .unwrap()
            .parse()
            .unwrap();
        let server = &config["servers"]["develop"];
        assert_eq!(server["host"].as_str(), Some("127.0.0.1"));
        let port = server["port"].as_integer().expect("fixture port missing");
        assert!(
            port > 0 && port != 22,
            "refusing an unowned SSH port: {port}"
        );
        assert_eq!(server["auth"].as_str(), Some("password"));
        assert!(server.get("key").is_none(), "refusing a HOME key path");
        assert_eq!(config["local"]["root_dir"].as_str(), env.local_dir.to_str());
        assert_eq!(server["root_dir"].as_str(), env.remote_dir.to_str());
        assert!(env.local_dir.starts_with(env.temp_root()));
        assert!(env.remote_dir.starts_with(env.temp_root()));
    }

    for (env, expected, other) in [
        (&first, "first local long version\n", "second remote\n"),
        (&second, "second local long version\n", "first remote\n"),
    ] {
        let status = env.cmd_with("diff").arg("same.txt").output().unwrap();
        let text = String::from_utf8_lossy(&status.stdout);
        assert!(text.contains(expected.trim()), "{status:?}");
        assert!(!text.contains(other.trim()), "{status:?}");

        let result = env
            .cmd_with("merge")
            .args(["same.txt", "--left", "local", "--right", "develop"])
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        assert_eq!(
            fs::read_to_string(env.remote_dir.join("same.txt")).unwrap(),
            expected
        );
    }
    assert_ne!(first.remote_dir, second.remote_dir);
}

// @kotowari[REQ-testing-001, REQ-testing-002]
#[cfg(feature = "test-utils")]
#[test]
fn unsafe_ssh_configuration_is_rejected_before_a_cli_process_can_start() {
    use std::fs;

    let env = common::CliEnv::new(&[], &[]);
    let safe = fs::read_to_string(&env.config_path).unwrap();
    let config: toml::Value = safe.parse().unwrap();
    let owned_port = config["servers"]["develop"]["port"].as_integer().unwrap();
    assert_ne!(owned_port, 22);

    let unsafe_port = safe.replace(&format!("port = {owned_port}"), "port = 22");
    fs::write(&env.config_path, unsafe_port).unwrap();
    assert!(std::panic::catch_unwind(|| env.cmd()).is_err());

    fs::write(
        &env.config_path,
        safe.replace(
            "auth = \"password\"",
            "auth = \"key\"\nkey = \"~/.ssh/id_ed25519\"",
        ),
    )
    .unwrap();
    assert!(std::panic::catch_unwind(|| env.cmd()).is_err());
}
#[cfg(feature = "test-utils")]
mod common;
