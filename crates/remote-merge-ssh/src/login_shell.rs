//! リモートのログインシェルの種類によらずコマンドを実行するための包み方。

/// `script` を、リモートのログインシェルから同じ文字列のまま `sh` に渡る `sh -c <引数>` にする。
///
/// SSH の exec はコマンドをリモートのログインシェルに渡すので、fish のように POSIX の sh と文法の
/// 違うシェルでは Bourne の構文がそのままでは動かない。引数は単一引用符で囲み、単一引用符と
/// バックスラッシュだけは引用符を閉じてから前にバックスラッシュを置く。fish は単一引用符の中でも
/// `\'` と `\\` を解釈するので、POSIX の `'\''` の書き方だけでは引用符の中にバックスラッシュが残る。
pub(crate) fn wrap_for_login_shell(script: &str) -> String {
    let mut command = String::with_capacity(script.len() + 8);
    command.push_str("sh -c '");
    for ch in script.chars() {
        if ch == '\'' || ch == '\\' {
            command.push('\'');
            command.push('\\');
            command.push(ch);
            command.push('\'');
        } else {
            command.push(ch);
        }
    }
    command.push('\'');
    command
}

/// テストでリモートのログインシェルの代わりに使うシェル。
#[cfg(test)]
pub(crate) const LOGIN_SHELLS: [&str; 4] = ["sh", "bash", "dash", "fish"];

/// テストで `shell` が入っていないことを知らせ、そのシェルでの確認を飛ばす。
///
/// `REMOTE_MERGE_TEST_REQUIRE_FISH` が空でなければ、fish がないことを失敗にする（CI は fish を
/// 入れてこの変数を置き、fish での確認が黙って抜けないようにする）。
#[cfg(test)]
pub(crate) fn skip_missing_shell(shell: &str) {
    let fish_required =
        std::env::var_os("REMOTE_MERGE_TEST_REQUIRE_FISH").is_some_and(|value| !value.is_empty());
    assert!(
        !(shell == "fish" && fish_required),
        "fish is required by REMOTE_MERGE_TEST_REQUIRE_FISH but is not installed"
    );
    eprintln!("note: {shell} is not installed; skipped running under it");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `shell -c command` を実行して標準出力を返す。シェルが入っていなければ `None`。
    fn run_with(shell: &str, command: &str, dir: &std::path::Path) -> Option<std::process::Output> {
        std::process::Command::new(shell)
            .args(["-c", command])
            .current_dir(dir)
            .stdin(std::process::Stdio::null())
            .output()
            .ok()
    }

    /// 包んだコマンドを各ログインシェルで実行した結果が、包む前のスクリプトを sh で実行した結果と同じか確かめる
    fn assert_every_login_shell_runs(script: &str, dir: &std::path::Path) {
        let expected = run_with("sh", script, dir).expect("sh is required");
        assert!(expected.status.success(), "{script}: {expected:?}");
        let wrapped = wrap_for_login_shell(script);
        for shell in LOGIN_SHELLS {
            let Some(output) = run_with(shell, &wrapped, dir) else {
                skip_missing_shell(shell);
                continue;
            };
            assert_eq!(
                output.status.code(),
                expected.status.code(),
                "{shell}: {output:?}"
            );
            assert_eq!(output.stdout, expected.stdout, "{shell}: {output:?}");
        }
    }

    #[test]
    fn a_bourne_script_runs_unchanged_under_every_login_shell() {
        let dir = tempfile::TempDir::new().unwrap();
        let scripts = [
            "if [ -d . ]; then echo yes; fi; x=$((1 + 2)); echo \"$x\"",
            r#"printf '%s\n' 'it'\''s' "a\\b" 'tail\' "\$HOME" '!bang' '*'"#,
            "printf '%s|' 'line one\nline two' \"tab\there\"",
            "for p do printf '%s\\n' \"$p\"; done",
        ];
        for script in scripts {
            assert_every_login_shell_runs(script, dir.path());
        }
    }

    #[test]
    fn the_scan_command_runs_unchanged_under_every_login_shell() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("it's")).unwrap();
        std::fs::write(dir.path().join("it's/a\\b.txt"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("it's", dir.path().join("link")).unwrap();
        let script = crate::tree_parser::build_find_command(dir.path().to_str().unwrap(), &[]);
        assert_every_login_shell_runs(&script, dir.path());
    }
}
