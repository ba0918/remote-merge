#![cfg(unix)]
//! CentOS 5 testenv に対するリモートバックアップ E2E。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serial_test::serial;
use tempfile::TempDir;

const REMOTE_BASE: &str = "/srv/testdata/remote-merge-e2e";
const SHARED_BASE: &str = "/home/testuser/remote-merge-e2e-shared";
const DEPLOYED_AGENT: &str = "/var/tmp/remote-merge-testuser/remote-merge";

#[derive(Clone, Copy, Debug)]
enum Transport {
    Ssh,
    Agent,
}

impl Transport {
    fn all() -> [Self; 2] {
        [Self::Ssh, Self::Agent]
    }

    fn name(self) -> &'static str {
        match self {
            Self::Ssh => "ssh",
            Self::Agent => "agent",
        }
    }

    fn uses_agent(self) -> bool {
        matches!(self, Self::Agent)
    }
}

struct Harness {
    temp: TempDir,
    container: String,
    server_toml: toml::Value,
    agent_binary: PathBuf,
}

impl Harness {
    fn from_environment() -> Self {
        let source = std::env::var_os("REMOTE_MERGE_E2E_SERVER_TOML")
            .map(PathBuf::from)
            .expect("REMOTE_MERGE_E2E_SERVER_TOML must point to the server TOML");
        let container = std::env::var("REMOTE_MERGE_E2E_CONTAINER")
            .expect("REMOTE_MERGE_E2E_CONTAINER must name the E2E container");
        let agent_binary = std::env::var_os("REMOTE_MERGE_AGENT_BINARY")
            .map(PathBuf::from)
            .expect("REMOTE_MERGE_AGENT_BINARY must point to the musl agent binary");
        assert!(agent_binary.is_file(), "musl agent binary does not exist");
        let server_toml = fs::read_to_string(&source)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()))
            .parse::<toml::Value>()
            .unwrap_or_else(|error| panic!("invalid server TOML: {error}"));
        assert!(
            server_toml
                .get("servers")
                .and_then(|servers| servers.get("develop"))
                .is_some(),
            "server TOML must contain [servers.develop]"
        );
        Self {
            temp: TempDir::new().expect("failed to create temporary directory"),
            container,
            server_toml,
            agent_binary,
        }
    }

    fn root(&self, case: &str, transport: Transport) -> String {
        format!("{REMOTE_BASE}/{case}-{}", transport.name())
    }

    fn shared(&self, case: &str, transport: Transport) -> String {
        format!("{SHARED_BASE}/{case}-{}", transport.name())
    }

    fn local(&self, case: &str, transport: Transport) -> PathBuf {
        self.temp
            .path()
            .join(format!("{case}-{}", transport.name()))
    }

    fn docker(&self, script: &str) -> String {
        let output = Command::new("docker")
            .args([
                "exec",
                "-u",
                "testuser",
                &self.container,
                "sh",
                "-c",
                script,
            ])
            .output()
            .expect("failed to execute docker");
        assert_success(&output, "docker exec");
        String::from_utf8(output.stdout).expect("docker output was not UTF-8")
    }

    fn reset(&self, case: &str, transport: Transport, setup: &str) {
        let root = self.root(case, transport);
        let shared = self.shared(case, transport);
        self.docker(&format!(
            "rm -rf '{root}' '{shared}'; mkdir -p '{root}' '{shared}'; {setup}"
        ));
    }

    fn config(&self, case: &str, transport: Transport, remote_root: &str) -> PathBuf {
        let local = self.local(case, transport);
        fs::create_dir_all(&local).expect("failed to create local root");
        let mut config = self.server_toml.clone();
        config["servers"]["develop"]["root_dir"] = toml::Value::String(remote_root.into());
        let mut local_config = toml::map::Map::new();
        local_config.insert(
            "root_dir".into(),
            toml::Value::String(local.to_string_lossy().into_owned()),
        );
        config["local"] = toml::Value::Table(local_config);
        config["ssh"] = toml::toml! {
            timeout_sec = 30
            strict_host_key_checking = "no"
        }
        .into();
        config["backup"] = toml::toml! { enabled = true retention_days = 7 }.into();
        let mut agent_config = toml::map::Map::new();
        agent_config.insert(
            "enabled".into(),
            toml::Value::Boolean(transport.uses_agent()),
        );
        config["agent"] = toml::Value::Table(agent_config);
        let path = self
            .temp
            .path()
            .join(format!("{case}-{}.toml", transport.name()));
        fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        path
    }

    fn local_file(&self, case: &str, transport: Transport, path: &str) {
        let path = self.local(case, transport).join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "merged").unwrap();
    }

    fn run(&self, transport: Transport, args: &[&str]) -> Output {
        let home = self.temp.path().join(format!("home-{}", transport.name()));
        let data = home.join("data");
        fs::create_dir_all(&data).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
        command
            .args(args)
            .env_clear()
            .env("HOME", &home)
            .env("XDG_DATA_HOME", data);
        if transport.uses_agent() {
            command.env("REMOTE_MERGE_AGENT_BINARY", &self.agent_binary);
        }
        command.output().expect("failed to execute remote-merge")
    }

    fn merge(&self, config: &Path, transport: Transport, path: &str) -> Output {
        self.run(
            transport,
            &[
                "--config",
                config.to_str().unwrap(),
                "merge",
                path,
                "--left",
                "local",
                "--right",
                "develop",
                "--force",
            ],
        )
    }

    fn rollback(&self, config: &Path, transport: Transport) -> Output {
        self.run(
            transport,
            &[
                "--config",
                config.to_str().unwrap(),
                "rollback",
                "--target",
                "develop",
                "--force",
                "--format",
                "json",
            ],
        )
    }
}

