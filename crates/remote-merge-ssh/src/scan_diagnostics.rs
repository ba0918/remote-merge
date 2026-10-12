//! 走査のスクリプトの標準エラーに find が出した知らせを、ディレクトリを確かめた結果と照らし合わせる。
//! 読めるか・検索できるかは確かめた結果で決め、知らせからは推し量らない。確かめた結果と合わない
//! 知らせと、読み取れない知らせは走査の誤りにする
use std::collections::HashMap;

use crate::scan_fields::diagnosed_path;

/// find が出会ったディレクトリを確かめた結果。`printed` は find に渡した形のパス
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirCheck {
    pub printed: Vec<u8>,
    pub readable: bool,
    pub searchable: bool,
    /// 確かめたときに、もうないかディレクトリでなかった（読めも検索もできないものとして扱う）
    pub gone: bool,
    /// 読めて検索できないときの子の名前
    pub names: Vec<Vec<u8>>,
}

/// 知らせを照らし合わせた結果
#[derive(Debug, PartialEq, Eq)]
pub struct Validated {
    /// 確かめた結果と合った知らせの数
    pub explained: usize,
    /// 一覧に出たのに、情報を取りに行ったらなかった項目（find に渡した形のパス）
    pub vanished: Vec<Vec<u8>>,
}

/// 項目がもうないときの知らせの終わり（LC_ALL=C）
const VANISHED: &[u8] = b": No such file or directory";

/// 知らせがすべて確かめた結果と合うかを確かめる。ディレクトリについての知らせはそのディレクトリが
/// 読めないか検索できないとき、項目についての知らせはそれを入れているディレクトリが検索できない
/// ときだけ合う。項目がもうないという知らせは、走査の途中で変わった項目として返す
pub fn validate(diagnostics: &[u8], checks: &[DirCheck]) -> Result<Validated, String> {
    let checked: HashMap<&[u8], &DirCheck> = checks
        .iter()
        .map(|check| (check.printed.as_slice(), check))
        .collect();
    let mut explained = 0;
    let mut vanished = Vec::new();
    for message in messages(diagnostics) {
        let unexplained = || format!("find reported: {}", String::from_utf8_lossy(&message));
        let path = diagnosed_path(&message).ok_or_else(unexplained)?;
        // 一覧に出た名前の情報を取りに行ったら、もうなかった（docs/remote-tools.md）
        if message.ends_with(VANISHED) {
            vanished.push(path);
            explained += 1;
            continue;
        }
        let fits = match checked.get(path.as_slice()) {
            Some(check) => !check.readable || !check.searchable,
            None => path
                .iter()
                .rposition(|byte| *byte == b'/')
                .and_then(|at| checked.get(&path[..at]))
                .is_some_and(|parent| !parent.searchable),
        };
        if !fits {
            return Err(unexplained());
        }
        explained += 1;
    }
    Ok(Validated {
        explained,
        vanished,
    })
}

/// 標準エラーを find の知らせ一つずつに分ける。古い find（4.2.27）はパスを引用しないので、名前の
/// 中の改行で行が割れる。"find: " で始まらない行と、前の知らせがまだ読み分けられないときの行は、
/// 前の知らせの続きとしてつなぐ
fn messages(text: &[u8]) -> Vec<Vec<u8>> {
    let mut messages: Vec<Vec<u8>> = Vec::new();
    let body = text.strip_suffix(b"\n").unwrap_or(text);
    if body.is_empty() {
        return messages;
    }
    for line in body.split(|byte| *byte == b'\n') {
        // 前の知らせがまだパスと理由に読み分けられないなら、"find: " で始まる行も、名前の中の
        // 改行の後に続く部分としてつなぐ
        match messages.last_mut() {
            Some(last) if !line.starts_with(b"find: ") || diagnosed_path(last).is_none() => {
                last.push(b'\n');
                last.extend_from_slice(line);
            }
            _ => messages.push(line.to_vec()),
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_whose_unquoted_path_holds_a_newline_is_kept_whole() {
        let text = b"find: ./cache/lo\ncked: Permission denied\nfind: './x': Permission denied\n";
        assert_eq!(
            messages(text),
            [
                b"find: ./cache/lo\ncked: Permission denied".to_vec(),
                b"find: './x': Permission denied".to_vec()
            ]
        );
        assert!(messages(b"").is_empty());
        assert_eq!(
            messages(b"find: ./c/x\nfind: y: Permission denied\n"),
            [b"find: ./c/x\nfind: y: Permission denied".to_vec()]
        );
    }
}

#[cfg(test)]
mod validate_tests {
    use super::*;

    fn check(printed: &[u8], readable: bool, searchable: bool) -> DirCheck {
        DirCheck {
            printed: printed.to_vec(),
            readable,
            searchable,
            gone: false,
            names: Vec::new(),
        }
    }

    #[test]
    fn a_message_must_fit_what_the_directory_check_found() {
        let stderr = |line: &str| format!("{line}\n").into_bytes();
        let checks = [
            check(b"./locked", false, false),
            check(b"./d", true, false),
            check(b"./ok", true, true),
        ];
        let fits = [
            "find: './locked': Permission denied",
            "find: './d/x': Permission denied",
        ];
        for line in fits {
            assert_eq!(
                validate(&stderr(line), &checks).map(|read| read.explained),
                Ok(1),
                "{line}"
            );
        }
        let misfits = [
            "find: './ok': Permission denied",
            "find: './ok/x': Permission denied",
            "find: './unknown': Permission denied",
            "find: something else",
        ];
        for line in misfits {
            assert!(validate(&stderr(line), &checks).is_err(), "{line}");
        }
    }
}

#[cfg(test)]
mod vanished_tests {
    use super::*;

    // @kotowari[REQ-scan-017]
    #[test]
    fn an_entry_find_could_not_find_is_read_as_gone_in_every_quoting() {
        let checks = [DirCheck {
            printed: b"./ok".to_vec(),
            readable: true,
            searchable: true,
            gone: false,
            names: Vec::new(),
        }];
        let stderr = concat!(
            "find: ./ok/a: No such file or directory\n",
            "find: `./ok/b': No such file or directory\n",
            "find: './ok/c': No such file or directory\n",
        );
        let read = validate(stderr.as_bytes(), &checks).unwrap();
        assert_eq!(read.explained, 3);
        assert_eq!(
            read.vanished,
            [b"./ok/a".to_vec(), b"./ok/b".to_vec(), b"./ok/c".to_vec()]
        );
        assert!(validate(b"find: './ok/d': Input/output error\n", &checks).is_err());
    }
}
