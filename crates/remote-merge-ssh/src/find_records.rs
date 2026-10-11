//! SSH の経路の走査と子の一覧で find が出す項目の読み取り（docs/ir/scan/listing.md の REQ-scan-006）。
//!
//! 名前とリンク先は TAB も改行も含みうるので、項目の列を NUL で区切って受け、バイト列のまま切り
//! 分ける。シェルの `$(...)` は末尾の改行を落とすので、リンク先は find の `%l` で受ける。UTF-8 として
//! 読めないバイトは、ほかの経路と同じく列ごとに置換文字にする。

use chrono::{DateTime, Utc};

/// find の `-printf` に渡す、一つの項目の七つの列。どの列も NUL で終える。
/// 種類、大きさ、更新時刻、権限、パス、リンク先、リンク先の種類の順
pub const RECORD_FORMAT: &str = "%y\\0%s\\0%T@\\0%m\\0%p\\0%l\\0%Y\\0";

/// [`RECORD_FORMAT`] の、パスの代わりに名前（`%f`）を出すもの
pub const NAMED_RECORD_FORMAT: &str = "%y\\0%s\\0%T@\\0%m\\0%f\\0%l\\0%Y\\0";

const FIELDS: usize = 7;

/// find が出した一つの項目
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindRecord {
    /// `%y`（f・d・l など）
    pub kind: String,
    pub size: String,
    /// `%T@`（"秒.小数"）
    pub mtime: String,
    /// `%m`（8 進の権限）
    pub mode: String,
    /// `%p` か `%f`
    pub path: String,
    /// `%l`（symlink でなければ空）
    pub link_target: String,
    /// `%Y`（symlink ならリンク先の種類）
    pub link_kind: String,
}

/// NUL で区切った find の出力を項目に切り分ける。列の数が七の倍数でないか、最後の列が NUL で
/// 終わっていなければ、出力が途中で切れたか形が違うので Err
pub fn parse_find_records(output: &[u8]) -> anyhow::Result<Vec<FindRecord>> {
    if output.is_empty() {
        return Ok(Vec::new());
    }
    let Some(body) = output.strip_suffix(b"\0") else {
        anyhow::bail!("unexpected output from find: the last entry is not terminated");
    };
    let fields: Vec<String> = body
        .split(|byte| *byte == 0)
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect();
    let (records, rest) = fields.as_chunks::<FIELDS>();
    if !rest.is_empty() {
        anyhow::bail!(
            "unexpected output from find: {} fields do not make whole entries",
            fields.len()
        );
    }
    Ok(records
        .iter()
        .map(
            |[kind, size, mtime, mode, path, link_target, link_kind]| FindRecord {
                kind: kind.clone(),
                size: size.clone(),
                mtime: mtime.clone(),
                mode: mode.clone(),
                path: path.clone(),
                link_target: link_target.clone(),
                link_kind: link_kind.clone(),
            },
        )
        .collect())
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
    fn names_and_link_targets_keep_tabs_newlines_and_backslashes() {
        let output = b"f\x001\x001.5\x00644\x00/r/x\ty.txt\x00\x00f\x00\
l\x004\x002.0\x00777\x00/r/p\nq\x00line\n\x00N\x00\
l\x004\x002.0\x00777\x00/r/b\\c\x00a\tb\\c\x00f\x00";
        let records = parse_find_records(output).unwrap();
        let paths: Vec<&str> = records.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["/r/x\ty.txt", "/r/p\nq", "/r/b\\c"]);
        assert_eq!(records[1].link_target, "line\n");
        assert_eq!(records[1].link_kind, "N");
        assert_eq!(records[2].link_target, "a\tb\\c");
        assert_eq!(records[0].mtime, "1.5");
    }

    #[test]
    fn empty_output_has_no_entries() {
        assert!(parse_find_records(b"").unwrap().is_empty());
    }

    #[test]
    fn output_cut_in_the_middle_of_an_entry_is_an_error() {
        assert!(parse_find_records(b"f\x001\x001.0\x00644\x00/r/a\x00\x00f").is_err());
        assert!(parse_find_records(b"f\x001\x001.0\x00644\x00/r/a\x00").is_err());
    }

    #[test]
    fn bytes_that_are_not_utf8_become_replacement_characters_in_their_field_only() {
        let output = b"f\x001\x001.0\x00644\x00/r/\xff.txt\x00\x00f\x00";
        let records = parse_find_records(output).unwrap();
        assert_eq!(records[0].path, "/r/\u{fffd}.txt");
        assert_eq!(records[0].kind, "f");
    }

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
}
