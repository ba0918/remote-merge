use zeroize::Zeroizing;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum PasswordSource {
    EnvVar,
    Config,
}

pub(super) fn resolve_password(
    server_name: &str,
    config_password: Option<&str>,
) -> Option<(Zeroizing<String>, PasswordSource)> {
    resolve_password_with(server_name, config_password, |key| std::env::var(key))
}

fn resolve_password_with(
    server_name: &str,
    config_password: Option<&str>,
    read_env: impl FnOnce(&str) -> Result<String, std::env::VarError>,
) -> Option<(Zeroizing<String>, PasswordSource)> {
    let env_key = crate::server_env::server_env_name("REMOTE_MERGE_PASSWORD_", server_name);
    if let Ok(val) = read_env(&env_key)
        && !val.is_empty()
    {
        return Some((Zeroizing::new(val), PasswordSource::EnvVar));
    }
    config_password.map(|p| (Zeroizing::new(p.to_string()), PasswordSource::Config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_password_env_var_takes_priority() {
        let (pass, source) = resolve_password_with("testserver", Some("config-pass"), |_| {
            Ok("env-pass".to_string())
        })
        .expect("パスワードが解決されるべき");
        assert_eq!(pass.as_str(), "env-pass");
        assert_eq!(source, PasswordSource::EnvVar);
    }

    #[test]
    fn test_resolve_password_falls_back_to_config() {
        let (pass, source) = resolve_password_with("fallbacktest", Some("config-pass"), |_| {
            Err(std::env::VarError::NotPresent)
        })
        .expect("パスワードが解決されるべき");
        assert_eq!(pass.as_str(), "config-pass");
        assert_eq!(source, PasswordSource::Config);
    }

    #[test]
    fn test_resolve_password_empty_env_var_falls_back_to_config() {
        let (pass, source) =
            resolve_password_with("emptyenvtest", Some("config-pass"), |_| Ok(String::new()))
                .expect("パスワードが解決されるべき");
        assert_eq!(pass.as_str(), "config-pass");
        assert_eq!(source, PasswordSource::Config);
    }

    #[test]
    fn test_resolve_password_no_source_returns_none() {
        assert!(
            resolve_password_with("nosource", None, |_| Err(std::env::VarError::NotPresent))
                .is_none()
        );
    }

    #[test]
    fn the_env_name_is_the_server_name_uppercased_with_hyphens_and_dots_as_underscores() {
        let (pass, source) = resolve_password_with("My-sErver.x_ß", None, |key| {
            assert_eq!(key, "REMOTE_MERGE_PASSWORD_MY_SERVER_X_SS");
            Ok("found".to_string())
        })
        .expect("パスワードが解決されるべき");
        assert_eq!(pass.as_str(), "found");
        assert_eq!(source, PasswordSource::EnvVar);
    }

    #[test]
    fn test_resolve_password_returns_zeroizing_env_var() {
        let result: Option<(Zeroizing<String>, PasswordSource)> =
            resolve_password_with("zeroizetest", None, |_| Ok("zeroize-me".to_string()));
        let (pass, source) = result.expect("Zeroizing パスワードが返るべき");
        assert_eq!(pass.as_str(), "zeroize-me");
        assert_eq!(source, PasswordSource::EnvVar);
    }

    #[test]
    fn test_resolve_password_returns_zeroizing_config() {
        let result: Option<(Zeroizing<String>, PasswordSource)> =
            resolve_password_with("zeroconfigtest", Some("config-secret"), |_| {
                Err(std::env::VarError::NotPresent)
            });
        let (pass, source) = result.expect("Zeroizing パスワードが返るべき");
        assert_eq!(pass.as_str(), "config-secret");
        assert_eq!(source, PasswordSource::Config);
    }

    #[test]
    fn unreadable_environment_falls_back_to_config() {
        let (pass, source) = resolve_password_with("server", Some("config-pass"), |_| {
            Err(std::env::VarError::NotUnicode("unreadable".into()))
        })
        .expect("設定にフォールバックするべき");
        assert_eq!(pass.as_str(), "config-pass");
        assert_eq!(source, PasswordSource::Config);
    }

    #[test]
    fn empty_environment_without_config_returns_none() {
        assert!(resolve_password_with("server", None, |_| Ok(String::new())).is_none());
    }

    #[test]
    fn empty_config_is_preserved() {
        let (pass, source) =
            resolve_password_with("server", Some(""), |_| Err(std::env::VarError::NotPresent))
                .expect("空の設定値も保持するべき");
        assert!(pass.is_empty());
        assert_eq!(source, PasswordSource::Config);
    }

    #[test]
    fn runtime_environment_is_read_in_child_process() {
        const CHILD: &str = "REMOTE_MERGE_PASSWORD_TEST_CHILD";
        const KEY: &str = "REMOTE_MERGE_PASSWORD_RUNTIME_BOUNDARY";
        if let Ok(case) = std::env::var(CHILD) {
            let (pass, source) = resolve_password("runtime-boundary", Some("config-pass"))
                .expect("パスワードが解決されるべき");
            if case == "nonempty" {
                assert_eq!(pass.as_str(), "env-pass");
                assert_eq!(source, PasswordSource::EnvVar);
            } else {
                assert_eq!(pass.as_str(), "config-pass");
                assert_eq!(source, PasswordSource::Config);
            }
            return;
        }

        for (case, value) in [
            ("nonempty", Some("env-pass")),
            ("empty", Some("")),
            ("absent", None),
        ] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "password::tests::runtime_environment_is_read_in_child_process",
                ])
                .env(CHILD, case)
                .env_remove(KEY);
            if let Some(value) = value {
                command.env(KEY, value);
            }
            let output = command.output().unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains("1 passed"),
                "子プロセスの検証に失敗: {case}\n{stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
