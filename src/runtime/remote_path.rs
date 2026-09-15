use std::path::PathBuf;

use super::target_io::TargetPath;

pub(crate) fn build_inspect_path_command(path: &str) -> String {
    let path = crate::ssh::tree_parser::shell_escape(path);
    format!(
        "resolve_existing_prefix() {{ q=$1; suffix=; while ! resolved=$(readlink -f -- \"$q\" 2>/dev/null); do base=${{q##*/}}; [ -n \"$base\" ] || return 1; suffix=/$base$suffix; q=${{q%/*}}; [ -n \"$q\" ] || q=/; done; printf '%s%s\\n' \"$resolved\" \"$suffix\"; }}; p={path}; if [ -L \"$p\" ]; then target=$(readlink -- \"$p\") || exit 1; if resolved=$(readlink -f -- \"$p\" 2>/dev/null); then :; else parent=${{p%/*}}; [ \"$parent\" != \"$p\" ] || parent=.; resolved_parent=$(resolve_existing_prefix \"$parent\") || exit 1; case $target in /*) candidate=$target ;; *) candidate=$resolved_parent/$target ;; esac; resolved=$(resolve_existing_prefix \"$candidate\") || exit 1; fi; printf 'symlink\\n%s\\n%s\\n' \"$target\" \"$resolved\"; elif [ -e \"$p\" ]; then printf 'file\\n'; readlink -f -- \"$p\"; else resolved=$(resolve_existing_prefix \"$p\") || exit 1; printf 'missing\\n%s\\n' \"$resolved\"; fi"
    )
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
