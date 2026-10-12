//! SSH の経路の走査の集め方。問いをまとめてスクリプトにし、一度の SSH の実行で答えさせる
//! （docs/decision/records/2026-10-12-foundation-2.md の C7）。

use remote_merge_core::error::AppError;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::raw::{Answer, CollectError, Collector, Question};

use crate::batch_read::SSH_BATCH_MAX_COMMAND_LEN;
use crate::client::SshClient;
use crate::scan_diagnostics::DirCheck;
use crate::scan_output::parse;
use crate::scan_recheck::{apply_recheck, read_recheck, recheck_script, split_recheck, EmptyDir};
use crate::scan_script::{build, split_for_command_line, ScriptQuestion};

pub struct SshCollector<'a> {
    pub client: &'a mut SshClient,
    pub runtime: &'a tokio::runtime::Runtime,
    /// root_dir（設定に書いたもの）
    pub root: Vec<u8>,
    /// 一度の実行の時間切れ（[ssh] timeout_sec。REQ-scan-013）
    pub timeout_sec: u64,
}

impl Collector for SshCollector<'_> {
    fn collect(
        &mut self,
        questions: &[Question],
        exclude: &Exclude,
    ) -> Result<Vec<Answer>, CollectError> {
        let (client, runtime, timeout) = (&mut *self.client, self.runtime, self.timeout_sec);
        let run = |script: &str| runtime.block_on(client.run_scan_script(script, timeout));
        collect_with(
            &self.root,
            questions,
            exclude,
            SSH_BATCH_MAX_COMMAND_LEN,
            run,
        )
        .map_err(|failure| match failure {
            CollectFailure::Run(error) => CollectError(error),
            CollectFailure::Read(message) => {
                CollectError(anyhow::Error::from(AppError::RemoteParse { message }))
            }
        })
    }
}

/// 走査のスクリプトを動かせなかったか、出力を読めなかった
#[derive(Debug)]
pub enum CollectFailure<E> {
    Run(E),
    Read(String),
}

/// 問いをコマンド行に収まるまとまりにして `run` で動かし（標準出力と標準エラーを返す）、
/// 空に見えたディレクトリにはもう一度まとめて尋ねてから、問いごとの答えにする
pub fn collect_with<E>(
    root: &[u8],
    questions: &[Question],
    exclude: &Exclude,
    limit: usize,
    mut run: impl FnMut(&str) -> Result<(Vec<u8>, Vec<u8>), E>,
) -> Result<Vec<Answer>, CollectFailure<E>> {
    let prunable = exclude.prunable();
    let script_questions: Vec<ScriptQuestion> = questions
        .iter()
        .map(|q| ScriptQuestion {
            path: &q.path,
            descend: q.descend,
        })
        .collect();
    let mut answers = Vec::with_capacity(questions.len());
    let mut empty = Vec::new();
    for range in split_for_command_line(root, &script_questions, &prunable, limit) {
        let part = &script_questions[range];
        let (stdout, stderr) = run(&build(root, part, &prunable)).map_err(CollectFailure::Run)?;
        let parsed = parse(&stdout, &stderr, part).map_err(CollectFailure::Read)?;
        let offset = answers.len();
        empty.extend(parsed.empty.into_iter().map(|dir| EmptyDir {
            answer: dir.answer + offset,
            ..dir
        }));
        answers.extend(parsed.answers);
    }
    if empty.is_empty() {
        return Ok(answers);
    }
    let mut printed: Vec<&[u8]> = empty.iter().map(|dir| dir.printed.as_slice()).collect();
    printed.sort();
    printed.dedup();
    let mut checks = Vec::new();
    for range in split_recheck(root, &printed, limit) {
        let (stdout, stderr) =
            run(&recheck_script(root, &printed[range.clone()])).map_err(CollectFailure::Run)?;
        match read_recheck(&stdout, &stderr).map_err(CollectFailure::Read)? {
            Some(group) => checks.extend(group),
            // root_dir にもう入れない。尋ねたディレクトリはどれも変わったものにする
            None => checks.extend(printed[range].iter().map(|path| gone(path))),
        }
    }
    let paths: Vec<&[u8]> = questions.iter().map(|q| q.path.as_slice()).collect();
    apply_recheck(&mut answers, &empty, &checks, &paths, exclude).map_err(CollectFailure::Read)?;
    Ok(answers)
}

fn gone(printed: &[u8]) -> DirCheck {
    DirCheck {
        printed: printed.to_vec(),
        readable: false,
        searchable: false,
        gone: true,
        names: Vec::new(),
    }
}
