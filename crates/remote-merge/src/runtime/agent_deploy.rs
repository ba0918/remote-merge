//! エージェントのバイナリの配置（docs/ir/ssh/deploy.md）: 配置済みのバイナリの照合、sudo の前の
//! 確かめ、配置

use crate::agent::deploy;

use super::core::CoreRuntime;
use super::route_state::require_ssh_client;

impl CoreRuntime {
    /// 版が一致した配置済みのバイナリを、ローカルの Agent のバイナリと SHA-256 で照合する。
    ///
    /// ローカルのバイナリを解決できないときは照合できないので、配置先の検査を通ったことだけを
    /// 根拠に実行する（利用者が手で置いた Agent を使えるようにするため）。解決できたのに読めない
    /// ときは照合せずに動かさず、配置できない誤りにする。
    #[cfg(unix)]
    pub(super) fn installed_binary_check(
        &mut self,
        server_name: &str,
        remote_path: &std::path::Path,
        remote_target: &str,
        sudo: bool,
    ) -> anyhow::Result<deploy::InstalledBinary> {
        let local_hash = super::agent_local_hash::local_agent_hash(
            deploy::resolve_agent_binary(remote_target).map(|resolved| resolved.path),
            |path| std::fs::File::open(path),
        )?;
        let Some(local_hash) = local_hash else {
            tracing::info!(
                "Local agent binary unavailable; running the installed agent unchecked: server={}",
                server_name
            );
            return Ok(deploy::judge_installed_binary(None, ""));
        };
        let checksum_cmd = deploy::installed_checksum_command(remote_path, sudo);
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        let output = self.rt.block_on(ssh_client.exec(&checksum_cmd))?;
        let judgement = deploy::judge_installed_binary(Some(&local_hash), &output);
        match judgement {
            deploy::InstalledBinary::Redeploy => tracing::info!(
                "Installed agent binary differs from the local one: server={}",
                server_name
            ),
            deploy::InstalledBinary::RunUnverified => tracing::warn!(
                target: "remote_merge::warning",
                "Cannot compute SHA-256 on {}: running the installed agent {} without comparing it with the local binary",
                server_name,
                remote_path.display()
            ),
            deploy::InstalledBinary::Run => {}
        }
        Ok(judgement)
    }

    /// sudo=true 時の pre-flight チェック。
    ///
    /// 1. NOPASSWD が設定されているか確認
    /// 2. ユーザーの uid/gid を取得
    #[cfg(unix)]
    pub(super) fn sudo_preflight(
        &mut self,
        server_name: &str,
        user: &str,
    ) -> anyhow::Result<(Option<u32>, Option<u32>)> {
        // ホスト名を先に取得（借用分離）
        let host = self
            .config
            .servers
            .get(server_name)
            .map(|s| s.host.clone())
            .unwrap_or_else(|| server_name.to_string());

        // sudo NOPASSWD チェック
        let sudo_cmd = deploy::build_sudo_check_command();
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        if let Err(error) = self.rt.block_on(ssh_client.exec_strict(sudo_cmd)) {
            // 接続が使えなかった失敗は sudo の設定の誤りではないので、種類を残して返す
            // （起動し直しが再接続に進めるように）
            if super::connection::classify(&error).is_disconnection() {
                return Err(error);
            }
            anyhow::bail!(
                "sudo requires NOPASSWD to be configured for user '{}' on {}. \
                 Add to /etc/sudoers: {} ALL=(ALL) NOPASSWD: ALL",
                user,
                host,
                user,
            );
        }

        // uid/gid 取得
        let id_cmd = deploy::build_id_command(user);
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        let id_output = self.rt.block_on(ssh_client.exec(&id_cmd))?;
        let (uid, gid) = deploy::parse_id_output(&id_output)
            .map_err(|e| anyhow::anyhow!("failed to get uid/gid for user '{}': {}", user, e))?;

        tracing::info!(
            "sudo preflight passed: server={}, user={}, uid={}, gid={}",
            server_name,
            user,
            uid,
            gid
        );
        Ok((Some(uid), Some(gid)))
    }

