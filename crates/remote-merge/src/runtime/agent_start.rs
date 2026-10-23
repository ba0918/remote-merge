//! エージェントを配置して起動し、通信できるかを確かめる（docs/ir/ssh/deploy.md）。使えないときは
//! 経路の選び方に渡すエージェントの状態を返し、ここでは経路を選ばない

#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
use crate::agent::client::AgentClient;
#[cfg(unix)]
use crate::agent::deploy::{self, VersionCheck};
#[cfg(unix)]
use crate::agent::protocol::{CLI_VERSION, PROTOCOL_VERSION};
#[cfg(unix)]
use crate::agent::ssh_transport;

#[cfg(unix)]
use remote_merge_core::route_choice::AgentState;

use super::agent_clients::ConnectedAgent;
#[cfg(unix)]
use super::agent_target::{cannot_deploy, cannot_start, not_connected};
use super::core::CoreRuntime;
#[cfg(unix)]
use super::route_state::require_ssh_client;

impl CoreRuntime {
    /// SSH exec 経由で Agent プロセスを起動し、つないだエージェントを返す。
    ///
    /// 1. sudo=true なら、sudo を使えるかと既定の所有者を確かめる
    /// 2. エージェントを配置済みにする（`ensure_agent_deployed`）
    /// 3. エージェントを起動してつなぐ（`launch_agent`）
    ///
    /// 使えなければ、経路の選び方（TBL-ssh-001）に渡すエージェントの状態を返す
    #[cfg(unix)]
    pub(crate) fn start_agent_via_ssh(
        &mut self,
        server_name: &str,
    ) -> Result<ConnectedAgent, AgentState> {
        // サーバー設定を取得（借用を先に解決）
        let server_config = self
            .get_server_config(server_name)
            .map_err(|error| cannot_start(&error))?;
        let user = server_config.user.clone();
        let root_dir = server_config.root_dir.to_string_lossy().to_string();
        let sudo = server_config.sudo;

        // sudo=true の場合: pre-flight チェック
        let (default_uid, default_gid) = if sudo {
            self.sudo_preflight(server_name, &user)
                .map_err(|error| cannot_start(&error))?
        } else {
            (None, None)
        };

        let remote_path = self.ensure_agent_deployed(server_name, &user, sudo)?;
        let agent_command =
            deploy::build_agent_command(&remote_path, &root_dir, sudo, default_uid, default_gid);
        self.launch_agent(server_name, &agent_command)
    }

    /// 配置先のディレクトリの検査 → バージョンチェック → 不一致・未配置・中身の違いなら配置し、
    /// 配置先のバイナリのパスを返す
    #[cfg(unix)]
    fn ensure_agent_deployed(
        &mut self,
        server_name: &str,
        user: &str,
        sudo: bool,
    ) -> Result<PathBuf, AgentState> {
        // リモートバイナリパスを計算
        let remote_path = deploy::remote_binary_path(&self.config.agent.deploy_dir, user);
        tracing::debug!(
            "Agent deploy target: server={}, path={}",
            server_name,
            remote_path.display()
        );

        // 配置先の中のもの（版の確認を含む）を実行する前に、他人が差し替えられない場所かを確かめる
        let dir_check = deploy::build_deploy_dir_check_command(&remote_path, sudo);
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)
            .map_err(|error| cannot_deploy(&error))?;
        let dir_check_output = self
            .rt
            .block_on(ssh_client.exec(&dir_check))
            .map_err(|error| cannot_deploy(&error))?;
        let deploy_dir_state = deploy::judge_deploy_dir(&dir_check_output);
        let deploy_dir_display = remote_path.parent().unwrap_or(&remote_path).display();
        match deploy_dir_state {
            deploy::DeployDir::Unsafe => {
                return Err(AgentState::UnsafeLocation {
                    location: deploy_dir_display.to_string(),
                });
            }
            // 利用者に向けた警告は、記録の細かさによらず標準エラーに出す（F51）
            deploy::DeployDir::Repaired => tracing::warn!(
                target: "remote_merge::warning",
                "Agent deploy directory {} on {} was writable by group or others: changed its mode to 700 and placing the agent again",
                deploy_dir_display,
                server_name
            ),
            deploy::DeployDir::Safe => {}
        }
        // 直した配置先の中身は信用できないので、版の確認でも実行せずに置き直す
        let repaired = deploy_dir_state == deploy::DeployDir::Repaired;

