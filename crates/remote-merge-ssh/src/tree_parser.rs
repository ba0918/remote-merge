//! リモートのファイル操作（書き込み・削除・symlink の作成）のシェルコマンドの組み立て。

use std::path::Path;

/// シェル引数をエスケープする
pub fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// リモートパスの親ディレクトリを作成する `mkdir -p` コマンドを構築する。
///
/// 親ディレクトリがない（ルート直下のファイル等）場合は `None` を返す。
pub fn build_mkdir_command(remote_path: &str) -> Option<String> {
    let parent = Path::new(remote_path).parent()?;
    let parent_str = parent.to_string_lossy();
    if parent_str.is_empty() {
        return None;
    }
    Some(format!("mkdir -p {}", shell_escape(&parent_str)))
}

/// ファイルか symlink を消し、ないパスでは 0 以外で終わるコマンドを構築する。
///
/// 存在を確かめてから `rm -f` で消すと、その間に消えた対象を消したと報告するので、確かめずに
/// `-f` のない `rm` で消してないパスを rm の失敗にする。`-f` のない `rm` は書き込めないファイルで
/// 確認を求めるのは標準入力が端末のときだけで、SSH の exec には端末がない。
pub fn build_remove_command(remote_path: &str) -> String {
    format!("rm -- {}", shell_escape(remote_path))
}

/// 親ディレクトリを作ってから symlink を作るコマンドを構築する。
///
/// 作成先に何か（通常ファイル・ディレクトリ・symlink）があれば置き換えずに 0 以外で終わる。
/// 作成先がディレクトリでもその中にリンクを作らない GNU の `ln -T` で作り、それが使えないとき
/// だけ作成先に何もないことを確かめてから `ln` で作る。リンク先は読み込み元の symlink の文字列
/// なので、`-` で始まってもオプションとして読ませないよう前に `--` を置く。
pub fn build_create_symlink_command(target: &str, remote_path: &str) -> String {
    let t = shell_escape(target);
    let create = format!(
        "p={q}; if ! e=$(ln -s -T -- {t} \"$p\" 2>&1); then \
         if [ -e \"$p\" ] || [ -L \"$p\" ]; then printf '%s\\n' \"$e\" >&2; exit 1; fi; \
         ln -s -- {t} \"$p\"; fi",
        q = shell_escape(remote_path)
    );
    match build_mkdir_command(remote_path) {
        Some(mkdir) => format!("{mkdir} || exit 1; {create}"),
        None => create,
    }
}

/// 標準入力の base64 をデコードして書き込むコマンドを構築する（[`write_in_place`]）。
///
/// `-A` は改行なしの1行 base64 を受け付けるモード（Rust の base64::STANDARD.encode は改行を
/// 挿入しないため必須。`-A` なしだと内部バッファ 80 文字で切り捨てられる）。
pub fn build_write_bytes_command(remote_path: &str) -> String {
    write_in_place(remote_path, "openssl base64 -d -A")
}

/// 標準入力をそのまま書き込むコマンドを構築する（[`write_in_place`]）。
pub fn build_write_text_command(remote_path: &str) -> String {
    write_in_place(remote_path, "cat")
}

