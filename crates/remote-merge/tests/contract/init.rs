use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use tempfile::TempDir;

fn run_init(dir: &TempDir, input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .arg("init")
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

// @kotowari[EX-cli-023, EX-cli-026]
#[test]
fn first_initialization_creates_an_editable_project_configuration() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(".remote-merge.toml");
    assert!(!config_path.exists());
    let output = run_init(&dir, b"\nexample.invalid\n\n\n\n/srv/app\n\n\n");
    assert!(output.status.success(), "{output:?}");
    let text = fs::read_to_string(config_path).unwrap();
    let parsed: toml::Value = toml::from_str(&text).unwrap();
    assert_eq!(
        parsed["servers"]["develop"]["host"].as_str(),
        Some("example.invalid")
    );
    assert_eq!(
        parsed["servers"]["develop"]["root_dir"].as_str(),
        Some("/srv/app")
    );
    assert_eq!(parsed["local"]["root_dir"].as_str(), Some("."));
}

// @kotowari[EX-cli-024, EX-cli-025]
#[test]
fn reinitialization_does_not_silently_replace_existing_values() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(".remote-merge.toml");
    let original = "[servers.custom]\nhost = \"user-value.invalid\"\n";
    fs::write(&config_path, original).unwrap();
    let output = run_init(&dir, b"n\n");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
    assert!(String::from_utf8_lossy(&output.stderr).contains("Overwrite?"));
}