fn assert_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore]
#[serial]
fn remote_merge_keeps_backups_outside_the_target_root() {
    let h = Harness::from_environment();
    for transport in Transport::all() {
        let case = "aggregate-only";
        let root = h.root(case, transport);
        h.reset(
            case,
            transport,
            &format!("printf original > '{root}/file.txt'"),
        );
        h.local_file(case, transport, "file.txt");
        let config = h.config(case, transport, &root);
        let before = h.docker(&format!("find '{root}' -mindepth 1 -maxdepth 1 -print"));
        assert_success(&h.merge(&config, transport, "file.txt"), "merge");
        let after = h.docker(&format!("find '{root}' -mindepth 1 -maxdepth 1 -print"));
        assert_eq!(before, after, "merge added an entry below the remote root");
    }
}

#[test]
#[ignore]
#[serial]
fn rollback_follows_an_unchanged_symlink_root() {
    let h = Harness::from_environment();
    for transport in Transport::all() {
        let case = "stable-root-link";
        let root = h.root(case, transport);
        h.reset(case, transport, &format!("mkdir -p '{root}/releases/A'; printf original > '{root}/releases/A/file.txt'; ln -s releases/A '{root}/current'"));
        h.local_file(case, transport, "file.txt");
        let config = h.config(case, transport, &format!("{root}/current"));
        assert_success(&h.merge(&config, transport, "file.txt"), "merge");
        assert_success(&h.rollback(&config, transport), "rollback");
        assert_eq!(
            h.docker(&format!("cat '{root}/releases/A/file.txt'")),
            "original"
        );
    }
}

#[test]
#[ignore]
#[serial]
fn rollback_skips_a_repointed_symlink_root() {
    let h = Harness::from_environment();
    for transport in Transport::all() {
        let case = "repointed-root-link";
        let root = h.root(case, transport);
        h.reset(case, transport, &format!("mkdir -p '{root}/releases/A' '{root}/releases/B'; printf original-a > '{root}/releases/A/file.txt'; printf untouched-b > '{root}/releases/B/file.txt'; ln -s releases/A '{root}/current'"));
        h.local_file(case, transport, "file.txt");
        let config = h.config(case, transport, &format!("{root}/current"));
        assert_success(&h.merge(&config, transport, "file.txt"), "merge");
        h.docker(&format!(
            "rm '{root}/current'; ln -s releases/B '{root}/current'"
        ));
        let output = h.rollback(&config, transport);
        assert_eq!(output.status.code(), Some(2));
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            json["skipped"][0]["reason"],
            "path now resolves to a different location"
        );
        assert_eq!(
            h.docker(&format!("cat '{root}/releases/B/file.txt'")),
            "untouched-b"
        );
    }
}

#[test]
#[ignore]
#[serial]
fn rollback_restores_through_an_intermediate_symlink_outside_the_root() {
    let h = Harness::from_environment();
    for transport in Transport::all() {
        let case = "outside-root-link";
        let root = h.root(case, transport);
        let shared = h.shared(case, transport);
        h.reset(
            case,
            transport,
            &format!("printf original > '{shared}/file.txt'; ln -s '{shared}' '{root}/shared'"),
        );
        h.local_file(case, transport, "shared/file.txt");
        let config = h.config(case, transport, &root);
        assert_success(&h.merge(&config, transport, "shared/file.txt"), "merge");
        assert_success(&h.rollback(&config, transport), "rollback");
        assert_eq!(h.docker(&format!("cat '{shared}/file.txt'")), "original");
    }
}

#[test]
#[ignore]
#[serial]
fn rollback_preserves_existing_remote_owner_and_permissions() {
    let h = Harness::from_environment();
    for transport in Transport::all() {
        let case = "metadata";
        let root = h.root(case, transport);
        h.reset(
            case,
            transport,
            &format!("printf original > '{root}/file.txt'; chmod 640 '{root}/file.txt'"),
        );
        h.local_file(case, transport, "file.txt");
        let config = h.config(case, transport, &root);
        let before = h.docker(&format!("stat -c '%U:%G %a' '{root}/file.txt'"));
        assert_success(&h.merge(&config, transport, "file.txt"), "merge");
        assert_success(&h.rollback(&config, transport), "rollback");
        let after = h.docker(&format!("stat -c '%U:%G %a' '{root}/file.txt'"));
        assert_eq!(before, after);
    }
}

#[test]
#[ignore]
#[serial]
fn agent_setting_selects_the_requested_remote_transport() {
    let h = Harness::from_environment();
    for transport in Transport::all() {
        let case = "transport";
        let root = h.root(case, transport);
        h.docker(&format!("rm -f '{DEPLOYED_AGENT}'"));
        h.reset(
            case,
            transport,
            &format!("printf original > '{root}/file.txt'"),
        );
        h.local_file(case, transport, "file.txt");
        let config = h.config(case, transport, &root);
        assert_success(&h.merge(&config, transport, "file.txt"), "merge");
        let deployed = h.docker(&format!(
            "test -f '{DEPLOYED_AGENT}' && printf yes || printf no"
        ));
        assert_eq!(deployed == "yes", transport.uses_agent());
    }
}
