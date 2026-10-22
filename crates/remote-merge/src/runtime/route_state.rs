//! サーバごとの接続と、つなぐときに一度だけ決めるエージェントの扱い（docs/ir/ssh/fallback.md の
//! TBL-ssh-001）。
//!
//! リモートのファイルの操作はエージェントだけで行う。初めてつなぐときに `choose_route` で使うか止めるかを
//! 決め、その後は操作の失敗でも、つなぎ直しでも決め直さない（REQ-ssh-010・REQ-ssh-024）。

use std::collections::HashMap;

use remote_merge_core::route_choice::{AgentState, RouteChoice, choose_route};

use super::core::CoreRuntime;
use crate::ssh::client::SshClient;
use crate::ssh::passphrase_provider::CliPassphraseProvider;

impl CoreRuntime {
    /// サーバにつなぐ。初めてならエージェントを使えるかを決め、使えなければファイルを読み書きする前に
    /// 止める。つなぎ直しならエージェントを起動し直す
    pub fn connect(&mut self, server_name: &str) -> anyhow::Result<()> {
        if !self.config.agent.enabled && !self.agent_chosen.contains(server_name) {
            // エージェントを使わないと決まっているので、SSH につなぐ前に止める
            return self.settle_route(server_name, &AgentState::Disabled);
        }
        self.connect_ssh(server_name)?;
        if self.agent_chosen.contains(server_name) {
            return self.start_agent(server_name);
        }
        let state = self.agent_state(server_name);
        self.settle_route(server_name, &state)
    }

    /// エージェントを使うと決めたサーバか。つないでいなければ誤り
    pub(crate) fn require_agent_route(&self, server_name: &str) -> anyhow::Result<()> {
        if self.agent_chosen.contains(server_name) {
            Ok(())
        } else {
            anyhow::bail!("SSH not connected: {server_name}")
        }
    }

    fn connect_ssh(&mut self, server_name: &str) -> anyhow::Result<()> {
        let server_config = self.get_server_config(server_name)?;
        tracing::info!(
            "SSH connecting: server={}, host={}",
            server_name,
            server_config.host
        );
        let verifier = crate::ssh::host_key_verifier::verifier_from_policy(
            self.config.ssh.strict_host_key_checking,
            self.config.ssh.auto_yes,
        );
        match self.rt.block_on(SshClient::connect_with_verifier(
            server_name,
            server_config,
            &self.config.ssh,
            &*verifier,
            Some(&CliPassphraseProvider),
        )) {
            Ok(client) => {
                tracing::info!("SSH connected: server={}", server_name);
                self.ssh_clients.insert(server_name.to_string(), client);
                Ok(())
            }
            Err(e) => {
                tracing::error!("SSH connection failed: server={}, error={}", server_name, e);
                Err(e)
            }
        }
    }

    /// エージェントを配置して起動してみた結果の状態。起動できたらクライアントを持つ
    fn agent_state(&mut self, server_name: &str) -> AgentState {
        match self.start_agent_via_ssh(server_name) {
            Ok(agent) => {
                self.agent_clients.insert(server_name.to_string(), agent);
                AgentState::Ready
            }
            Err(state) => state,
        }
    }

    /// エージェントを使うと決めて記録する。止めるときは接続を切って接続のエラーにする
    fn settle_route(&mut self, server_name: &str, state: &AgentState) -> anyhow::Result<()> {
        match choose_route(state) {
            RouteChoice::Agent => {
                self.agent_chosen.insert(server_name.to_string());
                Ok(())
            }
            RouteChoice::Stop { reason } => {
                self.disconnect(server_name);
                anyhow::bail!("{server_name}: {reason}");
            }
        }
    }

    /// エージェントを起動し直す。起動できなければ誤り
    fn start_agent(&mut self, server_name: &str) -> anyhow::Result<()> {
        match self.agent_state(server_name) {
            AgentState::Ready => Ok(()),
            state => anyhow::bail!("{server_name}: the agent could not be started again: {state}"),
        }
    }

    /// エージェントのままつなぎ直す。まず今の SSH の接続で起動し直し、だめなら SSH からつなぎ直して
    /// 起動する（REQ-ssh-003）
    pub(crate) fn reconnect_route(&mut self, server_name: &str) -> anyhow::Result<()> {
        self.require_agent_route(server_name)?;
        if !self.has_client(server_name) {
            anyhow::bail!("the agent could not be restarted: SSH not connected: {server_name}");
        }
        self.disconnect_agent(server_name);
        let restart = match self.start_agent(server_name) {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        self.try_reconnect(server_name).map_err(|error| {
            error.context(format!(
                "the agent could not be restarted ({restart:#}), nor after reconnecting"
            ))
        })
    }

    /// SSH からつなぎ直す（エージェントを使うと決めたことは決め直さない）
    pub fn try_reconnect(&mut self, server_name: &str) -> anyhow::Result<()> {
        tracing::info!("Auto-reconnecting SSH: server={}", server_name);
        self.disconnect(server_name);
        self.connect(server_name)
    }

    /// 指定サーバの SSH クライアントが存在するか
    pub fn has_client(&self, server_name: &str) -> bool {
        self.ssh_clients.contains_key(server_name)
    }

    /// 指定サーバの接続を切断する（選んだ経路は覚えたままにする）
    pub fn disconnect(&mut self, server_name: &str) {
        self.disconnect_agent(server_name);
        if let Some(client) = self.ssh_clients.remove(server_name) {
            // 切る側の失敗は、使い終えた接続のことで扱いを変えないので、診断に残すだけにする
            if let Err(error) = self.rt.block_on(client.disconnect()) {
                tracing::debug!("SSH disconnect failed: server={server_name}, error={error:#}");
            }
        }
    }

    /// 全接続を切断する
    pub fn disconnect_all(&mut self) {
        let names: Vec<String> = self.ssh_clients.keys().cloned().collect();
        for name in names {
            self.disconnect(&name);
        }
    }
}

/// ssh_clients マップから指定サーバの SSH クライアントを取得する。`self.rt` と同時に借用するために
/// フリー関数にする
pub(crate) fn require_ssh_client<'a>(
    clients: &'a mut HashMap<String, SshClient>,
    server_name: &str,
) -> anyhow::Result<&'a mut SshClient> {
    clients
        .get_mut(server_name)
        .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))
}