/// 標準入力を `writer` に通した中身を、書き込み先のファイルをその場で開いて書くコマンド。
///
/// シェルのリダイレクトには O_NOFOLLOW がないので、書き込み先は dd の `oflag=nofollow` で開き、
/// 最後の要素が symlink なら開かずに失敗する。一時ファイルを作って名前を付け替えると、所有者と
/// ハードリンクが変わり、予測できる一時ファイルの名前を他のユーザーに差し替えられる。
///
/// - `writer` のコマンドがリモートにないときは、書き込み先を開く（切り詰める）前に失敗する
/// - POSIX の sh には pipefail がないので、`writer` の終了ステータスは fd 3 からコマンド置換で
///   取り出し、パイプの終了ステータス（dd のもの）と合わせて確かめる
/// - dd が `oflag=nofollow` を受け付けないときだけ、直前に symlink でないことを確かめてから
///   リダイレクトで書く。確かめた後に symlink に替えられる競合はこの場合だけ残る
/// - 転送量の表示を消す `status=none` は、受け付けない古い dd（coreutils 8.20 より前）には
///   付けない。そのときの表示は誰も読まない標準エラーに出る
/// - 失敗で終わるときも標準入力を読み切るのは、送り手のデータ送信を途中で切らず、終了コードで
///   失敗を返すため
fn write_in_place(remote_path: &str, writer: &str) -> String {
    let q = shell_escape(remote_path);
    let program = writer.split(' ').next().unwrap_or(writer);
    format!(
        "p={q}; fail() {{ cat >/dev/null; exit 1; }}; \
         command -v {program} >/dev/null 2>&1 || fail; \
         if dd if=/dev/null of=/dev/null oflag=nofollow 2>/dev/null; then \
         q=; if dd if=/dev/null of=/dev/null status=none 2>/dev/null; then q=status=none; fi; \
         s=$( {{ {{ {writer} 3>&-; echo $? >&3; }} | dd of=\"$p\" bs=65536 oflag=nofollow $q 3>&-; }} 3>&1 ) || fail; \
         [ \"$s\" = 0 ] || fail; \
         else \
         if [ -L \"$p\" ]; then fail; fi; \
         {writer} >\"$p\" || fail; \
         fi"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TAB で区切って書いた列（足りない列は空）を、find が出す一つの項目にする
    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("/var/www/app"), "'/var/www/app'");
        assert_eq!(shell_escape("it's a test"), "'it'\\''s a test'");
    }

    #[test]
    fn test_shell_escape_special_chars() {
        assert_eq!(
            shell_escape("/path/to/my file.txt"),
            "'/path/to/my file.txt'"
        );
        assert_eq!(shell_escape("/path;rm -rf /"), "'/path;rm -rf /'");
        assert_eq!(shell_escape("/path/\"quoted\""), "'/path/\"quoted\"'");
    }

    /// 生成したコマンドを実際の sh で実行し、成功したかを返す
    fn sh_succeeds(command: &str) -> bool {
        std::process::Command::new("sh")
            .args(["-c", command])
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
            .status
            .success()
    }

    #[cfg(unix)]
    #[test]
    fn removing_deletes_a_file_or_a_dangling_link_and_fails_on_a_missing_path() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        let dangling = dir.path().join("dangling");
        symlink(dir.path().join("nowhere"), &dangling).unwrap();
        let target = dir.path().join("target.txt");
        std::fs::write(&target, "kept").unwrap();
        let link = dir.path().join("link");
        symlink(&target, &link).unwrap();
        let missing = dir.path().join("missing.txt");
        let read_only = dir.path().join("read-only.txt");
        std::fs::write(&read_only, "x").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&read_only, std::fs::Permissions::from_mode(0o444)).unwrap();
        }

        assert!(sh_succeeds(&build_remove_command(file.to_str().unwrap())));
        assert!(sh_succeeds(&build_remove_command(
            read_only.to_str().unwrap()
        )));
        assert!(sh_succeeds(&build_remove_command(
            dangling.to_str().unwrap()
        )));
        assert!(sh_succeeds(&build_remove_command(link.to_str().unwrap())));
        assert!(!sh_succeeds(&build_remove_command(
            missing.to_str().unwrap()
        )));

        assert!(file.symlink_metadata().is_err());
        assert!(read_only.symlink_metadata().is_err());
        assert!(dangling.symlink_metadata().is_err());
        assert!(link.symlink_metadata().is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "kept");
        assert!(missing.symlink_metadata().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn creating_a_link_makes_its_missing_parent() {
        let dir = tempfile::TempDir::new().unwrap();
        let link = dir.path().join("new/dir/link");

        assert!(sh_succeeds(&build_create_symlink_command(
            "x",
            link.to_str().unwrap()
        )));

        assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("x"));
    }

    #[cfg(unix)]
    #[test]
    fn creating_a_link_whose_target_starts_with_a_dash_uses_it_as_the_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let link = dir.path().join("link");
        let target = format!("--target-directory={}", sub.display());

        assert!(sh_succeeds(&build_create_symlink_command(
            &target,
            link.to_str().unwrap()
        )));

        assert_eq!(std::fs::read_link(&link).unwrap(), Path::new(&target));
        assert_eq!(std::fs::read_dir(&sub).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn creating_a_link_where_a_link_or_a_file_exists_fails_without_change() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::TempDir::new().unwrap();
        let old_target = dir.path().join("old.txt");
        std::fs::write(&old_target, "old").unwrap();
        let link = dir.path().join("link");
        symlink(&old_target, &link).unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();

        assert!(!sh_succeeds(&build_create_symlink_command(
            "new.txt",
            link.to_str().unwrap()
        )));
        assert!(!sh_succeeds(&build_create_symlink_command(
            "new.txt",
            file.to_str().unwrap()
        )));

        assert_eq!(std::fs::read_link(&link).unwrap(), old_target);
        assert_eq!(std::fs::read_to_string(&old_target).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "x");
    }

    #[cfg(unix)]
    #[test]
    fn creating_a_link_where_a_directory_or_a_file_parent_is_fails_without_change() {
        let dir = tempfile::TempDir::new().unwrap();
        let existing_dir = dir.path().join("existing");
        std::fs::create_dir(&existing_dir).unwrap();
        let file_parent = dir.path().join("file.txt");
        std::fs::write(&file_parent, "x").unwrap();

        assert!(!sh_succeeds(&build_create_symlink_command(
            "x",
            existing_dir.to_str().unwrap()
        )));
        assert!(!sh_succeeds(&build_create_symlink_command(
            "x",
            file_parent.join("link").to_str().unwrap()
        )));

        assert_eq!(std::fs::read_dir(&existing_dir).unwrap().count(), 0);
        assert_eq!(std::fs::read_to_string(&file_parent).unwrap(), "x");
    }

    #[test]
    fn test_build_mkdir_command_no_parent() {
        // 親がないパスは None
        let cmd = build_mkdir_command("file.rs");
        assert!(cmd.is_none());
    }
    /// 書き込みのテストを実行するログインシェル。入っていないシェルは飛ばす。
    fn installed_login_shells() -> Vec<&'static str> {
        crate::login_shell::LOGIN_SHELLS
            .into_iter()
            .filter(|shell| {
                let installed = std::process::Command::new(shell)
                    .args(["-c", "true"])
                    .status()
                    .is_ok();
                if !installed {
                    crate::login_shell::skip_missing_shell(shell);
                }
                installed
            })
            .collect()
    }

    /// 書き込みのコマンドを、リモートと同じく `sh -c` に包んでログインシェル `shell` から実行する
    /// プロセス
    fn write_process(shell: &str, command: &str) -> std::process::Command {
        let mut process = std::process::Command::new(shell);
        process
            .args(["-c", &crate::login_shell::wrap_for_login_shell(command)])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        process
    }

    /// 書き込みのコマンドを `shell` から実行する。`fake_bin` があれば PATH の先頭に置く。
    fn spawn_write(shell: &str, command: &str, fake_bin: Option<&Path>) -> std::process::Child {
        let mut process = write_process(shell, command);
        if let Some(bin) = fake_bin {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let paths = std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&path));
            process.env("PATH", std::env::join_paths(paths).unwrap());
        }
        process.spawn().unwrap()
    }

    /// 起動した書き込みの標準入力に `input` を渡して、成功したかを返す
    fn finish_write(mut child: std::process::Child, input: &[u8]) -> bool {
        use std::io::Write;
        // 失敗するコマンドが先に終わると書き込みが EPIPE になるが、確かめるのは終了ステータス
        let _ = child.stdin.take().unwrap().write_all(input);
        child.wait().unwrap().success()
    }

    /// 書き込みのコマンドを `shell` から実行し、標準入力に `input` を渡して、成功したかを返す
    fn write_under(shell: &str, command: &str, input: &[u8], fake_bin: Option<&Path>) -> bool {
        finish_write(spawn_write(shell, command, fake_bin), input)
    }

    /// [`write_under`] と同じだが、PATH を `bin` だけにする。fish が利用者の設定で PATH に
    /// ディレクトリを足さないよう、設定の場所も `bin` の中の空のディレクトリにする。
    fn write_with_only(shell: &str, command: &str, input: &[u8], bin: &Path) -> bool {
        let mut process = write_process(shell, command);
        process
            .env("PATH", bin)
            .env("XDG_CONFIG_HOME", bin.join("config"));
        finish_write(process.spawn().unwrap(), input)
    }

    /// 今の PATH で `name` のコマンドが見つかる場所
    fn resolve_command(name: &str) -> Option<String> {
        let output = std::process::Command::new("sh")
            .args(["-c", "command -v \"$1\"", "_", name])
            .output()
            .unwrap();
        let found = String::from_utf8(output.stdout).unwrap();
        (output.status.success() && !found.trim().is_empty()).then(|| found.trim().to_string())
    }

    /// `dir` に `body` を中身とする実行できるスクリプト `name` を置く
    #[cfg(unix)]
    fn put_script(dir: &Path, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// `oflag=nofollow` を受け付けない dd の代わりに、いつも失敗する `dd` を置いたディレクトリ
    #[cfg(unix)]
    fn bin_without_dd_nofollow() -> tempfile::TempDir {
        let bin = tempfile::TempDir::new().unwrap();
        put_script(bin.path(), "dd", "#!/bin/sh\nexit 1\n");
        bin
    }

    /// openssl のないリモートの代わりに、sh・cat・ログインシェルと dd だけを置いたディレクトリ。
    /// `dd_nofollow` が false なら dd は `oflag=nofollow` を受け付けないいつも失敗するものにする。
    #[cfg(unix)]
    fn bin_without_openssl(dd_nofollow: bool) -> tempfile::TempDir {
        let bin = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(bin.path().join("config")).unwrap();
        let mut names = vec!["sh", "cat"];
        names.extend(crate::login_shell::LOGIN_SHELLS);
        if dd_nofollow {
            names.push("dd");
        } else {
            put_script(bin.path(), "dd", "#!/bin/sh\nexit 1\n");
        }
        for name in names {
            let link = bin.path().join(name);
            if let (Some(real), false) = (resolve_command(name), link.exists()) {
                std::os::unix::fs::symlink(real, link).unwrap();
            }
        }
        bin
    }

    /// `real.txt` を指す symlink `link.txt` を置いた一時ディレクトリ
    #[cfg(unix)]
    fn dir_with_link_to_real_file() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("real.txt"), "original").unwrap();
        std::os::unix::fs::symlink("real.txt", dir.path().join("link.txt")).unwrap();
        dir
    }

    #[cfg(unix)]
    fn assert_link_and_target_unchanged(dir: &tempfile::TempDir) {
        assert_eq!(
            std::fs::read_link(dir.path().join("link.txt")).unwrap(),
            Path::new("real.txt")
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("real.txt")).unwrap(),
            "original"
        );
    }

    /// ディレクトリにある項目の名前（並べ替え済み）
    fn entry_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[cfg(unix)]
    #[test]
    fn binary_write_refuses_a_symlink_destination() {
        for shell in installed_login_shells() {
            let dir = dir_with_link_to_real_file();
            let link = dir.path().join("link.txt");

            let written = write_under(
                shell,
                &build_write_bytes_command(link.to_str().unwrap()),
                b"bmV3\n",
                None,
            );

            assert!(!written, "{shell}");
            assert_link_and_target_unchanged(&dir);
        }
    }

    #[cfg(unix)]
    #[test]
    fn text_write_refuses_a_symlink_destination() {
        for shell in installed_login_shells() {
            let dir = dir_with_link_to_real_file();
            let link = dir.path().join("link.txt");

            let written = write_under(
                shell,
                &build_write_text_command(link.to_str().unwrap()),
                b"new",
                None,
            );

            assert!(!written, "{shell}");
            assert_link_and_target_unchanged(&dir);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_write_onto_a_dangling_link_fails_without_creating_its_target() {
        for shell in installed_login_shells() {
            let dir = tempfile::TempDir::new().unwrap();
            let link = dir.path().join("link.txt");
            std::os::unix::fs::symlink("missing.txt", &link).unwrap();

            let written = write_under(
                shell,
                &build_write_text_command(link.to_str().unwrap()),
                b"new",
                None,
            );

            assert!(!written, "{shell}");
            assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("missing.txt"));
            assert_eq!(entry_names(dir.path()), ["link.txt"], "{shell}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_write_keeps_owner_mode_and_hard_links_and_truncates_old_content() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        type BuildWrite = fn(&str) -> String;
        let cases: [(BuildWrite, &[u8]); 2] = [
            (build_write_text_command, b"new"),
            (build_write_bytes_command, b"bmV3\n"),
        ];
        for shell in installed_login_shells() {
            for (build, input) in cases {
                let dir = tempfile::TempDir::new().unwrap();
                let dest = dir.path().join("run.sh");
                std::fs::write(&dest, "much longer old content").unwrap();
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o751)).unwrap();
                let other_name = dir.path().join("other-name.sh");
                std::fs::hard_link(&dest, &other_name).unwrap();
                let before = std::fs::metadata(&dest).unwrap();

                assert!(
                    write_under(shell, &build(dest.to_str().unwrap()), input, None),
                    "{shell}"
                );

                let after = std::fs::metadata(&dest).unwrap();
                assert_eq!(std::fs::read(&dest).unwrap(), b"new", "{shell}");
                assert_eq!(std::fs::read(&other_name).unwrap(), b"new", "{shell}");
                assert_eq!(after.ino(), before.ino(), "{shell}");
                assert_eq!(after.nlink(), 2, "{shell}");
                assert_eq!(after.uid(), before.uid(), "{shell}");
                assert_eq!(after.gid(), before.gid(), "{shell}");
                assert_eq!(after.mode() & 0o7777, 0o751, "{shell}");
            }
        }
    }

    #[test]
    fn writes_create_a_missing_file_and_leave_no_other_entry() {
        for shell in installed_login_shells() {
            let dir = tempfile::TempDir::new().unwrap();
            let text = dir.path().join("missing.txt");
            let binary = dir.path().join("missing.bin");

            assert!(
                write_under(
                    shell,
                    &build_write_text_command(text.to_str().unwrap()),
                    b"new text",
                    None
                ),
                "{shell}"
            );
            assert!(
                write_under(
                    shell,
                    &build_write_bytes_command(binary.to_str().unwrap()),
                    b"AP8K\n",
                    None
                ),
                "{shell}"
            );

            assert_eq!(std::fs::read(&text).unwrap(), b"new text");
            assert_eq!(std::fs::read(&binary).unwrap(), [0x00, 0xff, b'\n']);
            assert_eq!(
                entry_names(dir.path()),
                ["missing.bin", "missing.txt"],
                "{shell}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_swapped_in_during_a_write_keeps_its_target_unchanged() {
        use std::io::Write;
        for shell in installed_login_shells() {
            let dir = tempfile::TempDir::new().unwrap();
            let dest = dir.path().join("dest.txt");
            std::fs::write(&dest, "old").unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let outside_file = outside.path().join("outside.txt");
            std::fs::write(&outside_file, "outside").unwrap();

            let mut child = spawn_write(
                shell,
                &build_write_text_command(dest.to_str().unwrap()),
                None,
            );
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(b"ne").unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::fs::read(&dest).unwrap() != b"ne" {
                assert!(
                    std::time::Instant::now() < deadline,
                    "{shell}: the write did not start"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::fs::remove_file(&dest).unwrap();
            std::os::unix::fs::symlink(&outside_file, &dest).unwrap();
            stdin.write_all(b"w").unwrap();
            drop(stdin);
            child.wait().unwrap();

            assert_eq!(
                std::fs::read_to_string(&outside_file).unwrap(),
                "outside",
                "{shell}"
            );
            assert_eq!(std::fs::read_link(&dest).unwrap(), outside_file);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_write_onto_a_directory_fails_without_change() {
        for shell in installed_login_shells() {
            let dir = tempfile::TempDir::new().unwrap();
            let dest = dir.path().join("folder");
            std::fs::create_dir(&dest).unwrap();

            assert!(
                !write_under(
                    shell,
                    &build_write_text_command(dest.to_str().unwrap()),
                    b"new",
                    None
                ),
                "{shell}"
            );

            assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);
            assert_eq!(entry_names(dir.path()), ["folder"], "{shell}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn without_dd_nofollow_a_link_destination_is_refused_and_a_file_is_written() {
        use std::os::unix::fs::MetadataExt;
        let bin = bin_without_dd_nofollow();
        for shell in installed_login_shells() {
            let dir = dir_with_link_to_real_file();
            let link = dir.path().join("link.txt");
            let dest = dir.path().join("dest.txt");
            std::fs::write(&dest, "much longer old content").unwrap();
            std::fs::hard_link(&dest, dir.path().join("other-name.txt")).unwrap();

            let refused = !write_under(
                shell,
                &build_write_bytes_command(link.to_str().unwrap()),
                b"bmV3\n",
                Some(bin.path()),
            );
            let written = write_under(
                shell,
                &build_write_text_command(dest.to_str().unwrap()),
                b"new",
                Some(bin.path()),
            );

            assert!(refused, "{shell}");
            assert_link_and_target_unchanged(&dir);
            assert!(written, "{shell}");
            assert_eq!(std::fs::read(&dest).unwrap(), b"new", "{shell}");
            assert_eq!(std::fs::metadata(&dest).unwrap().nlink(), 2, "{shell}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_decoder_that_fails_after_writing_part_fails_the_write() {
        // デコードの途中で失敗する openssl の代わりに、少し書いてから失敗する `openssl`
        let bin = tempfile::TempDir::new().unwrap();
        put_script(
            bin.path(),
            "openssl",
            "#!/bin/sh\ncat >/dev/null\nprintf partial\nexit 1\n",
        );
        for shell in installed_login_shells() {
            let dir = tempfile::TempDir::new().unwrap();
            let dest = dir.path().join("dest.bin");
            std::fs::write(&dest, "original").unwrap();

            let written = write_under(
                shell,
                &build_write_bytes_command(dest.to_str().unwrap()),
                b"bmV3\n",
                Some(bin.path()),
            );

            assert!(!written, "{shell}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_write_without_openssl_fails_and_keeps_the_destination() {
        for dd_nofollow in [true, false] {
            let bin = bin_without_openssl(dd_nofollow);
            for shell in installed_login_shells() {
                let dir = tempfile::TempDir::new().unwrap();
                let dest = dir.path().join("dest.bin");
                let text = dir.path().join("text.txt");
                std::fs::write(&dest, "original").unwrap();

                let written = write_with_only(
                    shell,
                    &build_write_bytes_command(dest.to_str().unwrap()),
                    b"bmV3\n",
                    bin.path(),
                );
                // 同じ PATH でテキストは書けるので、失敗の原因は openssl がないこと
                let text_written = write_with_only(
                    shell,
                    &build_write_text_command(text.to_str().unwrap()),
                    b"new",
                    bin.path(),
                );

                let case = format!("{shell}, dd_nofollow={dd_nofollow}");
                assert!(!written, "{case}");
                assert_eq!(std::fs::read(&dest).unwrap(), b"original", "{case}");
                assert!(text_written, "{case}");
                assert_eq!(std::fs::read(&text).unwrap(), b"new", "{case}");
            }
        }
    }

    /// `status=none` を受け付けない古い dd（CentOS 5 の coreutils 5.97 など）の代わりに、それを
    /// 拒み、他は実際の dd に渡して引数を `dd.log` に残す `dd` を置いたディレクトリ
    #[cfg(unix)]
    fn bin_with_dd_lacking_status_none() -> tempfile::TempDir {
        let real_dd = resolve_command("dd").unwrap();
        let bin = tempfile::TempDir::new().unwrap();
        let log = bin.path().join("dd.log");
        put_script(
            bin.path(),
            "dd",
            &format!(
                "#!/bin/sh\nfor a do case $a in status=*) exit 1;; esac; done\n\
                 printf '%s\\n' \"$*\" >>'{}'\nexec {} \"$@\"\n",
                log.display(),
                real_dd
            ),
        );
        bin
    }

    #[cfg(unix)]
    #[test]
    fn a_dd_without_status_none_still_opens_the_destination_without_following_a_link() {
        let bin = bin_with_dd_lacking_status_none();
        for shell in installed_login_shells() {
            let dir = dir_with_link_to_real_file();
            let link = dir.path().join("link.txt");
            let dest = dir.path().join("dest.txt");
            std::fs::write(&dest, "much longer old content").unwrap();
            let _ = std::fs::remove_file(bin.path().join("dd.log"));

            let refused = !write_under(
                shell,
                &build_write_text_command(link.to_str().unwrap()),
                b"new",
                Some(bin.path()),
            );
            let written = write_under(
                shell,
                &build_write_text_command(dest.to_str().unwrap()),
                b"new",
                Some(bin.path()),
            );

            assert!(refused, "{shell}");
            assert_link_and_target_unchanged(&dir);
            assert!(written, "{shell}");
            assert_eq!(std::fs::read(&dest).unwrap(), b"new", "{shell}");
            let log = std::fs::read_to_string(bin.path().join("dd.log")).unwrap_or_default();
            let opened_with_nofollow = |path: &Path| {
                log.lines().any(|line| {
                    line.contains(&format!("of={}", path.display()))
                        && line.contains("oflag=nofollow")
                })
            };
            assert!(opened_with_nofollow(&link), "{shell}: {log}");
            assert!(opened_with_nofollow(&dest), "{shell}: {log}");
        }
    }
}
