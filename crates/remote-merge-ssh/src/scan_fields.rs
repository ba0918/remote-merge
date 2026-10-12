//! 走査のスクリプトの出力の一項目と、ls・find の文言の読み方

use remote_merge_core::scan::raw::{DirId, LinkTarget, RawEntry, RawKind, RawTime, Unreached};

/// find の -printf の一項目（[`super::scan_script::RECORD_FORMAT`] の 8 つの欄）
pub struct Record<'a> {
    pub kind: &'a [u8],
    pub dev: &'a [u8],
    pub ino: &'a [u8],
    pub time: &'a [u8],
    pub mode: &'a [u8],
    pub size: &'a [u8],
    pub link: &'a [u8],
    pub path: &'a [u8],
}

/// 10 進か 8 進の数の欄
fn number<T: TryFrom<u64>>(field: &[u8], radix_octal: bool) -> Option<T> {
    let text = std::str::from_utf8(field).ok()?;
    let value = if radix_octal {
        u64::from_str_radix(text, 8).ok()?
    } else {
        text.parse::<u64>().ok()?
    };
    T::try_from(value).ok()
}

/// find の %T@ を読む。秒の整数部と、出た桁までの小数部（古い find は秒だけを出す）
pub fn parse_time(field: &[u8]) -> RawTime {
    let Ok(text) = std::str::from_utf8(field) else {
        return RawTime::Unreadable;
    };
    let (secs, fraction) = text.split_once('.').unwrap_or((text, ""));
    let Ok(secs) = secs.parse::<i64>() else {
        return RawTime::Unreadable;
    };
    if !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return RawTime::Unreadable;
    }
    let digits = fraction.len().min(9);
    let mut nanos_text = fraction[..digits].to_string();
    while nanos_text.len() < 9 {
        nanos_text.push('0');
    }
    match nanos_text.parse::<u32>() {
        Ok(nanos) => RawTime::Read {
            secs,
            nanos,
            digits: digits as u8,
        },
        Err(_) => RawTime::Unreadable,
    }
}

/// 一項目を生の項目にする。`rel` は問いのパスからの相対パス。欄を読めなければ None
pub fn raw_entry(record: &Record, rel: Vec<u8>) -> Option<RawEntry> {
    let kind = match record.kind {
        b"f" => RawKind::File,
        b"d" => RawKind::Directory,
        b"l" => RawKind::Symlink {
            target: (!record.link.is_empty()).then(|| record.link.to_vec()),
        },
        [_] => RawKind::Other,
        _ => return None,
    };
    Some(RawEntry {
        path: rel,
        kind,
        size: number(record.size, false)?,
        mtime: parse_time(record.time),
        mode: number(record.mode, true)?,
        id: DirId {
            dev: number(record.dev, false)?,
            ino: number(record.ino, false)?,
        },
    })
}

/// ls が出した文言の終わりから、パスに届かなかった理由を読む
pub fn unreached_from(message: &[u8]) -> Unreached {
    if message.ends_with(b"No such file or directory") {
        Unreached::Missing
    } else if message.ends_with(b"Not a directory") {
        Unreached::NotDirectory
    } else {
        Unreached::Unreadable
    }
}

/// リンク先を辿れなかったときの ls の文言から、リンク切れか辿れないかを読む（REQ-scan-018）
pub fn link_target_from(message: &[u8]) -> LinkTarget {
    match unreached_from(message) {
        Unreached::Missing => LinkTarget::Missing,
        _ => LinkTarget::Unresolvable,
    }
}

/// find の標準エラーの一行から、読めなかったパスを読む。find の版で引用の仕方が違う
/// （4.5.11 以降は '…'、4.4.2 は `…'、4.2.27 は引用しない）。引用の中は C の書き方
/// （"\\ooo"・"\\n"・"\\'" など）で書かれるので元のバイト列に戻し、戻せなければ読まない
pub fn diagnosed_path(line: &[u8]) -> Option<Vec<u8>> {
    let rest = line.strip_prefix(b"find: ")?;
    let end = rest.windows(2).rposition(|pair| pair == b": ")?;
    let path = match &rest[..end] {
        [b'\'', inner @ .., b'\''] | [b'`', inner @ .., b'\''] => unescape(inner)?,
        raw => raw.to_vec(),
    };
    (!path.is_empty()).then_some(path)
}

/// C の書き方のエスケープを戻す。知らない書き方があれば None
fn unescape(quoted: &[u8]) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(quoted.len());
    let mut rest = quoted;
    while let Some((&byte, after)) = rest.split_first() {
        rest = after;
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        let (&code, after) = rest.split_first()?;
        rest = after;
        let plain = match code {
            b'\\' | b'\'' | b'"' | b'?' => code,
            b'n' => b'\n',
            b't' => b'\t',
            b'r' => b'\r',
            b'a' => 0x07,
            b'b' => 0x08,
            b'f' => 0x0c,
            b'v' => 0x0b,
            b'0'..=b'7' => {
                let digits = rest
                    .iter()
                    .take(2)
                    .take_while(|d| (b'0'..=b'7').contains(*d))
                    .count();
                let octal = std::str::from_utf8(&[&[code][..], &rest[..digits]].concat())
                    .ok()?
                    .to_string();
                rest = &rest[digits..];
                u8::from_str_radix(&octal, 8).ok()?
            }
            _ => return None,
        };
        bytes.push(plain);
    }
    Some(bytes)
}

/// 問いのパスを find に渡した形（root_dir は "."、それ以外は "./" を前に付ける）
pub(crate) fn printed_start(question: &crate::scan_script::ScriptQuestion) -> Vec<u8> {
    if question.path.is_empty() {
        b".".to_vec()
    } else {
        [b"./", question.path].concat()
    }
}

#[cfg(test)]
#[path = "scan_fields_tests.rs"]
mod tests;
