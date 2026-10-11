//! SSH の経路で、パスの項目そのもの（最後の要素のリンクは辿らない）が symlink ならそのリンク先を
//! 読むコマンドと、その出力の読み取り。親ディレクトリを一覧せずに一つの項目だけを見る。
//!
//! 結果の分け方はローカルの経路（remote_merge_core::entry_link）と同じにする。コマンドは子の一覧
//! （[`crate::child_listing`]）と同じ find の `-printf` だけを使い、`readlink` などほかの道具は
//! 要らない。

use crate::tree_parser::shell_escape;

/// `path`（サーバ上の絶対パス）の項目の種類とリンク先を出すコマンド。区切りの後のリンク先は
/// 最後まで読んだ文字列そのままで、行の終わりを付けない。ないパスと途中がディレクトリでない
/// パスは、find の原因の文言（C ロケール）で見分ける
pub fn link_target_command(path: &str) -> String {
    format!(
        "LC_ALL=C find {} -maxdepth 0 -printf '%y\\t%l'",
        shell_escape(path)
    )
}

/// [`link_target_command`] の出力を読む。`rel` は root_dir からのパス（文言に使う）
pub fn parse_link_target_output(
    stdout: &str,
    stderr: &str,
    exit_code: u32,
    rel: &str,
) -> anyhow::Result<Option<String>> {
    if exit_code != 0 {
        let cause = stderr.trim_end();
        let absent = [": No such file or directory", ": Not a directory"]
            .iter()
            .any(|ending| cause.ends_with(ending));
        if absent {
            return Ok(None);
        }
        anyhow::bail!("failed to read {rel:?}: {cause}");
    }
    let Some((kind, target)) = stdout.split_once('\t') else {
        anyhow::bail!("failed to read {rel:?}: unexpected output from the server: {stdout:?}");
    };
    Ok((kind == "l").then(|| target.to_string()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// `root` の下の `rel` に対してコマンドをこのマシンの sh で動かし、出力を読む
    fn run(root: &std::path::Path, rel: &str) -> anyhow::Result<Option<String>> {
        let command = link_target_command(root.join(rel).to_str().unwrap());
        let output = std::process::Command::new("sh")
            .args(["-c", &command])
            .output()
            .unwrap();
        parse_link_target_output(
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
            output.status.code().unwrap() as u32,
            rel,
        )
    }

    fn root_with_entries() -> tempfile::TempDir {
        let root = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(root.path().join("dir")).unwrap();
        std::fs::write(root.path().join("file"), "x").unwrap();
        symlink(" target ", root.path().join("spaced")).unwrap();
        symlink("sub\\b.txt", root.path().join("backslash")).unwrap();
        symlink("line\n", root.path().join("newline")).unwrap();
        symlink("dir", root.path().join("dirlink")).unwrap();
        root
    }

    #[test]
    fn a_symlink_gives_its_link_target_as_read_without_following_it() {
        let root = root_with_entries();
        let target = |rel: &str| run(root.path(), rel).unwrap();
        assert_eq!(target("spaced").as_deref(), Some(" target "));
        assert_eq!(target("backslash").as_deref(), Some("sub\\b.txt"));
        assert_eq!(target("newline").as_deref(), Some("line\n"));
        assert_eq!(target("dirlink").as_deref(), Some("dir"));
    }

    #[test]
    fn an_entry_that_is_not_a_symlink_or_is_absent_has_no_link_target() {
        let root = root_with_entries();
        for rel in [
            "file",
            "dir",
            "absent",
            "absent/link",
            "file/link",
            "dirlink/absent",
        ] {
            assert_eq!(run(root.path(), rel).unwrap(), None, "{rel}");
        }
    }

    #[test]
    fn a_failure_other_than_an_absent_path_is_an_error_with_the_cause() {
        let error = parse_link_target_output(
            "",
            "find: '/r/locked/link': Permission denied\n",
            1,
            "locked/link",
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("locked/link") && error.contains("Permission denied"),
            "{error}"
        );
    }

    #[test]
    fn output_in_an_unexpected_form_is_an_error() {
        assert!(parse_link_target_output("unexpected", "", 0, "link").is_err());
    }
}
