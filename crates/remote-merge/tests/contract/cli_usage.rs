//! サブコマンドなしの起動（docs/ir/cli/safety.md の REQ-cli-076）の契約テスト。

use std::fs;
use std::process::{Command, Stdio};

use tempfile::TempDir;

// @kotowari[REQ-cli-076]
#[test]
fn starting_without_a_subcommand_shows_usage_and_writes_nothing() {
    for flags in [&[][..], &["-y"][..], &["--debug"][..]] {
        let home = TempDir::new().unwrap();
        let cwd = home.path().join("cwd");
        fs::create_dir_all(&cwd).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
            .env_clear()
            .env("HOME", home.path().join("home"))
            .env("XDG_CACHE_HOME", home.path().join("cache"))
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_DATA_HOME", home.path().join("data"))
            .current_dir(&cwd)
            .args(flags)
            .stdin(Stdio::null())
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(2), "{flags:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{flags:?}: {output:?}");
        assert!(!output.stderr.is_empty(), "{flags:?}: no usage was shown");
        assert!(
            !home.path().join("cache").exists(),
            "{flags:?}: the diagnostics location was created"
        );
        assert_eq!(
            fs::read_dir(&cwd).unwrap().count(),
            0,
            "{flags:?}: something was written to the current directory"
        );
    }
}

// @kotowari[REQ-cli-076]
#[test]
fn help_and_version_are_exempt_and_answer_on_stdout_with_success() {
    for flag in ["--help", "--version"] {
        let home = TempDir::new().unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
            .env_clear()
            .env("HOME", home.path().join("home"))
            .env("XDG_CACHE_HOME", home.path().join("cache"))
            .current_dir(home.path())
            .arg(flag)
            .stdin(Stdio::null())
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(0), "{flag}: {output:?}");
        assert!(output.stderr.is_empty(), "{flag}: {output:?}");
        assert!(!output.stdout.is_empty(), "{flag}: nothing was shown");
    }
    let version = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&version.stdout).contains(env!("CARGO_PKG_VERSION")),
        "{version:?}"
    );
}
