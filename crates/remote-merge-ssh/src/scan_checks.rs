//! ディレクトリを確かめた結果（走査のスクリプトの標準エラーに出す）を読み、一覧に出した
//! ディレクトリと突き合わせて答えに入れる（docs/decision/records/2026-10-12-foundation-2.md の C14）。
//!
//! 確かめた結果は find の知らせと同じ標準エラーに混ざって出る。結果は NUL で区切った目印付きの
//! 記録で、知らせは NUL を含まない文なので見分けられる。形の壊れた記録・欠けた結果・余分な結果は、
//! どれも走査の誤りにする。

use std::collections::HashMap;

use remote_merge_core::scan::raw::{join_relative, ReadCause, Subtree, UnreadableDir};

pub use crate::scan_diagnostics::DirCheck;

/// find が出会ったディレクトリごとに、読めるか・検索できるかを確かめるスクリプト（変数 c）。結果は
/// find の一覧と混ざらないよう標準エラーに、NUL で区切った目印付きの記録で出す。
/// find の診断から推し量らない（診断を出さない find もある）。もうないかディレクトリでなくなって
/// いたら "G" を出す。読めて検索できないディレクトリ（`every_name` なら読めるディレクトリすべて）は、
/// その場で glob で子の名前を集める。glob が当たらずにパターンの文字列が返ったときは、その文字列を
/// 名前に持つ子があるかを確かめて見分ける。検索できるディレクトリでは test で、検索できない
/// ディレクトリでは一文字を [] で囲んだ glob で確かめる。シェルの関数は使わない（古い bash は
/// 関数を呼ぶたびに位置引数を写すので、数千のディレクトリを渡すと秒の単位で遅くなる）
pub(crate) fn check_script(every_name: bool) -> String {
    let names_when = if every_name {
        "[ $r = 1 ]"
    } else {
        "[ $r = 1 ] && [ $x = 0 ]"
    };
    format!(
        concat!(
            "c='exec >&2\n",
            "for d do printf \"\\000@D\\000%s\\000\" \"$d\"\n",
            " if [ ! -d \"$d\" ]; then printf \"G\\000\"; else\n",
            " if [ -r \"$d\" ]; then r=1; else r=0; fi; if [ -x \"$d\" ]; then x=1; else x=0; fi\n",
            " printf \"%s%s\\000\" $r $x\n",
            " if {names_when}; then for p in \"$d\"/* \"$d\"/.[!.]* \"$d\"/..?*; do\n",
            "  case \"$p\" in\n",
            "   \"$d/*\") g=\"[*]\";; \"$d/.[!.]*\") g=\".[[]!.][*]\";; \"$d/..?*\") g=\"..[?][*]\";; *) g=;;\n",
            "  esac\n",
            "  if [ -z \"$g\" ]; then :\n",
            "  elif [ $x = 1 ]; then [ -e \"$p\" ] || [ -h \"$p\" ] || continue\n",
            "  else k=0; for q in \"$d\"/$g; do [ \"$q\" != \"$d/$g\" ] && k=1; done; [ $k = 1 ] || continue; fi\n",
            "  printf \"N%s\\000\" \"${{p##*/}}\"\n",
            " done; fi; fi; printf \"@E\\000\"\n",
            "done'\n",
        ),
        names_when = names_when
    )
}

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
