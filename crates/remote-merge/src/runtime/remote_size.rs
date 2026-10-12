//! リモートの経路がファイルの大きさを答えるための、SSH のコマンドとエージェントの答えの読み方。
//!
//! 大きさは読み込みと同じく symlink を辿った先のファイルについて調べる。上限の判定はここでは
//! せず、`size_limit` が入口で行う。

use crate::ssh::batch_read::{chunk_paths, SSH_BATCH_MAX_COMMAND_LEN};
use crate::ssh::tree_parser::shell_escape;

/// SSH の大きさのコマンドの先頭の並び。最初に既知のパス "/" で stat が使え `-L` と `%s` を
/// 受け付けることを確かめ、だめなら 0 以外で終える。stat が使えないことをパスごとの失敗と
/// 取り違えると、どのパスも大きさ不明として読み込みに進み、上限が黙って外れる。
/// 試験 SSH サーバは test-utils の機能でこれを読んで大きさの問い合わせを見分ける
pub const FILE_SIZE_PREFIX: &str = "LC_ALL=C; c=$(stat -L -c %s / 2>/dev/null) || exit 3; ";

/// パスの順番に、大きさを "S<バイト数>"、パスごとの失敗を "E" として一行ずつ出し、最後に
/// "END" を出すコマンド。名前は出力しない（改行を含む名前で解析が壊れないように）
pub fn build_file_size_command(full_paths: &[String]) -> String {
    let quoted: Vec<String> = full_paths.iter().map(|path| shell_escape(path)).collect();
    format!(
        "{FILE_SIZE_PREFIX}case $c in ''|*[!0-9]*) exit 3 ;; esac; for p in {}; do if v=$(stat -L -c %s -- \"$p\" 2>/dev/null); then printf 'S%s\\n' \"$v\"; else printf 'E\\n'; fi; done; printf 'END\\n'",
        quoted.join(" ")
    )
}

/// 尋ねるパスを SSH の一括読み込みと同じ長さの上限で分け、分けたパスとそのコマンドの組にする
pub fn file_size_commands(full_paths: &[String]) -> Vec<(Vec<String>, String)> {
    chunk_paths(full_paths, SSH_BATCH_MAX_COMMAND_LEN)
        .into_iter()
        .map(|chunk| {
            let command = build_file_size_command(&chunk);
            (chunk, command)
        })
        .collect()
}

/// `count` 個のパスを尋ねたコマンドの出力を、パスの順番の大きさ（パスごとの失敗は None）にする。
/// 数が合わない、知らない行がある、終わりの印がない出力は、終了コードによらず操作の失敗にする
pub fn parse_file_size_output(output: &[u8], count: usize) -> anyhow::Result<Vec<Option<u64>>> {
    let malformed = || {
        anyhow::anyhow!(
            "unexpected output from the file size command: {:?}",
            String::from_utf8_lossy(&output[..output.len().min(200)])
        )
    };
    let text = std::str::from_utf8(output).map_err(|_| malformed())?;
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.pop() != Some("") || lines.pop() != Some("END") || lines.len() != count {
        return Err(malformed());
    }
    lines
        .into_iter()
        .map(|line| match line.strip_prefix('S') {
            Some(digits) if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
                digits.parse().map(Some).map_err(|_| malformed())
            }
            _ if line == "E" => Ok(None),
            _ => Err(malformed()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    // ── SSH のコマンド ──

    #[test]
    fn many_paths_are_split_into_commands_within_the_length_limit() {
        let long = "x".repeat(200);
        let full_paths: Vec<String> = (0..1000).map(|i| format!("/srv/{long}/{i}")).collect();

        let commands = file_size_commands(&full_paths);

        assert!(commands.len() > 1);
        let asked: usize = commands.iter().map(|(chunk, _)| chunk.len()).sum();
        assert_eq!(asked, full_paths.len());
        for (_, command) in &commands {
            assert!(
                command.len() <= SSH_BATCH_MAX_COMMAND_LEN,
                "{}",
                command.len()
            );
            assert!(command.starts_with(FILE_SIZE_PREFIX));
        }
    }

    #[test]
    fn sizes_and_per_path_failures_are_read_in_the_order_asked() {
        assert_eq!(
            parse_file_size_output(b"S10\nE\nS0\nEND\n", 3).unwrap(),
            vec![Some(10), None, Some(0)]
        );
    }

    #[test]
    fn output_that_does_not_match_the_paths_asked_is_a_failure() {
        for output in [
            b"S10\nEND\n".as_slice(),
            b"S10\nE\nS1\nEND\n",
            b"S10\nE\n",
            b"S1x\nE\nEND\n",
            b"ready\n",
            b"",
        ] {
            assert!(
                parse_file_size_output(output, 2).is_err(),
                "{}",
                String::from_utf8_lossy(output)
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_command_reports_the_size_of_a_symlink_target_and_a_missing_path_as_unknown() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("file"), "12345").unwrap();
        std::os::unix::fs::symlink(root.join("file"), root.join("link")).unwrap();
        let full_paths: Vec<String> = ["file", "link", "missing", "it's"]
            .iter()
            .map(|name| root.join(name).to_string_lossy().into_owned())
            .collect();
        std::fs::write(root.join("it's"), "").unwrap();

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(build_file_size_command(&full_paths))
            .output()
            .unwrap();

        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            parse_file_size_output(&output.stdout, full_paths.len()).unwrap(),
            vec![Some(5), Some(5), None, Some(0)]
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_command_fails_as_a_whole_when_stat_cannot_report_sizes() {
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(build_file_size_command(&paths(&["/"])))
            .env_clear()
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();

        assert_ne!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
    }
}
