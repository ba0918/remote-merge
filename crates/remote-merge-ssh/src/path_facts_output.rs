//! パスの要素の事実を尋ねたスクリプトの出力を読む。答えは問いと一つずつ対応しなければならず、
//! 欠け・余り・問いと違うパス・形の壊れた出力は読めない答えにする

use remote_merge_core::path_resolution::facts::PathFact;
use remote_merge_core::path_resolution::ResolveError;

use super::path_facts_script::MARKER;

/// 読めた答え
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    /// 問いごとに、確かめたパスと事実
    Facts(Vec<Vec<(String, PathFact)>>),
    /// リモートに ls がない
    NoLs,
}

/// ログインシェルが先に出したものを飛ばし、目印の後ろを NUL で区切った欄にする
fn fields(stdout: &[u8]) -> Result<Vec<&[u8]>, String> {
    let mark = [MARKER.as_bytes(), b"\0"].concat();
    let start = stdout
        .windows(mark.len())
        .position(|window| window == mark.as_slice())
        .ok_or("the answer has no marker")?;
    let rest = &stdout[start + mark.len()..];
    let mut fields: Vec<&[u8]> = rest.split(|byte| *byte == 0).collect();
    // 最後の NUL の後ろは空
    if fields.pop() != Some(&[][..]) {
        return Err("the answer does not end with NUL".into());
    }
    Ok(fields)
}

fn text(field: &[u8]) -> Result<String, String> {
    String::from_utf8(field.to_vec()).map_err(|_| "a path in the answer is not UTF-8".to_string())
}

/// 一つの記録の事実。`next` はリンク文字列を読むときに次の欄を取り出す
fn fact<'a>(
    code: &[u8],
    next: &mut impl Iterator<Item = &'a [u8]>,
) -> Result<(PathFact, bool), String> {
    Ok(match code {
        b"D" => (PathFact::Directory { searchable: true }, false),
        b"N" => (PathFact::Directory { searchable: false }, true),
        b"L" => {
            let target = next.next().ok_or("a link has no target")?;
            (PathFact::Symlink(Ok(target.to_vec())), true)
        }
        b"R" => (
            PathFact::Symlink(Err(ResolveError::ReadlinkNotAvailable)),
            true,
        ),
        b"F" => (PathFact::RegularFile, true),
        b"O" => (PathFact::Other, true),
        b"M" => (PathFact::Missing, true),
        b"X" => (PathFact::Unreadable, true),
        other => return Err(format!("unknown fact {:?}", String::from_utf8_lossy(other))),
    })
}

/// `chains` に答えた出力を読む
pub fn parse(stdout: &[u8], chains: &[Vec<String>]) -> Result<Answer, String> {
    let fields = fields(stdout)?;
    let mut iter = fields.into_iter().peekable();
    let mut answers = Vec::new();
    // ls が要るのにないと分かったところで、スクリプトは記録の代わりに "@NOLS" を出して終える。
    // 記録の欄（リンク文字列など）に同じ文字列が出ても取り違えないよう、記録の始まりでだけ読む
    for chain in chains {
        match iter.next() {
            Some(b"@Q") => {}
            Some(b"@NOLS") => return Ok(Answer::NoLs),
            _ => return Err("an answer is missing for a question".into()),
        }
        let mut facts = Vec::new();
        let mut stopped = false;
        while let Some(field) = iter.peek() {
            if *field == b"@Q" || *field == b"@END" {
                break;
            }
            if *field == b"@NOLS" {
                return Ok(Answer::NoLs);
            }
            let path = text(iter.next().expect("peeked"))?;
            let expected = chain.get(facts.len()).ok_or("more facts than asked")?;
            if stopped || &path != expected {
                return Err(format!("a fact for {path:?} was not asked here"));
            }
            let code = iter.next().ok_or("a fact has no kind")?;
            let (fact, stops) = fact(code, &mut iter)?;
            stopped = stops;
            facts.push((path, fact));
        }
        if facts.is_empty() && !chain.is_empty() {
            return Err("a question has no facts".into());
        }
        answers.push(facts);
    }
    if iter.next() != Some(&b"@END"[..]) || iter.next().is_some() {
        return Err("the answer has more than was asked".into());
    }
    Ok(Answer::Facts(answers))
}

/// 作業ディレクトリの実パスの答え（バイト列のまま）。求められなかったら None
pub fn parse_working_directory(stdout: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let fields = fields(stdout)?;
    match fields.as_slice() {
        [b"@END"] => Ok(None),
        [path, b"@END"] => Ok(Some(path.to_vec())),
        _ => Err("the working directory answer is malformed".into()),
    }
}
