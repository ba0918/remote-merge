//! SSH の経路の子の一覧のコマンドと、その出力の読み取り（docs/ir/paths/operations.md）。
//!
//! 「ディレクトリではない」「一覧を取れなかった」「直下の子」を区別する。更新時刻は find の
//! `%T@` を文字列のまま読み、find が出した桁まで（ナノ秒まで）を落とさない。

use chrono::{DateTime, Utc};

use crate::filter;
use crate::tree::FileNode;
use crate::tree_parser::shell_escape;
use remote_merge_core::listing::ChildListing;

const DIRECTORY: &str = "DIR";
const NOT_DIRECTORY: &str = "NOTDIR";
const UNLISTABLE: &str = "UNLISTABLE";

/// `dir` の直下の子を一覧するコマンド。最初の行で、一覧したか、ディレクトリでないか、
/// 一覧を取れなかったかを示す。ないパスとディレクトリでないものを途中に含むパスは、find の
/// 原因の文言（C ロケール）で「ディレクトリではない」と見分ける
pub fn list_children_command(dir: &str) -> String {
    format!(
        "p={p}; if [ -d \"$p\" ]; then printf '{DIRECTORY}\\n'; \
exec find \"$p\"/ -maxdepth 1 -mindepth 1 -printf '%y\\t%s\\t%T@\\t%m\\t%f\\t%l\\t%Y\\n'; \
elif [ -e \"$p\" ] || [ -L \"$p\" ]; then printf '{NOT_DIRECTORY}\\n'; \
else e=$(LC_ALL=C find \"$p\" -maxdepth 0 2>&1 >/dev/null); case $e in \
*'No such file or directory'*|*'Not a directory'*) printf '{NOT_DIRECTORY}\\n';; \
*) printf '{UNLISTABLE}\\n%s\\n' \"$e\";; esac; fi",
        p = shell_escape(dir)
    )
}

/// [`list_children_command`] の出力を読む。`rel` は一覧したディレクトリの root_dir からのパス。
/// 項目の更新時刻などを読めなければ、既定の値で埋めずに Err にする
pub fn parse_children_output(
    stdout: &str,
    stderr: &str,
    exit_code: u32,
    rel: &str,
    exclude: &[String],
    max_entries: usize,
) -> anyhow::Result<ChildListing> {
    let (head, rest) = stdout.split_once('\n').unwrap_or((stdout, ""));
    match head {
        NOT_DIRECTORY => Ok(ChildListing::NotDirectory),
        UNLISTABLE => Ok(ChildListing::Unlistable(format!(
            "failed to list {rel:?}: {}",
            rest.trim()
        ))),
        DIRECTORY if exit_code != 0 => Ok(ChildListing::Unlistable(format!(
            "failed to list {rel:?}: {}",
            stderr.trim()
        ))),
        DIRECTORY => children(rest, rel, exclude, max_entries).map(ChildListing::Children),
        _ => anyhow::bail!("failed to list {rel:?}: unexpected output from the server: {head:?}"),
    }
}

fn children(
    lines: &str,
    rel: &str,
    exclude: &[String],
    max_entries: usize,
) -> anyhow::Result<Vec<FileNode>> {
    let mut nodes = Vec::new();
    for line in lines.lines().filter(|line| !line.is_empty()) {
        let fields: Vec<&str> = line.splitn(7, '\t').collect();
        let [kind, size, mtime, mode, name, target, target_kind] = fields.as_slice() else {
            anyhow::bail!("failed to list {rel:?}: unexpected output from the server: {line:?}");
        };
        let child_rel = if rel.is_empty() {
            name.to_string()
        } else {
            format!("{rel}/{name}")
        };
        // ローカルとエージェントの経路と同じく、root_dir からのパスに当てる
        if filter::is_path_excluded(&child_rel, exclude) {
            continue;
        }
        if nodes.len() >= max_entries {
            break;
        }
        let mut node = match *kind {
            "d" => FileNode::new_dir(*name),
            "l" => {
                let mut node = FileNode::new_symlink(*name, *target);
                node.link_is_dir = *target_kind == "d";
                node
            }
            _ => FileNode::new_file(*name),
        };
        let unreadable = |what: &str, raw: &str| {
            anyhow::anyhow!("failed to list {child_rel:?}: cannot read the {what}: {raw:?}")
        };
        node.size = Some(size.parse().map_err(|_| unreadable("size", size))?);
        node.permissions = Some(
            u32::from_str_radix(mode, 8).map_err(|_| unreadable("permissions", mode))? & 0o7777,
        );
        node.mtime =
            Some(parse_find_time(mtime).ok_or_else(|| unreadable("modification time", mtime))?);
        nodes.push(node);
    }
    Ok(nodes)
}

