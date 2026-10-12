//! 走査のスクリプト（[`crate::scan_script`]）の出力を、問いへの答えにする。読めない出力・
//! 読めないパスを知らせる以外の find の診断・途中で切れた出力は、空にせずに誤りにする。

use remote_merge_core::scan::raw::{Answer, LinkTarget, RawEntry, Subtree, Unreached};

use crate::scan_fields::{link_target_from, raw_entry, unreached_from, Record};
use crate::scan_script::{ScriptQuestion, MARKER};

/// NUL で区切った出力を前から読む
struct Tokens<'a> {
    rest: std::slice::Split<'a, u8, fn(&u8) -> bool>,
}

impl<'a> Tokens<'a> {
    fn next(&mut self) -> Result<&'a [u8], String> {
        self.rest
            .next()
            .ok_or_else(|| "The scan output ended early".to_string())
    }

    fn expect(&mut self, tag: &[u8]) -> Result<(), String> {
        let token = self.next()?;
        if token == tag {
            return Ok(());
        }
        Err(format!(
            "Unexpected scan output: {:?} instead of {:?}",
            String::from_utf8_lossy(token),
            String::from_utf8_lossy(tag)
        ))
    }

    /// 項目の 8 つの欄。`first` は先に読んだ最初の欄
    fn record(&mut self, first: &'a [u8]) -> Result<Record<'a>, String> {
        Ok(Record {
            kind: first,
            dev: self.next()?,
            ino: self.next()?,
            time: self.next()?,
            mode: self.next()?,
            size: self.next()?,
            link: self.next()?,
            path: self.next()?,
        })
    }
}

/// 問いのパスを find に渡した形（root_dir は "."、それ以外は "./" を前に付ける）
pub(crate) fn printed_start(question: &ScriptQuestion) -> Vec<u8> {
    if question.path.is_empty() {
        b".".to_vec()
    } else {
        [b"./", question.path].concat()
    }
}

/// find が出したパスの、問いのパスからの相対パス
fn relative(printed: &[u8], start: &[u8]) -> Result<Vec<u8>, String> {
    if printed == start {
        return Ok(Vec::new());
    }
    printed
        .strip_prefix(start)
        .and_then(|rest| rest.strip_prefix(b"/"))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| {
            format!(
                "Unexpected path in the scan output: {}",
                String::from_utf8_lossy(printed)
            )
        })
}

fn entry_of(record: &Record, start: &[u8]) -> Result<RawEntry, String> {
    raw_entry(record, relative(record.path, start)?).ok_or_else(|| {
        format!(
            "Unreadable scan record: {}",
            String::from_utf8_lossy(record.path)
        )
    })
}

/// 一つの問いへの答えを読む。読めないディレクトリはまだ入れない
fn answer(
    tokens: &mut Tokens,
    question: &ScriptQuestion,
    statuses: &mut Vec<Vec<u8>>,
) -> Result<Answer, String> {
    tokens.expect(b"@Q")?;
    let start = printed_start(question);
    let first = tokens.next()?;
    if first == b"@E" {
        let message = tokens.next()?;
        tokens.expect(b"@Z")?;
        return Ok(Answer {
            entry: Err(unreached_from(message)),
            link: None,
            subtree: None,
        });
    }
    let entry = entry_of(&tokens.record(first)?, &start)?;
    let mut answer = Answer {
        entry: Ok(entry),
        link: None,
        subtree: None,
    };
    let mut token = tokens.next()?;
    let link = match token {
        b"@LD" => Some(LinkTarget::Directory),
        b"@LF" => Some(LinkTarget::NotDirectory),
        b"@LX" => Some(link_target_from(tokens.next()?)),
        _ => None,
    };
    if link.is_some() {
        answer.link = link;
        token = tokens.next()?;
    }
    if token == b"@T" {
        let first = tokens.next()?;
        let mut tree = Subtree {
            start: entry_of(&tokens.record(first)?, &start)?,
            entries: Vec::new(),
            unreadable: Vec::new(),
        };
        loop {
            let first = tokens.next()?;
            if first == b"@R" {
                statuses.push(tokens.next()?.to_vec());
                break;
            }
            tree.entries.push(entry_of(&tokens.record(first)?, &start)?);
        }
        answer.subtree = Some(tree);
        token = tokens.next()?;
    }
    if token != b"@Z" {
        return Err(format!(
            "Unexpected scan output: {}",
            String::from_utf8_lossy(token)
        ));
    }
    Ok(answer)
}

/// 標準出力と標準エラーを読んで、問いごとの答えにする
pub fn parse(
    stdout: &[u8],
    stderr: &[u8],
    questions: &[ScriptQuestion],
) -> Result<Vec<Answer>, String> {
    let marker = [MARKER.as_bytes(), b"\0"].concat();
    let begin = stdout
        .windows(marker.len())
        .position(|window| window == marker)
        .ok_or("The scan output has no start marker")?;
    let split: fn(&u8) -> bool = |byte| *byte == 0;
    let mut tokens = Tokens {
        rest: stdout[begin + marker.len()..].split(split),
    };
    let mut answers = Vec::with_capacity(questions.len());
    let mut statuses = Vec::new();
    for question in questions {
        if answers.is_empty() {
            let mut peek = Tokens {
                rest: tokens.rest.clone(),
            };
            if peek.next()? == b"@F" {
                let why = match peek.next()? {
                    b"U" => Unreached::Unreadable,
                    b"" => Unreached::NotDirectory,
                    message => unreached_from(message),
                };
                return Ok(questions.iter().map(|_| unreached_answer(why)).collect());
            }
        }
        answers.push(answer(&mut tokens, question, &mut statuses)?);
    }
    tokens.expect(b"@END")?;
    let attributed = crate::scan_diagnostics::attribute(stderr, questions, &mut answers)?;
    if attributed == 0 && statuses.iter().any(|status| status != b"0") {
        return Err("find failed without saying which directory it could not read".into());
    }
    Ok(answers)
}

fn unreached_answer(why: Unreached) -> Answer {
    Answer {
        entry: Err(why),
        link: None,
        subtree: None,
    }
}

#[cfg(all(test, unix))]
#[path = "scan_output_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "scan_agree_tests.rs"]
mod agree_tests;
