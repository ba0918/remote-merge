use std::path::PathBuf;

use super::target_io::TargetPath;

pub(crate) fn build_inspect_path_command(path: &str) -> String {
    let path = crate::ssh::tree_parser::shell_escape(path);
    format!(
        "p={path}; if [ -L \"$p\" ]; then target=$(readlink -- \"$p\") || exit 1; resolved=$(readlink -f -- \"$p\") || exit 1; printf 'symlink\\n%s\\n%s\\n' \"$target\" \"$resolved\"; elif [ -e \"$p\" ]; then printf 'file\\n'; readlink -f -- \"$p\"; else parent=${{p%/*}}; [ \"$parent\" != \"$p\" ] || parent=.; suffix=; while ! resolved=$(readlink -f -- \"$parent\" 2>/dev/null); do base=${{parent##*/}}; [ -n \"$base\" ] || exit 1; suffix=/$base$suffix; parent=${{parent%/*}}; [ -n \"$parent\" ] || parent=/; done; printf 'missing\\n%s%s\\n' \"$resolved\" \"$suffix\"; fi"
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
}
