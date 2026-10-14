#![expect(
    unsafe_code,
    reason = "テストは権限・FIFO・時刻を直接扱うため libc を呼ぶ"
)]
mod common;

use common::{CliEnv, remote_merge_cmd};

fn xdg_data_home(command: &std::process::Command) -> Option<std::path::PathBuf> {
    command
        .get_envs()
        .find(|(name, _)| *name == "XDG_DATA_HOME")
        .and_then(|(_, value)| value)
        .map(std::path::PathBuf::from)
}

#[test]
fn cli_commands_use_isolated_data_home() {
    let env = CliEnv::new(&[], &[]);
    let command = env.cmd();
    assert!(xdg_data_home(&command).is_some_and(|path| path.starts_with(env.temp_root())));
}

#[test]
fn command_without_config_uses_isolated_data_home() {
    assert!(xdg_data_home(&remote_merge_cmd()).is_some());
}

#[test]
fn the_isolated_homes_of_a_command_without_config_are_removed_with_the_command() {
    let command = remote_merge_cmd();
    let data_home = xdg_data_home(&command).unwrap();
    assert!(data_home.exists());
    drop(command);
    assert!(
        !data_home.exists(),
        "{} was left behind",
        data_home.display()
    );
}

#[cfg(unix)]
#[test]
fn a_command_whose_isolated_homes_cannot_be_removed_says_so() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: geteuid は引数を取らず、常に成功して呼び出したプロセスの実効 uid を返す
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let command = remote_merge_cmd();
    let data_home = xdg_data_home(&command).unwrap();
    let homes = data_home.parent().unwrap().to_path_buf();
    let locked = data_home.join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::write(locked.join("x"), "x").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(command)));

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir_all(&homes).unwrap();
    assert!(dropped.is_err(), "the failed removal was not reported");
}
