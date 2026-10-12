//! サーバごとの接続と、つなぐときに一度だけ選ぶ経路（docs/ir/ssh/fallback.md の TBL-ssh-001）。
//!
//! 経路は初めてつなぐときに `choose_route` で選び、その後は操作の失敗でも、つなぎ直しでも選び
//! 直さない（REQ-ssh-010・REQ-ssh-024）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use remote_merge_core::route_choice::{choose_route, AgentState, RouteChoice};

use super::core::CoreRuntime;
use crate::ssh::client::SshClient;

/// 選んだ経路
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteRoute {
    Agent,
    Ssh,
}

impl CoreRuntime {
    /// サーバにつなぐ。初めてなら経路を選び、つなぎ直しなら選んだ経路でつなぐ
    pub fn connect(&mut self, server_name: &str) -> anyhow::Result<()> {
        let sudo = self.get_server_config(server_name)?.sudo;
        if !self.config.agent.enabled && !self.routes.contains_key(server_name) {
            // エージェントを使わないと決まっているので、SSH につなぐ前に選ぶ
            self.settle_route(server_name, sudo, AgentState::Disabled)?;
            return self.connect_ssh(server_name);
        }
        self.connect_ssh(server_name)?;
        match self.routes.get(server_name).copied() {
            Some(RemoteRoute::Ssh) => Ok(()),
            Some(RemoteRoute::Agent) => self.start_agent(server_name),
            None => {
                let state = self.agent_state(server_name);
                self.settle_route(server_name, sudo, state)
            }
        }
    }

    /// 選んだ経路。つないでいなければ誤り
    pub(crate) fn remote_route(&self, server_name: &str) -> anyhow::Result<RemoteRoute> {
        self.routes
            .get(server_name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {server_name}"))
    }

    fn connect_ssh(&mut self, server_name: &str) -> anyhow::Result<()> {
        let server_config = self.get_server_config(server_name)?;
        tracing::info!(
            "SSH connecting: server={}, host={}",
            server_name,
            server_config.host
        );
        let provider = self.passphrase_provider.as_ref().map(|p| p.as_ref());
        match self.rt.block_on(SshClient::connect_with_passphrase(
            server_name,
            server_config,
            &self.config.ssh,
            provider,
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
            Ok(client) => {
                self.agent_clients
                    .insert(server_name.to_string(), Arc::new(Mutex::new(client)));
                AgentState::Ready
            }
            Err(state) => state,
        }
    }

    /// 経路を選んで記録する。止めるときは接続を切って接続のエラーにする
    fn settle_route(
        &mut self,
        server_name: &str,
        sudo: bool,
        state: AgentState,
    ) -> anyhow::Result<()> {
        let route = match choose_route(sudo, &state) {
            RouteChoice::Agent => RemoteRoute::Agent,
            RouteChoice::Ssh { warning } => {
                if let Some(warning) = warning {
                    tracing::warn!("{server_name}: {warning}");
                }
                RemoteRoute::Ssh
            }
            RouteChoice::Stop { reason } => {
                self.disconnect(server_name);
                anyhow::bail!("{server_name}: {reason}");
            }
        };
        self.routes.insert(server_name.to_string(), route);
        Ok(())
    }

    /// 選んだエージェントの経路でエージェントを起動し直す。起動できなければ SSH には切り替えずに誤り
    fn start_agent(&mut self, server_name: &str) -> anyhow::Result<()> {
        match self.agent_state(server_name) {
            AgentState::Ready => Ok(()),
            state => anyhow::bail!(
                "{server_name}: the agent could not be started again and SSH is not used instead: {state}"
            ),
        }
    }

    /// 選んだ経路のままつなぎ直す。エージェントはまず今の SSH の接続で起動し直し、だめなら SSH から
    /// つなぎ直して起動する（REQ-ssh-003・REQ-ssh-023）
    pub(crate) fn reconnect_route(&mut self, server_name: &str) -> anyhow::Result<()> {
        if self.remote_route(server_name)? == RemoteRoute::Ssh {
            return self.try_reconnect(server_name);
        }
        if !self.has_client(server_name) {
            anyhow::bail!("the agent could not be restarted: SSH not connected: {server_name}");
        }
        self.disconnect_agent(server_name);
        if self.start_agent(server_name).is_ok() {
            return Ok(());
        }
        self.try_reconnect(server_name)
            .map_err(|error| error.context("the agent could not be restarted"))
    }

    /// SSH からつなぎ直す（経路は選び直さない）
    pub fn try_reconnect(&mut self, server_name: &str) -> anyhow::Result<()> {
        tracing::info!("Auto-reconnecting SSH: server={}", server_name);
        self.disconnect(server_name);
        self.connect(server_name)
    }

    /// 指定サーバの SSH クライアントを取得する
    pub fn get_client(&mut self, server_name: &str) -> anyhow::Result<&mut SshClient> {
        require_ssh_client(&mut self.ssh_clients, server_name)
    }

    /// 指定サーバの SSH クライアントが存在するか
    pub fn has_client(&self, server_name: &str) -> bool {
        self.ssh_clients.contains_key(server_name)
    }

    /// 指定サーバの接続を切断する（選んだ経路は覚えたままにする）
    pub fn disconnect(&mut self, server_name: &str) {
        self.disconnect_agent(server_name);
        if let Some(client) = self.ssh_clients.remove(server_name) {
            let _ = self.rt.block_on(client.disconnect());
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
