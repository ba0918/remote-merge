//! ディレクトリを確かめた結果（走査のスクリプトの標準エラーに出す）を読み、一覧に出した
//! ディレクトリと突き合わせて答えに入れる（docs/decision/records/2026-10-12-foundation-2.md の C14）。
//!
//! 確かめた結果は find の知らせと同じ標準エラーに混ざって出る。結果は NUL で区切った目印付きの
//! 記録で、知らせは NUL を含まない文なので見分けられる。形の壊れた記録・欠けた結果・余分な結果は、
//! どれも走査の誤りにする。

use std::collections::HashMap;

use remote_merge_core::scan::raw::{join_relative, ReadCause, Subtree, UnreadableDir};

pub use crate::scan_diagnostics::DirCheck;

/// 標準エラーを分けたもの
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StderrParts {
    /// find の知らせ（確かめた結果を除いた文）
    pub diagnostics: Vec<u8>,
    /// 下も集めた問いごとの、確かめた結果（スクリプトの問いの順）
    pub groups: Vec<Vec<DirCheck>>,
}

/// 目印より後ろの標準エラーを、知らせと確かめた結果に分ける
pub fn split_stderr(after_marker: &[u8]) -> Result<StderrParts, String> {
    let broken = || "A directory check in the scan output is broken".to_string();
    let mut parts = StderrParts::default();
    let mut tokens = after_marker.split(|byte| *byte == 0);
    while let Some(token) = tokens.next() {
        match token {
            b"" => {}
            b"@QS" => parts.groups.push(Vec::new()),
            b"@D" => {
                let check = read_check(&mut tokens).ok_or_else(broken)?;
                parts.groups.last_mut().ok_or_else(broken)?.push(check);
            }
            text if text.starts_with(b"@") => return Err(broken()),
            text => parts.diagnostics.extend_from_slice(text),
        }
    }
    Ok(parts)
}

/// "@D" に続く、パス・読めるか検索できるか（もうないかディレクトリでなければ "G"）・子の名前
/// （"N" を前に付ける）・"@E" を読む
fn read_check<'a>(tokens: &mut impl Iterator<Item = &'a [u8]>) -> Option<DirCheck> {
    let printed = tokens.next()?.to_vec();
    let (readable, searchable, gone) = match tokens.next()? {
        b"11" => (true, true, false),
        b"10" => (true, false, false),
        b"01" => (false, true, false),
        b"00" => (false, false, false),
        b"G" => (false, false, true),
        _ => return None,
    };
    let mut names = Vec::new();
    loop {
        match tokens.next()? {
            b"@E" => break,
            name => names.push(name.strip_prefix(b"N")?.to_vec()),
        }
    }
    Some(DirCheck {
        printed,
        readable,
        searchable,
        gone,
        names,
    })
}

/// 問いの起点 `start`（find に渡した形）の部分木に、確かめた結果を入れる。一覧に出したディレクトリ
/// （起点を含む）と確かめた結果は一つずつ対応しなければならない
pub fn apply_checks(tree: &mut Subtree, checks: &[DirCheck], start: &[u8]) -> Result<(), String> {
    let mismatch = |what: &str, path: &[u8]| {
        format!(
            "The directory checks do not match the listing ({what}): {}",
            String::from_utf8_lossy(path)
        )
    };
    let mut by_rel: HashMap<Vec<u8>, &DirCheck> = HashMap::new();
    for check in checks {
        let rel =
            relative(&check.printed, start).ok_or_else(|| mismatch("outside", &check.printed))?;
        if by_rel.insert(rel, check).is_some() {
            return Err(mismatch("twice", &check.printed));
        }
    }
    let listed: Vec<Vec<u8>> = std::iter::once(Vec::new())
        .chain(
            tree.entries
                .iter()
                .filter(|entry| entry.kind == remote_merge_core::scan::raw::RawKind::Directory)
                .map(|entry| entry.path.clone()),
        )
        .collect();
    for rel in &listed {
        let check = by_rel.remove(rel).ok_or_else(|| mismatch("missing", rel))?;
        if check.gone {
            tree.changed.push(rel.clone());
        } else if !check.readable {
            tree.unreadable
                .push(UnreadableDir::new(rel.clone(), ReadCause::PermissionDenied));
        } else if !check.searchable {
            for name in &check.names {
                tree.unreadable_entries.push(UnreadableDir::new(
                    join_relative(rel, name),
                    ReadCause::PermissionDenied,
                ));
            }
        }
    }
    match by_rel.into_values().next() {
        Some(extra) => Err(mismatch("not listed", &extra.printed)),
        None => Ok(()),
    }
}

/// find に渡した形のパスの、起点からの相対パス
pub(crate) fn relative(printed: &[u8], start: &[u8]) -> Option<Vec<u8>> {
    if printed == start {
        return Some(Vec::new());
    }
    printed
        .strip_prefix(start)?
        .strip_prefix(b"/")
        .map(<[u8]>::to_vec)
}

#[cfg(test)]
#[path = "scan_checks_tests.rs"]
mod tests;
