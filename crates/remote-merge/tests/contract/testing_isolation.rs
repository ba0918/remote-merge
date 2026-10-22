//! 試験の環境が開発者の環境と、同時に走るほかのテストから隔離されていること
//! （docs/ir/testing/environments.md の REQ-testing-001・002）の契約テスト。

use std::fs;

use super::common::{CliEnv, assert_owned_server};

// @kotowari[REQ-testing-002]
#[test]
fn two_synchronous_cli_sessions_compare_and_write_only_their_own_remote_files() {
    let first = CliEnv::new(
        &[("same.txt", "first local long version\n")],
        &[("same.txt", "first remote\n")],
    );
    let second = CliEnv::new(
        &[("same.txt", "second local long version\n")],
        &[("same.txt", "second remote\n")],
    );

    for env in [&first, &second] {
        let config: toml::Table = fs::read_to_string(&env.config_path)
            .unwrap()
            .parse()
            .unwrap();
        let server = &config["servers"]["develop"];
        assert_owned_server(server, env.server_port());
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
#[test]
fn unsafe_ssh_configuration_is_rejected_before_a_cli_process_can_start() {
    let env = CliEnv::new(&[], &[]);
    let safe = fs::read_to_string(&env.config_path).unwrap();
    let config: toml::Table = safe.parse().unwrap();
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
