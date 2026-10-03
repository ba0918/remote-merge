use crate::ssh::tree_parser::shell_escape;

/// sudo が NOPASSWD で利用可能かチェックするコマンドを生成する。
///
/// `sudo -n true` は NOPASSWD 設定がある場合のみ成功する（exit 0）。
pub fn build_sudo_check_command() -> &'static str {
    "sudo -n true"
}

/// 指定ユーザーの uid/gid を取得するコマンドを生成する。
///
/// `id -u {user} && id -g {user}` の形式で出力する。
pub fn build_id_command(user: &str) -> String {
    let escaped = shell_escape(user);
    format!("id -u {escaped} && id -g {escaped}")
}

/// `id` コマンドの出力をパースして (uid, gid) タプルを返す。
///
/// 期待フォーマット: 2行の数値
/// ```text
/// 1000
/// 1000
/// ```
pub fn parse_id_output(output: &str) -> Result<(u32, u32), String> {
    let lines: Vec<&str> = output.trim().lines().collect();
    if lines.len() != 2 {
        return Err(format!(
            "expected 2 lines (uid and gid), got {}: {:?}",
            lines.len(),
            output.trim()
        ));
    }
    let uid: u32 = lines[0]
        .trim()
        .parse()
        .map_err(|e| format!("failed to parse uid '{}': {e}", lines[0].trim()))?;
    let gid: u32 = lines[1]
        .trim()
        .parse()
        .map_err(|e| format!("failed to parse gid '{}': {e}", lines[1].trim()))?;
    Ok((uid, gid))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- build_sudo_check_command ---

    #[test]
    fn build_sudo_check_command_returns_sudo_n_true() {
        assert_eq!(build_sudo_check_command(), "sudo -n true");
    }

    // --- build_id_command ---

    #[test]
    fn build_id_command_basic() {
        let cmd = build_id_command("deploy");
        assert_eq!(cmd, "id -u 'deploy' && id -g 'deploy'");
    }

    #[test]
    fn build_id_command_escapes_special_chars() {
        let cmd = build_id_command("my user");
        assert!(cmd.contains("'my user'"), "cmd={cmd}");
    }

    // --- parse_id_output ---

    #[test]
    fn parse_id_output_valid() {
        assert_eq!(parse_id_output("1000\n1000\n"), Ok((1000, 1000)));
    }

    #[test]
    fn parse_id_output_different_uid_gid() {
        assert_eq!(parse_id_output("1000\n33\n"), Ok((1000, 33)));
    }

    #[test]
    fn parse_id_output_with_trailing_whitespace() {
        assert_eq!(parse_id_output("  1000  \n  33  \n"), Ok((1000, 33)));
    }

    #[test]
    fn parse_id_output_single_line_rejected() {
        assert!(parse_id_output("1000\n").is_err());
    }

    #[test]
    fn parse_id_output_empty_rejected() {
        assert!(parse_id_output("").is_err());
    }

    #[test]
    fn parse_id_output_non_numeric_rejected() {
        assert!(parse_id_output("abc\n1000\n").is_err());
    }

    #[test]
    fn parse_id_output_three_lines_rejected() {
        assert!(parse_id_output("1000\n1000\nextra\n").is_err());
    }
}
