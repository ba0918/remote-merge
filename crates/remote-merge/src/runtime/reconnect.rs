//! `接続断` の扱い（docs/ir/ssh/reconnect.md の REQ-ssh-003・REQ-ssh-004・REQ-ssh-023）。
//!
//! - 読み取りは同じ経路で一度だけやり直す。エージェントは起動し直し、SSH は再接続する
//! - 変更はどの `接続断` でも繰り返さず、別の経路でも行わない
//! - やり直してもだめなとき、変更の `接続断` のとき、送る前に接続が使えなかったときは、
//!   そのサーバへの処理を止める（以降の操作は行わずに失敗にする）。変更の `接続断` の後の
//!   書き込み先の確かめは `confirm` が行う

use super::connection::{classify, Failure, ProcessingStopped};
use super::core::{BoxedAgentClient, CoreRuntime};

/// エージェントの操作が読み取りか変更か
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Access {
    Read,
    Modify,
}

/// エージェントの操作の結果
pub(crate) enum AgentAnswer<T> {
    /// 成功か、接続が使えなくなった失敗（読み取りはやり直した後のもの）
    Done(anyhow::Result<T>),
    /// エージェントが失敗を返した
    Refused(anyhow::Error),
}

impl CoreRuntime {
    /// エージェントがあれば `operation` を行う。なければ None。読み取りの `接続断` は、
    /// エージェントを起動し直して一度だけやり直す。変更はやり直さない
    pub(crate) fn agent_call<T>(
        &mut self,
        server_name: &str,
        access: Access,
        mut operation: impl FnMut(&mut BoxedAgentClient) -> anyhow::Result<T>,
    ) -> Option<AgentAnswer<T>> {
        let error = match self.agent_call_once(server_name, &mut operation)? {
            Ok(value) => return Some(AgentAnswer::Done(Ok(value))),
            Err(error) => error,
        };
        if classify(&error) == Failure::Refused {
            return Some(AgentAnswer::Refused(error));
        }
        if access == Access::Modify {
            return Some(AgentAnswer::Done(Err(error)));
        }
        tracing::warn!(
            "Agent connection lost for {}; restarting the agent and trying once more: {:#}",
            server_name,
            error
        );
        if let Err(restart) = self.restart_agent(server_name) {
            let message = format!("{error}; the agent could not be restarted: {restart:#}");
            return Some(AgentAnswer::Done(Err(error.context(message))));
        }
        Some(match self.agent_call_once(server_name, &mut operation) {
            None => AgentAnswer::Done(Err(error)),
            Some(Ok(value)) => AgentAnswer::Done(Ok(value)),
            Some(Err(again)) if classify(&again) == Failure::Refused => AgentAnswer::Refused(again),
            Some(Err(again)) => AgentAnswer::Done(Err(again)),
        })
    }

    pub(crate) fn agent_call_once<T>(
        &mut self,
        server_name: &str,
        operation: &mut impl FnMut(&mut BoxedAgentClient) -> anyhow::Result<T>,
    ) -> Option<anyhow::Result<T>> {
        let agent = self.agent_clients.get(server_name)?.clone();
        let Ok(mut agent) = agent.lock() else {
            // 使っている途中で止まった接続なので、通信路が壊れたものとして扱う
            return Some(Err(std::io::Error::other(format!(
                "agent connection for {server_name} is unusable (lock poisoned)"
            ))
            .into()));
        };
        Some(operation(&mut agent))
    }

    /// エージェントを一度だけ起動し直す。SSH の session も切れていれば、再接続してから起動する
    /// （再接続がエージェントを起動する）。起動し直せなければ Err
    pub(crate) fn restart_agent(&mut self, server_name: &str) -> anyhow::Result<()> {
        self.agent_clients.remove(server_name);
        #[cfg(unix)]
        self.transport_guards.remove(server_name);
        match self.start_agent_via_ssh(server_name) {
            Ok(client) => {
                self.agent_clients.insert(
                    server_name.to_string(),
                    std::sync::Arc::new(std::sync::Mutex::new(client)),
                );
                tracing::info!("Agent restarted: server={}", server_name);
                Ok(())
            }
            Err(error) if classify(&error).is_disconnection() => {
                self.try_reconnect(server_name)?;
                if !self.has_agent(server_name) {
                    anyhow::bail!("the agent did not start after reconnecting to {server_name}");
                }
                tracing::info!("Agent restarted after reconnecting: server={}", server_name);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// SSH の読み取り。`接続断` なら再接続して一度だけやり直す
    pub(crate) fn ssh_read<T>(
        &mut self,
        server_name: &str,
        mut read: impl FnMut(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        match read(self) {
            Err(error) if classify(&error).is_disconnection() => {
                tracing::info!(
                    "SSH read failed (connection lost), reconnecting once: server={}: {:#}",
                    server_name,
                    error
                );
                if let Err(reconnect) = self.try_reconnect(server_name) {
                    let message = format!("{error}; could not reconnect: {reconnect:#}");
                    return Err(reconnect.context(message));
                }
                read(self)
            }
            other => other,
        }
    }

    /// `result` が接続の使えない失敗なら、そのサーバへの処理を止める
    pub(crate) fn stop_on_disconnection<T>(
        &mut self,
        server_name: &str,
        result: anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        if let Err(error) = &result {
            if classify(error).is_disconnection() && !self.stopped.contains_key(server_name) {
                tracing::warn!(
                    "Stopping further operations on {} after the connection was lost: {:#}",
                    server_name,
                    error
                );
                self.stopped
                    .insert(server_name.to_string(), format!("{error:#}"));
            }
        }
        result
    }

    /// どれかのサーバへの処理を `接続断` で止めたか（その回は終了コード 2 で終える）
    pub fn has_stopped(&self) -> bool {
        !self.stopped.is_empty()
    }

    /// `side` が処理を止めたサーバなら、行わなかったことを示すエラー
    pub fn stopped_error_for(&self, side: &crate::side::Side) -> Option<anyhow::Error> {
        side.server_name()
            .and_then(|server_name| self.stopped_error(server_name))
    }

    /// 処理を止めたサーバなら、行わなかったことを示すエラー
    pub(crate) fn stopped_error(&self, server_name: &str) -> Option<anyhow::Error> {
        self.stopped.get(server_name).map(|reason| {
            ProcessingStopped {
                server: server_name.to_string(),
                reason: reason.clone(),
            }
            .into()
        })
    }
}
