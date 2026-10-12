//! SSH の経路のパスの確認（docs/ir/paths/routes.md、docs/decision/records/2026-10-12-foundation-3.md
//! の D5）。辿る手順は core の `path_resolution` のものをそのまま使い、ここは手順が求めた要素の
//! 事実をまとめて尋ねるだけにする。

use remote_merge_core::path_resolution::facts::{next_step, FactBook, Step};
use remote_merge_core::path_resolution::{ResolveError, ResolvedPath};

use crate::path_facts_output::{parse, parse_working_directory, Answer};
use crate::path_facts_script::{build, split, working_directory};

/// まとめて尋ねられなかった理由
#[derive(Debug)]
pub enum FactsFailure<E> {
    /// コマンドを動かせなかった
    Run(E),
    /// 答えを問いへの答えとして読めなかった（接続断として扱う）
    Unreadable(String),
    /// リモートに ls がない
    NoLs,
}

/// 一つのパスの確認の結果
pub type Resolution = Result<ResolvedPath, ResolveError>;

/// root_dir `root` の下の `rels` を確かめる。`run` はスクリプトを動かして標準出力を返す。
/// 相対の root_dir は、リモートの作業ディレクトリの実パスを基準にする（REQ-paths-012）
pub fn resolve_all<E>(
    root: &str,
    rels: &[String],
    limit: usize,
    mut run: impl FnMut(&str) -> Result<Vec<u8>, E>,
) -> Result<Vec<Resolution>, FactsFailure<E>> {
    let root = if root.starts_with('/') {
        root.to_string()
    } else {
        let stdout = run(&working_directory()).map_err(FactsFailure::Run)?;
        let base = parse_working_directory(&stdout).map_err(FactsFailure::Unreadable)?;
        match base.map(String::from_utf8) {
            Some(Ok(base)) => format!("{}/{root}", base.trim_end_matches('/')),
            Some(Err(_)) => return Ok(vec![Err(ResolveError::NonUtf8Path); rels.len()]),
            None => return Ok(vec![Err(ResolveError::CannotRead); rels.len()]),
        }
    };
    let mut facts = FactBook::default();
    let mut results: Vec<Option<Resolution>> = vec![None; rels.len()];
    loop {
        let chains = questions(&root, rels, &facts, &mut results);
        if chains.is_empty() {
            return Ok(results
                .into_iter()
                .map(|r| r.expect("every path was resolved"))
                .collect());
        }
        for range in split(&chains, limit) {
            let asked = &chains[range];
            let stdout = run(&build(asked)).map_err(FactsFailure::Run)?;
            match parse(&stdout, asked).map_err(FactsFailure::Unreadable)? {
                Answer::NoLs => return Err(FactsFailure::NoLs),
                Answer::Facts(answers) => {
                    for (path, fact) in answers.into_iter().flatten() {
                        if facts.get(&path).is_some() {
                            return Err(FactsFailure::Unreadable(format!(
                                "{path:?} was answered twice"
                            )));
                        }
                        facts.insert(path, fact);
                    }
                }
            }
        }
    }
}

/// まだ辿り終えていないパスを辿り、辿り終えたものは `results` に置き、事実の要る要素を問いに
/// する。同じ回の前の問いにある要素は尋ねない
fn questions(
    root: &str,
    rels: &[String],
    facts: &FactBook,
    results: &mut [Option<Resolution>],
) -> Vec<Vec<String>> {
    let mut asked = std::collections::HashSet::new();
    let mut chains = Vec::new();
    for (rel, result) in rels.iter().zip(results.iter_mut()) {
        if result.is_some() {
            continue;
        }
        match next_step(root, rel, facts) {
            Step::Done(resolution) => *result = Some(resolution),
            Step::Ask(chain) => {
                let chain: Vec<String> = chain
                    .into_iter()
                    .filter(|path| asked.insert(path.clone()))
                    .collect();
                if !chain.is_empty() {
                    chains.push(chain);
                }
            }
        }
    }
    chains
}

#[cfg(all(test, unix))]
#[path = "path_facts_tests.rs"]
mod tests;
