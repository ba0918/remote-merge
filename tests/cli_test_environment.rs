mod common;

use common::{remote_merge_cmd, CliEnv, E2eEnv};

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
fn tui_command_uses_isolated_data_home() {
    let env = E2eEnv::new(&[], &[]);
    let command = env.tui_command(&[]);
    assert!(xdg_data_home(&command).is_some_and(|path| path.starts_with(env.temp_root())));
}
