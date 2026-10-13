//! SSH のスクリプトをこのマシンの sh（または REMOTE_MERGE_TEST_SCAN_IMAGE のコンテナ）で動かす
//! テストの道具

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::scan_test_shell::shell_command;

pub(crate) fn run(script: &str) -> Result<Vec<u8>, String> {
    let output = shell_command(script).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    Ok(output.stdout)
}

/// `bin` だけを PATH にして動かす
pub(crate) fn run_with_only(bin: &Path) -> impl FnMut(&str) -> Result<Vec<u8>, String> + '_ {
    move |script: &str| run(&format!("PATH='{}'; export PATH\n{script}", bin.display()))
}

/// `tools` だけを置いた PATH。どれも、スクリプトを動かすマシンの /usr/bin か /bin にあるものを
/// 起動する小さなスクリプトにする（どちらにあるかはマシンで違う）
pub(crate) fn bin_with(tools: &[&str]) -> tempfile::TempDir {
    let bin = tempfile::tempdir().unwrap();
    for tool in tools {
        let path = bin.path().join(tool);
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nfor d in /usr/bin /bin; do [ -x \"$d/{tool}\" ] && exec \"$d/{tool}\" \"$@\"; done\nexit 127\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}