/// find の `%T@`（"秒.小数"）を、出した桁までの時刻に読む
pub fn parse_find_time(raw: &str) -> Option<DateTime<Utc>> {
    let (seconds, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    let seconds: i64 = seconds.parse().ok()?;
    if !fraction.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let digits: String = fraction
        .chars()
        .chain(std::iter::repeat('0'))
        .take(9)
        .collect();
    DateTime::from_timestamp(seconds, digits.parse().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_time_keeps_the_digits_find_printed_without_rounding() {
        let time = parse_find_time("1700000000.1234567890").unwrap();
        assert_eq!(time.timestamp(), 1_700_000_000);
        assert_eq!(time.timestamp_subsec_nanos(), 123_456_789);
        let seconds_only = parse_find_time("1700000000").unwrap();
        assert_eq!(seconds_only.timestamp_subsec_nanos(), 0);
        assert_eq!(
            parse_find_time("1700000000.5")
                .unwrap()
                .timestamp_subsec_nanos(),
            500_000_000
        );
        assert!(parse_find_time("x.1").is_none());
        assert!(parse_find_time("1.x").is_none());
    }

    #[test]
    fn the_first_line_tells_a_listing_from_a_non_directory_and_an_unlistable_directory() {
        let listing = |stdout: &str, exit: u32| {
            parse_children_output(stdout, "find: Permission denied", exit, "d", &[], 10).unwrap()
        };
        assert!(matches!(listing("NOTDIR\n", 0), ChildListing::NotDirectory));
        assert!(
            matches!(listing("UNLISTABLE\nfind: x: Permission denied\n", 0), ChildListing::Unlistable(reason) if reason.contains("Permission denied") && reason.contains("\"d\""))
        );
        assert!(
            matches!(listing("DIR\n", 1), ChildListing::Unlistable(reason) if reason.contains("Permission denied"))
        );
        assert!(matches!(listing("DIR\n", 0), ChildListing::Children(nodes) if nodes.is_empty()));
    }

    #[test]
    fn a_link_target_keeps_its_surrounding_spaces_and_special_permission_bits_are_kept() {
        let stdout =
            "DIR\nl\t3\t1700000000.5\t777\tl\t t \tf\nd\t4096\t1700000000.0\t2755\ts\t\td\n";
        let ChildListing::Children(nodes) =
            parse_children_output(stdout, "", 0, "", &[], 10).unwrap()
        else {
            panic!();
        };
        assert!(
            matches!(&nodes[0].kind, crate::tree::NodeKind::Symlink { target } if target == " t ")
        );
        assert_eq!(nodes[1].permissions, Some(0o2755));
    }

    #[test]
    fn an_unreadable_modification_time_stops_the_listing_instead_of_being_left_empty() {
        let stdout = "DIR\nf\t1\tnan\t644\ta.txt\t\tf\n";
        let error = parse_children_output(stdout, "", 0, "", &[], 10)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("a.txt") && error.contains("modification time"),
            "{error}"
        );
    }

    #[test]
    fn the_listing_stops_at_the_entry_limit() {
        let stdout = "DIR\nf\t1\t1.0\t644\ta\t\tf\nf\t1\t1.0\t644\tb\t\tf\n";
        let ChildListing::Children(nodes) =
            parse_children_output(stdout, "", 0, "", &[], 1).unwrap()
        else {
            panic!();
        };
        assert_eq!(nodes.len(), 1);
    }
}
