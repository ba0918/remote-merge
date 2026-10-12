//! SSH の経路の走査の集め方。問いをまとめてスクリプトにし、一度の SSH の実行で答えさせる
//! （docs/decision/records/2026-10-12-foundation-2.md の C7）。

use remote_merge_core::error::AppError;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::raw::{Answer, CollectError, Collector, Question};

use crate::batch_read::SSH_BATCH_MAX_COMMAND_LEN;
use crate::client::SshClient;
use crate::scan_output::parse;
use crate::scan_script::{build, split_for_command_line, ScriptQuestion};
use crate::scan_search::complete_unsearchable;

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
        let prunable = exclude.prunable();
        let script_questions: Vec<ScriptQuestion> = questions
            .iter()
            .map(|q| ScriptQuestion {
                path: &q.path,
                descend: q.descend,
            })
            .collect();
        let mut answers = Vec::with_capacity(questions.len());
        let ranges = split_for_command_line(
            &self.root,
            &script_questions,
            &prunable,
            SSH_BATCH_MAX_COMMAND_LEN,
        );
        for range in ranges {
            let part = &script_questions[range];
            let script = build(&self.root, part, &prunable);
            let (stdout, stderr) = self
                .runtime
                .block_on(self.client.run_scan_script(&script, self.timeout_sec))?;
            let mut parsed = parse(&stdout, &stderr, part)
                .map_err(|message| anyhow::Error::from(AppError::RemoteParse { message }))?;
            let (client, runtime, timeout_sec) =
                (&mut *self.client, self.runtime, self.timeout_sec);
            complete_unsearchable(&self.root, part, &mut parsed, |script| {
                let (stdout, _) = runtime.block_on(client.run_scan_script(script, timeout_sec))?;
                Ok(stdout)
            })?;
            answers.extend(parsed);
        }
        Ok(answers)
    }
}