    /// リモートサーバにエージェントバイナリをデプロイする（atomic write 方式）。
    ///
    /// 2 exec + 1 write の3ステップで実行する:
    /// 1. `build_pre_write_command`: mkdir + symlink チェックを1 exec で実施
    /// 2. `write_file_bytes`: .tmp パスにバイナリ書き込み
    /// 3. `build_post_write_script`: chmod + checksum + version + mv を1 exec で実施
    ///
    /// 検証失敗時はスクリプト内で `.tmp` を削除し、本番バイナリは無傷のまま。
    #[cfg(unix)]
    pub(super) fn deploy_agent_binary(
        &mut self,
        server_name: &str,
        remote_path: &std::path::Path,
        sudo: bool,
        remote_target: &str,
    ) -> anyhow::Result<()> {
        let tmp_path = format!("{}.tmp", remote_path.display());

        // リモートターゲットに対応するバイナリを解決
        let resolved = deploy::resolve_agent_binary(remote_target)?;
        tracing::info!(
            "Resolved agent binary: source={:?}, path={}",
            resolved.source,
            resolved.path.display()
        );
        let local_path = resolved.path;
        let metadata = std::fs::metadata(&local_path)?;
        if deploy::is_debug_binary(metadata.len()) {
            tracing::warn!(
                "Deploying a likely debug binary ({:.1} MB). Consider using a release build.",
                metadata.len() as f64 / (1024.0 * 1024.0)
            );
        }
        tracing::info!(
            "Deploying agent binary: {} -> {}:{} (via atomic write)",
            local_path.display(),
            server_name,
            remote_path.display()
        );
        let binary_bytes = std::fs::read(&local_path)?;
        let local_hash = remote_merge_core::sha256::hex_of_bytes(&binary_bytes);

        // Step 1: mkdir + symlink チェック（1 exec）
        let pre_cmd = deploy::build_pre_write_command(remote_path, sudo);
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        let pre_output = self.rt.block_on(ssh_client.exec(&pre_cmd))?;
        if !pre_output.contains("OK") {
            // fail-closed: "OK" を含まない出力は全て拒否する（SYMLINK、空出力、エラーメッセージ等）
            if pre_output.contains("SYMLINK") {
                anyhow::bail!(
                    "Security check failed: {} is a symlink on {}",
                    remote_path.display(),
                    server_name
                );
            }
            anyhow::bail!(
                "Pre-write check failed: unexpected output '{}' for {} on {}",
                pre_output.trim(),
                remote_path.display(),
                server_name
            );
        }

        // Step 2: .tmp パスに書き込み（標準入力の base64 を openssl で復号する）
        let upload = format!(
            "{}openssl base64 -d -A -out {}",
            if sudo { "sudo " } else { "" },
            remote_merge_ssh::quote::shell_escape(tmp_path.as_str())
        );
        let input = base64_input(&binary_bytes);
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        let (_, stderr, exit) = self
            .rt
            .block_on(ssh_client.run_with_input(&upload, &input))?;
        if exit != 0 {
            anyhow::bail!(
                "Agent upload failed: server={}, path={}, exit={exit}, stderr={}",
                server_name,
                tmp_path.as_str(),
                String::from_utf8_lossy(&stderr).trim_end()
            );
        }

        // Step 3: chmod + checksum + version + mv（1 exec）
        let post_script =
            deploy::build_post_write_script(remote_path, &tmp_path, &local_hash, sudo)?;
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        self.rt
            .block_on(ssh_client.exec_strict(&post_script))
            .map_err(|e| {
                anyhow::anyhow!(
                    "Agent deploy post-write script failed: server={}, path={}, error={}",
                    server_name,
                    remote_path.display(),
                    e
                )
            })?;

        tracing::info!("Agent binary deployed successfully: server={}", server_name);
        Ok(())
    }
}

/// 配置する実行ファイルを、openssl の base64 の復号に渡す標準入力にする
fn base64_input(binary: &[u8]) -> Vec<u8> {
    use base64::Engine;
    let mut encoded = base64::engine::general_purpose::STANDARD
        .encode(binary)
        .into_bytes();
    encoded.push(b'\n');
    encoded
}
