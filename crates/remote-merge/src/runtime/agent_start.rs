//! エージェントを配置して起動し、通信できるかを確かめる（docs/ir/ssh/deploy.md）。使えないときは
//! 経路の選び方に渡すエージェントの状態を返し、ここでは経路を選ばない

#[cfg(unix)]
use crate::agent::client::AgentClient;
#[cfg(unix)]
use crate::agent::deploy::{self, VersionCheck};
#[cfg(unix)]
use crate::agent::ssh_transport::SshAgentTransport;
#[cfg(unix)]
use remote_merge_core::route_choice::AgentState;

#[cfg(unix)]
use super::agent_target::{cannot_deploy, cannot_start, not_connected};
use super::core::{BoxedAgentClient, CoreRuntime};
#[cfg(unix)]
use super::route_state::require_ssh_client;

impl CoreRuntime {
    /// SSH exec 経由で Agent プロセスを起動し、AgentClient を返す。
    ///
    /// 1. 配置先のディレクトリの検査 → バージョンチェック → 不一致または未配置なら自動デプロイ
    /// 2. SSH exec チャネルで Agent プロセスを起動
    /// 3. SshAgentTransport でブリッジスレッドを起動し、UnixStream ペアを取得
    /// 4. AgentClient::connect でハンドシェイク
    ///
    /// 使えなければ、経路の選び方（TBL-ssh-001）に渡すエージェントの状態を返す
    #[cfg(unix)]
    pub(crate) fn start_agent_via_ssh(
        &mut self,
        server_name: &str,
    ) -> Result<BoxedAgentClient, AgentState> {
        // サーバー設定を取得（借用を先に解決）
        let server_config = self.get_server_config(server_name).map_err(cannot_start)?;
        let user = server_config.user.clone();
        let root_dir = server_config.root_dir.to_string_lossy().to_string();
        let sudo = server_config.sudo;
        let deploy_dir = self.config.agent.deploy_dir.clone();

        // sudo=true の場合: pre-flight チェック
        let (default_uid, default_gid) = if sudo {
            self.sudo_preflight(server_name, &user)
                .map_err(cannot_start)?
        } else {
            (None, None)
        };

        // リモートバイナリパスを計算
        let remote_path = deploy::remote_binary_path(&deploy_dir, &user);
        tracing::debug!(
            "Agent deploy target: server={}, path={}",
            server_name,
            remote_path.display()
        );

        // 配置先の中のもの（版の確認を含む）を実行する前に、他人が差し替えられない場所かを確かめる
        let dir_check = deploy::build_deploy_dir_check_command(&remote_path, sudo);
        let ssh_client =
            require_ssh_client(&mut self.ssh_clients, server_name).map_err(cannot_deploy)?;
        let dir_check_output = self
            .rt
            .block_on(ssh_client.exec(&dir_check))
            .map_err(cannot_deploy)?;
        let deploy_dir_state = deploy::judge_deploy_dir(&dir_check_output);
        let deploy_dir_display = remote_path.parent().unwrap_or(&remote_path).display();
        match deploy_dir_state {
            deploy::DeployDir::Unsafe => {
                return Err(AgentState::UnsafeLocation {
                    location: deploy_dir_display.to_string(),
                })
            }
            deploy::DeployDir::Repaired => tracing::warn!(
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
        let ssh_client =
            require_ssh_client(&mut self.ssh_clients, server_name).map_err(cannot_deploy)?;
        let combined_output = self
            .rt
            .block_on(ssh_client.exec(&combined))
            .map_err(cannot_deploy)?;
        let (uname_result, parsed_version_check) =
            deploy::parse_uname_and_version(&combined_output);
        let version_check = if repaired {
            VersionCheck::NotFound
        } else {
            parsed_version_check
        };

        let remote_target = super::agent_target::remote_target_of(server_name, uname_result);
        super::agent_target::log_version_check(server_name, &version_check);

        // 版が一致しても、ローカルのバイナリと中身が違えば実行せずに置き直す
        let needs_deploy = match version_check {
            VersionCheck::Match => {
                self.installed_binary_check(server_name, &remote_path, remote_target, sudo)
                    .map_err(cannot_deploy)?
                    == deploy::InstalledBinary::Redeploy
            }
            VersionCheck::Mismatch { .. } | VersionCheck::NotFound => true,
        };
        if needs_deploy {
            self.deploy_agent_binary(server_name, &remote_path, sudo, remote_target)
                .map_err(cannot_deploy)?;
        }

        // Agent プロセスを起動
        let agent_command =
            deploy::build_agent_command(&remote_path, &root_dir, sudo, default_uid, default_gid);
        tracing::debug!(
            "Starting agent: server={}, command={}",
            server_name,
            agent_command
        );
        let ssh_client =
            require_ssh_client(&mut self.ssh_clients, server_name).map_err(cannot_start)?;
        let channel = self
            .rt
            .block_on(ssh_client.open_exec_channel(&agent_command))
            .map_err(cannot_start)?;

        // ブリッジスレッドを起動し、UnixStream ペアを取得
        let handle = self.rt.handle().clone();
        let transport = SshAgentTransport::start(handle, channel).map_err(cannot_start)?;
        let (read_stream, write_stream, guard) = transport.into_streams();

        // AgentClient を接続（ハンドシェイク）
        let client = AgentClient::connect(read_stream, write_stream).map_err(not_connected)?;
        tracing::info!(
            "Agent client connected: server={}, protocol_version={}",
            server_name,
            client.protocol_version()
        );

        // TransportGuard を保持してブリッジスレッドのライフサイクルを管理
        self.transport_guards.insert(server_name.to_string(), guard);

        Ok(client)
    }

    /// Unix 以外のプラットフォームではエージェントは利用不可
    #[cfg(not(unix))]
    pub(crate) fn start_agent_via_ssh(
        &mut self,
        _server_name: &str,
    ) -> Result<BoxedAgentClient, AgentState> {
        Err(AgentState::CannotStart {
            reason: "the agent runs only on Unix platforms".to_string(),
        })
    }
}
