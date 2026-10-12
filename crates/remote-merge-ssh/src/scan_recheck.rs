//! find が子を一つも出さなかったのに、確かめで読めて検索できると答えたディレクトリに、本当に
//! 空かをもう一度まとめて尋ねる（docs/decision/records/2026-10-12-foundation-2.md の C14）。
//!
//! findutils 4.4.2 は検索できないディレクトリの子を知らせずに出さないので、find が通った後で
//! 確かめの前に検索できるようになると、子のあるディレクトリが空に見える。もう一度尋ねて子が
//! あれば、走査の途中で変わったものとして答える。find の版では分けない。

use std::collections::HashSet;
use std::ops::Range;

use remote_merge_core::scan::raw::Subtree;

use crate::scan_checks::{check_script, relative};
use crate::scan_diagnostics::DirCheck;
pub use crate::scan_recheck_read::{apply_recheck, match_round, read_recheck};
use crate::scan_script::{assign, root_setup, split_by_length, wrapped_len, END, MARKER};

/// もう一度尋ねるディレクトリ。`answer` は問いの番号、`rel` は問いのパスからの相対パス、
/// `printed` は find に渡した形のパス
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyDir {
    pub answer: usize,
    pub rel: Vec<u8>,
    pub printed: Vec<u8>,
}

/// 読めて検索できると確かめたのに、find が子を一つも出さなかったディレクトリ（相対パスと
/// find に渡した形のパス）
pub fn empty_dirs(tree: &Subtree, checks: &[DirCheck], start: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let parents: HashSet<&[u8]> = tree
        .entries
        .iter()
        .map(|entry| entry.path.as_slice())
        .chain(
            tree.unreadable_entries
                .iter()
                .map(|item| item.path.as_slice()),
        )
        .map(parent_of)
        .collect();
    checks
        .iter()
        .filter(|check| check.readable && check.searchable && !check.gone)
        .filter_map(|check| {
            let rel = relative(&check.printed, start)?;
            (!parents.contains(rel.as_slice())).then(|| (rel, check.printed.clone()))
        })
        .collect()
}

fn parent_of(path: &[u8]) -> &[u8] {
    path.iter()
        .rposition(|byte| *byte == b'/')
        .map_or(&path[..0], |at| &path[..at])
}

/// root_dir `root` に移り、`printed` のディレクトリそれぞれの子の名前を、確かめと同じ形で
/// 標準エラーに出すスクリプト
pub fn recheck_script(root: &[u8], printed: &[&[u8]]) -> String {
    let mut script =
        format!("LC_ALL=C; export LC_ALL\nprintf '{MARKER}\\000'\nprintf '{MARKER}\\n' >&2\n");
    script.push_str(&check_script(true));
    script.push_str(&root_setup(root));
    let args: Vec<(String, String)> = printed
        .iter()
        .enumerate()
        .map(|(index, path)| dir_arg(index, path))
        .collect();
    for (setup, _) in &args {
        script.push_str(setup);
    }
    script.push_str("printf '\\000@QS\\000' >&2; sh -c \"$c\" sh");
    for (_, arg) in &args {
        script.push_str(" \\\n ");
        script.push_str(arg);
    }
    script.push_str("\nprintf '@R\\000%s\\000' \"$?\"\n");
    script.push_str(END);
    script
}

/// 一つのディレクトリを sh に渡す引数と、その前に置く文。表示できる ASCII で "'" を含まない
/// パスはそのまま引用し、ほかは変数に入れてから渡す（コマンド置換の費用は docs/remote-tools.md）
fn dir_arg(index: usize, path: &[u8]) -> (String, String) {
    let plain = path
        .iter()
        .all(|byte| (b' '..=b'~').contains(byte) && *byte != b'\'');
    if plain {
        return (
            String::new(),
            format!("'{}'", String::from_utf8_lossy(path)),
        );
    }
    let name = format!("a{index}");
    (assign(&name, path), format!("\"${name}\""))
}

fn dir_line(index: usize, path: &[u8]) -> String {
    let (setup, arg) = dir_arg(index, path);
    format!("{setup} \\\n {arg}")
}

/// もう一度尋ねるディレクトリを、コマンド行が `limit` バイトに収まるまとまりに分ける
pub fn split_recheck(root: &[u8], printed: &[&[u8]], limit: usize) -> Vec<Range<usize>> {
    let base = wrapped_len(&recheck_script(root, &[]));
    let lines = printed
        .iter()
        .enumerate()
        .map(|(index, path)| dir_line(index, path));
    split_by_length(base, lines, limit)
}

#[cfg(all(test, unix))]
#[path = "scan_recheck_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "scan_agree_empty_tests.rs"]
mod agree_empty_tests;

#[cfg(all(test, unix))]
#[path = "scan_recheck_script_tests.rs"]
mod script_tests;
