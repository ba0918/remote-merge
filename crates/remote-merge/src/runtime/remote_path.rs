use std::path::PathBuf;

use super::core::CoreRuntime;
use super::target_io::TargetPath;

/// 検査のスクリプトが使う関数。ローカルの検査（`inspect_local_path`）と同じ規則で、
/// lstat が ENOENT で失敗したパスだけを missing にし、それ以外の失敗は終了コード 1 にする。
///
/// - `resolve_existing_prefix PATH`: PATH の祖先のうち lstat できる最も近いものを辿った実パスに、
///   その下の存在しない要素（PATH の末尾の要素は除く）を足して出す。その祖先が辿った先で検索できる
///   ディレクトリでなければ、PATH の lstat は ENOENT ではないので失敗する
/// - `canonical_is_missing PATH`: PATH の実パスへの解決が ENOENT で失敗するときだけ成功する。
///   symlink を辿る回数はカーネルと同じ 40 回までで、超えれば ELOOP として失敗する
/// - `parent_of`・`base_of`: 末尾の "/" を除いた親（`s`）と末尾の要素（`b`）
///
/// 試験 SSH サーバは先頭の `resolve_existing_prefix()` と、後に続く `; p=<パス>; if [` で
/// この検査を見分けるため、その並びを崩さない。
const INSPECT_PATH_FUNCTIONS: &str = r#"resolve_existing_prefix() {
  q=$1; suffix=;
  while :; do
    parent_of "$q"; [ "$s" != "$q" ] || return 1; q=$s;
    if [ -e "$q" ] || [ -L "$q" ]; then break; fi;
    base_of "$q"; suffix=/$b$suffix;
  done;
  [ -d "$q" ] && [ -x "$q" ] || return 1;
  r=$(readlink -f -- "$q") || return 1;
  case $r in /) printf '%s\n' "${suffix:-/}" ;; *) printf '%s%s\n' "$r" "$suffix" ;; esac;
};
parent_of() {
  s=$1;
  while [ "${s%/}" != "$s" ] && [ -n "${s%/}" ]; do s=${s%/}; done;
  case $s in */*) s=${s%/*}; [ -n "$s" ] || s=/ ;; *) s=. ;; esac;
};
base_of() {
  b=$1;
  while [ "${b%/}" != "$b" ] && [ -n "${b%/}" ]; do b=${b%/}; done;
  b=${b##*/};
};
canonical_is_missing() {
  x=$1; n=0;
  while [ "$n" -lt 40 ]; do
    n=$((n + 1)); y=$x;
    while ! { [ -e "$y" ] || [ -L "$y" ]; }; do parent_of "$y"; [ "$s" != "$y" ] || return 1; y=$s; done;
    if [ "$y" != "$x" ] && [ -d "$y" ] && [ -x "$y" ]; then return 0; fi;
    [ -L "$y" ] && ! [ -e "$y" ] || return 1;
    t=$(readlink -- "$y") || return 1;
    case $t in /*) ;; *) parent_of "$y"; t=$s/$t ;; esac;
    x=$t${x#"$y"};
  done;
  return 1;
}"#;

/// 本体。`p` の lstat の結果で分け、symlink はローカルの `resolved_symlink` と同じくリンク先を
/// 解決し、解決が ENOENT で失敗したときだけ存在する祖先からの形にする。
const INSPECT_PATH_BODY: &str = r#"if [ -L "$p" ]; then
  target=$(readlink -- "$p") || exit 1;
  case $target in /*) c=$target ;; *) parent_of "$p"; c=$s/$target ;; esac;
  if [ -e "$c" ]; then resolved=$(readlink -f -- "$c") || exit 1;
  else
    canonical_is_missing "$c" || exit 1;
    base_of "$c"; case $b in ''|..) exit 1 ;; esac;
    prefix=$(resolve_existing_prefix "$c") || exit 1;
    case $prefix in /) resolved=/$b ;; *) resolved=$prefix/$b ;; esac;
  fi;
  printf 'symlink\n%s\n%s\n' "$target" "$resolved";
elif [ -e "$p" ]; then printf 'file\n'; readlink -f -- "$p";
else resolved=$(resolve_existing_prefix "$p") || exit 1; printf 'missing\n%s\n' "$resolved";
fi"#;

/// 一行のスクリプトにする。各行は `;` かキーワードで終わるので、改行を空白に置き換えてよい
fn one_line(script: &str) -> String {
    script.lines().map(str::trim).collect::<Vec<_>>().join(" ")
}

pub(crate) fn build_inspect_path_command(path: &str) -> String {
    let path = crate::ssh::tree_parser::shell_escape(path);
    format!(
        "{}; p={path}; {}",
        one_line(INSPECT_PATH_FUNCTIONS),
        one_line(INSPECT_PATH_BODY)
    )
}

/// 辿った先が通常ファイルかを問うコマンド。失敗を終了コードにしないよう、答えを出力で返す
pub(crate) fn build_is_regular_file_command(path: &str) -> String {
    let path = crate::ssh::tree_parser::shell_escape(path);
    format!("if [ -f {path} ]; then echo regular; else echo other; fi")
}

pub(crate) fn parse_is_regular_file_output(output: &str) -> anyhow::Result<bool> {
    match output.trim_end_matches(['\r', '\n']) {
        "regular" => Ok(true),
        "other" => Ok(false),
        other => anyhow::bail!("unknown file kind output: {other}"),
    }
}

impl CoreRuntime {
    /// SSH の経路で、root_dir からの `rel_path` が辿った先で通常ファイルかを問う
    pub(crate) fn remote_is_regular_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> anyhow::Result<bool> {
        let root = self
            .get_server_config(server_name)?
            .root_dir
            .to_string_lossy()
            .to_string();
        let full_path = crate::merge::executor::validate_remote_path(&root, rel_path)?;
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {server_name}"))?;
        let output = self.rt.block_on(crate::ssh::exec_strict(
            client,
            &build_is_regular_file_command(&full_path),
        ))?;
        parse_is_regular_file_output(&output)
    }
}

pub(crate) fn parse_inspect_path_output(output: &str) -> anyhow::Result<TargetPath> {
    let (kind, value) = output
        .split_once('\n')
        .ok_or_else(|| anyhow::anyhow!("invalid path inspection output"))?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() {
        anyhow::bail!("path inspection returned an empty path");
    }
    match kind.trim_end_matches('\r') {
        "file" => Ok(TargetPath::File {
            real_path: PathBuf::from(value),
        }),
        "symlink" => {
            let (link_target, real_path) = value
                .split_once('\n')
                .ok_or_else(|| anyhow::anyhow!("symlink inspection omitted resolved path"))?;
            Ok(TargetPath::Symlink {
                link_target: PathBuf::from(link_target),
                real_path: PathBuf::from(real_path),
            })
        }
        "missing" => Ok(TargetPath::Missing {
            real_parent: PathBuf::from(value),
        }),
        other => anyhow::bail!("unknown path inspection kind: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(unix)]
    use std::process::Command;

    #[test]
    fn command_quotes_the_remote_path() {
        let command = build_inspect_path_command("/var/www/a file's.txt");
        assert!(command.contains("'/var/www/a file'\\''s.txt'"));
        assert!(command.contains("readlink -f"));
    }

    #[test]
    fn parser_returns_a_file_real_path() {
        assert_eq!(
            parse_inspect_path_output("file\n/mnt/shared/file.txt\n").unwrap(),
            TargetPath::File {
                real_path: PathBuf::from("/mnt/shared/file.txt")
            }
        );
    }

    #[test]
    fn parser_returns_a_symlink_target_and_its_resolved_path() {
        assert_eq!(
            parse_inspect_path_output("symlink\n../shared/file.txt\n/mnt/shared/file.txt\n")
                .unwrap(),
            TargetPath::Symlink {
                link_target: PathBuf::from("../shared/file.txt"),
                real_path: PathBuf::from("/mnt/shared/file.txt")
            }
        );
    }

    /// 検査のスクリプトを `sh` で実行し、出力を読んだ結果を返す。終了コードが 0 以外ならエラー
    #[cfg(unix)]
    fn inspect_with_script(path: &std::path::Path) -> anyhow::Result<TargetPath> {
        let output = Command::new("sh")
            .arg("-c")
            .arg(build_inspect_path_command(path.to_str().unwrap()))
            .output()?;
        if !output.status.success() {
            anyhow::bail!("exit status {}", output.status);
        }
        parse_inspect_path_output(&String::from_utf8(output.stdout)?)
    }

    #[cfg(unix)]
    #[test]
    fn script_and_local_inspection_agree_on_every_path_state() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::create_dir(root.join("dir")).unwrap();
        std::fs::write(root.join("file.txt"), "file\n").unwrap();
        std::fs::create_dir(root.join("locked")).unwrap();
        std::fs::write(root.join("locked/inner.txt"), "inner\n").unwrap();
        symlink("self", root.join("self")).unwrap();
        symlink("missing/file.txt", root.join("dangling")).unwrap();
        symlink("dangling", root.join("chain")).unwrap();
        symlink("nowhere", root.join("broken")).unwrap();
        symlink("loop", root.join("loop")).unwrap();
        symlink("dir", root.join("linkdir")).unwrap();
        symlink("file.txt", root.join("linkfile")).unwrap();
        std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o600))
            .unwrap();
        // 権限を無視できる実効ユーザーでは検索権限のない状態を作れない
        let can_lock = std::fs::symlink_metadata(root.join("locked/inner.txt")).is_err();

        let mut cases = vec![
            "file.txt",
            "dir",
            "dir/missing.txt",
            "nodir/sub/missing.txt",
            "file.txt/below.txt",
            "self",
            "dangling",
            "chain",
            "broken/below.txt",
            "loop/below.txt",
            "linkdir/missing.txt",
            "linkfile",
        ];
        if can_lock {
            cases.push("locked/inner.txt");
        }
        let mismatches: Vec<String> = cases
            .into_iter()
            .filter_map(|rel| {
                let local = remote_merge_engine::local_io::inspect_local_path(root, rel);
                let script = inspect_with_script(&root.join(rel));
                match (&local, &script) {
                    (Ok(local), Ok(script)) if local == script => None,
                    (Err(_), Err(_)) => None,
                    _ => Some(format!("{rel}: local {local:?}, script {script:?}")),
                }
            })
            .collect();
        std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
        assert!(mismatches.is_empty(), "{mismatches:#?}");
    }

    #[cfg(unix)]
    #[test]
    fn command_resolves_a_dangling_terminal_symlink_to_the_path_it_names() {
        let directory = tempfile::tempdir().unwrap();
        let link = directory.path().join("current");
        symlink("missing/file.txt", &link).unwrap();

        let output = Command::new("sh")
            .arg("-c")
            .arg(build_inspect_path_command(link.to_str().unwrap()))
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "path inspection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            parse_inspect_path_output(&String::from_utf8(output.stdout).unwrap()).unwrap(),
            TargetPath::Symlink {
                link_target: PathBuf::from("missing/file.txt"),
                real_path: directory.path().join("missing/file.txt"),
            }
        );
    }
}
