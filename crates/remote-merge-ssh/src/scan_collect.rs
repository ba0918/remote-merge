//! SSH の経路の走査の集め方。問いをまとめてスクリプトにし、一度の SSH の実行で答えさせる
//! （docs/decision/records/2026-10-12-foundation-2.md の C7）。

use remote_merge_core::error::AppError;
use remote_merge_core::op_failure::{OpError, OpFailure, Operation, Tool};
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::raw::{Answer, CollectError, Collector, Question};

use crate::client::SshClient;
use crate::command_limit::MAX_COMMAND_LEN as SSH_BATCH_MAX_COMMAND_LEN;
use crate::scan_find_probe::find_unusable;
use crate::scan_output::parse;
use crate::scan_recheck::{
    apply_recheck, match_round, read_recheck, recheck_script, split_recheck, EmptyDir,
};
use crate::scan_script::{build, root_gone, split_for_command_line, ScriptQuestion};

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
            CollectFailure::Run(error) => CollectError::Failed(error),
            CollectFailure::Read(message) => {
                CollectError::Failed(anyhow::Error::from(AppError::RemoteParse { message }))
            }
            CollectFailure::RootGone => CollectError::RootGone,
            CollectFailure::ToolMissing(tool) => CollectError::Failed(
                OpError::new(
                    Operation::Scan,
                    String::from_utf8_lossy(&self.root).into_owned(),
                    OpFailure::ToolMissing(tool),
                )
                .into(),
            ),
        })
    }
}

/// 走査のスクリプトを動かせなかったか、出力を読めなかったか、root_dir がなくなった
#[derive(Debug)]
pub enum CollectFailure<E> {
    Run(E),
    Read(String),
    /// 走査を始めたときにはあった root_dir が、もうないかディレクトリでない
    RootGone,
    /// 走査に要る道具がない（docs/ir/ssh/compatibility.md の REQ-ssh-026）
    ToolMissing(Tool),
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
            depth: q.depth,
        })
        .collect();
    let mut answers = Vec::with_capacity(questions.len());
    let mut empty = Vec::new();
    for range in split_for_command_line(root, &script_questions, &prunable, limit) {
        let part = &script_questions[range];
        let (stdout, stderr) = run(&build(root, part, &prunable)).map_err(CollectFailure::Run)?;
        if find_unusable(&stdout) {
            return Err(CollectFailure::ToolMissing(Tool::FindPrintf));
        }
        // root_dir が途中で消えたら、ほかの知らせによらずそれで答える
        if root_gone(&stdout) {
            return Err(CollectFailure::RootGone);
        }
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
        let asked = &printed[range];
        let (stdout, stderr) = run(&recheck_script(root, asked)).map_err(CollectFailure::Run)?;
        if root_gone(&stdout) {
            return Err(CollectFailure::RootGone);
        }
        // root_dir にもう入れないときも、root_dir がなくなったものとして答える
        let group = read_recheck(&stdout, &stderr)
            .map_err(CollectFailure::Read)?
            .ok_or(CollectFailure::RootGone)?;
        checks.extend(match_round(asked, group).map_err(CollectFailure::Read)?);
    }
    let paths: Vec<&[u8]> = questions.iter().map(|q| q.path.as_slice()).collect();
    apply_recheck(&mut answers, &empty, &checks, &paths, exclude).map_err(CollectFailure::Read)?;
    Ok(answers)
}

#[cfg(all(test, unix))]
#[path = "scan_root_gone_tests.rs"]
mod root_gone_tests;

#[cfg(all(test, unix))]
#[path = "scan_children_tests.rs"]
mod children_tests;