        // uname + version check を1回の SSH exec で実行
        let uname_cmd = deploy::detect_remote_target_command();
        let combined = if repaired {
            format!("{{ {uname_cmd}; }} 2>/dev/null")
        } else {
            let version_cmd = deploy::check_version_command(&remote_path, sudo);
            format!("{{ {uname_cmd}; }} 2>/dev/null; {version_cmd}")
        };
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)
            .map_err(|error| cannot_deploy(&error))?;
        let combined_output = self
            .rt
            .block_on(ssh_client.exec(&combined))
            .map_err(|error| cannot_deploy(&error))?;
        let (uname_result, parsed_version_check) =
            deploy::parse_uname_and_version(&combined_output, CLI_VERSION);
        let version_check = if repaired {
            VersionCheck::NotFound
        } else {
            parsed_version_check
        };

        let remote_target = super::agent_target::remote_target_of(server_name, uname_result)?;
        super::agent_target::log_version_check(server_name, &version_check);

        // 版が一致しても、ローカルのバイナリと中身が違えば実行せずに置き直す
        let needs_deploy = match version_check {
            VersionCheck::Match => {
                self.installed_binary_check(server_name, &remote_path, remote_target, sudo)
                    .map_err(|error| cannot_deploy(&error))?
                    == deploy::InstalledBinary::Redeploy
            }
            VersionCheck::Mismatch { .. } | VersionCheck::NotFound => true,
        };
        if needs_deploy {
            self.deploy_agent_binary(server_name, &remote_path, sudo, remote_target)
                .map_err(|error| cannot_deploy(&error))?;
        }
        Ok(remote_path)
    }

    /// `agent_command` を SSH exec チャネルで実行し、ssh_transport でブリッジスレッドを起動して
    /// AgentClient::connect でハンドシェイクする。ブリッジスレッドのガードはクライアントと一緒に返す
    #[cfg(unix)]
    fn launch_agent(
        &mut self,
        server_name: &str,
        agent_command: &str,
    ) -> Result<ConnectedAgent, AgentState> {
        tracing::debug!(
            "Starting agent: server={}, command={}",
            server_name,
            agent_command
        );
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)
            .map_err(|error| cannot_start(&error))?;
        let channel = self
            .rt
            .block_on(ssh_client.open_exec_channel(agent_command))
            .map_err(|error| cannot_start(&error))?;

        // ブリッジスレッドを起動し、UnixStream ペアを取得
        let handle = self.rt.handle().clone();
        // 読みの時間切れは、ssh の接続の時間切れと同じ値に、黙っていてよい時間の下限を当てる
        // （大きな走査の間はエージェントが黙って計算するので、短くすると健全な処理を切ってしまう。
        // `接続断` の時間切れ。REQ-ssh-003・REQ-ssh-025）
        let read_timeout = remote_merge_ssh::silence_timeout(self.config.ssh.timeout_sec);
        let (read_stream, write_stream, guard) =
            ssh_transport::start(handle, channel, read_timeout)
                .map_err(|error| cannot_start(&error))?;

        // AgentClient を接続（ハンドシェイク）
        let client = AgentClient::connect(read_stream, write_stream)
            .map_err(|error| not_connected(&error))?;
        tracing::info!(
            "Agent client connected: server={}, protocol_version={}",
            server_name,
            PROTOCOL_VERSION
        );

        Ok(ConnectedAgent {
            client,
            _guard: Some(guard),
        })
    }
}
