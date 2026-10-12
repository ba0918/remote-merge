//! エージェントの起動の部品: 置くリモートのターゲットの決め方、配置済みの版の確かめの記録、
//! 起動できなかったときのエージェントの状態

use crate::agent::deploy::{self, VersionCheck};
use remote_merge_core::route_choice::AgentState;

/// uname の答えから、置くバイナリのターゲットを決める。分からなければ手元のターゲットにする
pub(super) fn remote_target_of(
    server_name: &str,
    uname_result: Option<anyhow::Result<&'static str>>,
) -> &'static str {
    match uname_result {
        None => {
            // uname 失敗: ローカルターゲットにフォールバック
            let fallback = deploy::current_target();
            tracing::warn!(
                "Failed to detect remote target via uname: server={}, falling back to local target '{}'",
                server_name,
                fallback
            );
            fallback
        }
        Some(Ok(target)) => {
            tracing::info!(
                "Detected remote target: server={}, target={}",
                server_name,
                target
            );
            target
        }
        Some(Err(e)) => {
            let local = deploy::current_target();
            tracing::warn!(
                "Unknown remote target: server={}, error={}, falling back to local target '{}'",
                server_name,
                e,
                local
            );
            local
        }
    }
}

/// 配置済みのバイナリの版の確かめの結果を記録する
pub(super) fn log_version_check(server_name: &str, version_check: &VersionCheck) {
    match version_check {
        VersionCheck::Match => {
            tracing::info!("Agent binary version matches: server={}", server_name);
        }
        VersionCheck::Mismatch { remote_version } => {
            tracing::info!(
                "Agent binary version mismatch: server={}, remote={}",
                server_name,
                remote_version
            );
        }
        VersionCheck::NotFound => {
            tracing::info!("Agent binary not found: server={}", server_name);
        }
    }
}

/// 配置できなかった
pub(super) fn cannot_deploy(error: anyhow::Error) -> AgentState {
    AgentState::CannotDeploy {
        reason: format!("{error:#}"),
    }
}

/// 起動できなかった
pub(super) fn cannot_start(error: anyhow::Error) -> AgentState {
    AgentState::CannotStart {
        reason: format!("{error:#}"),
    }
}

/// 起動の挨拶が通らなかった。通信の版が違えばその状態にする
pub(super) fn not_connected(error: anyhow::Error) -> AgentState {
    match error
        .chain()
        .find_map(|cause| cause.downcast_ref::<crate::agent::protocol::ProtocolVersionMismatch>())
    {
        Some(mismatch) => AgentState::VersionMismatch {
            remote: mismatch.remote,
            local: mismatch.local,
        },
        None => cannot_start(error),
    }
}
