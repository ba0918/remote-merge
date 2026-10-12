//! 走査のスクリプト（[`crate::scan_script`]）の出力を、問いへの答えにする。読めない出力・
//! 読めないパスを知らせる以外の find の診断・途中で切れた出力は、空にせずに誤りにする。

use remote_merge_core::scan::raw::{Answer, LinkTarget, RawEntry, Subtree, Unreached};

use crate::scan_checks::{apply_checks, split_stderr};
use crate::scan_recheck::{empty_dirs, EmptyDir};
use crate::scan_tokens::Tokens;

use crate::scan_fields::{link_target_from, printed_start, raw_entry, unreached_from, Record};
use crate::scan_script::{ScriptQuestion, MARKER};

/// find が出したパスの、問いのパスからの相対パス
fn relative(printed: &[u8], start: &[u8]) -> Result<Vec<u8>, String> {
    crate::scan_checks::relative(printed, start).ok_or_else(|| {
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
            unreadable_entries: Vec::new(),
            changed: Vec::new(),
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

/// 一度目の走査の答えと、空かをもう一度尋ねるディレクトリ（[`crate::scan_recheck`]）
#[derive(Debug)]
pub struct Parsed {
    pub answers: Vec<Answer>,
    pub empty: Vec<EmptyDir>,
}

/// 標準出力と標準エラーを読んで、問いごとの答えにする
pub fn parse(stdout: &[u8], stderr: &[u8], questions: &[ScriptQuestion]) -> Result<Parsed, String> {
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
                return Ok(Parsed {
                    answers: questions.iter().map(|_| unreached_answer(why)).collect(),
                    empty: Vec::new(),
                });
            }
        }
        answers.push(answer(&mut tokens, question, &mut statuses)?);
    }
    tokens.expect(b"@END")?;
    let marker = [MARKER.as_bytes(), b"\n"].concat();
    let after = stderr
        .windows(marker.len())
        .position(|window| window == marker)
        .map_or(&stderr[stderr.len()..], |at| &stderr[at + marker.len()..]);
    let parts = split_stderr(after)?;
    let mut groups = parts.groups.into_iter();
    let mut checks = Vec::new();
    let mut empty = Vec::new();
    for (index, (question, answer)) in questions.iter().zip(answers.iter_mut()).enumerate() {
        let Some(tree) = &mut answer.subtree else {
            continue;
        };
        let group = groups
            .next()
            .ok_or("The directory checks in the scan output are missing")?;
        let start = printed_start(question);
        apply_checks(tree, &group, &start)?;
        empty.extend(
            empty_dirs(tree, &group, &start)
                .into_iter()
                .map(|(rel, printed)| EmptyDir {
                    answer: index,
                    rel,
                    printed,
                }),
        );
        checks.extend(group);
    }
    if groups.next().is_some() {
        return Err("The scan output has directory checks for no question".into());
    }
    let explained = crate::scan_diagnostics::validate(&parts.diagnostics, &checks)?;
    if explained == 0 && statuses.iter().any(|status| status != b"0") {
        return Err("find failed without saying which directory it could not read".into());
    }
    Ok(Parsed { answers, empty })
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

#[cfg(all(test, unix))]
#[path = "scan_agree_search_tests.rs"]
mod agree_search_tests;
