//! エージェントの起動の部品: 置くリモートのターゲットの決め方、配置済みの版の確かめの記録、
//! 起動できなかったときのエージェントの状態

use crate::agent::deploy::VersionCheck;
use remote_merge_core::route_choice::AgentState;

/// uname の答えから、置くバイナリのターゲットを決める。配布にないか、確かめられなければ、手元の
/// ターゲットのバイナリを置かずに配置できないとする（docs/ir/ssh/fallback.md の REQ-ssh-005）
pub(super) fn remote_target_of(
    server_name: &str,
    uname_result: Option<anyhow::Result<&'static str>>,
) -> Result<&'static str, AgentState> {
    match uname_result {
        Some(Ok(target)) => {
            tracing::info!(
                "Detected remote target: server={}, target={}",
                server_name,
                target
            );
            Ok(target)
        }
        Some(Err(error)) => Err(AgentState::CannotDeploy {
            reason: format!("the server is not in the agent distribution: {error:#}"),
        }),
        None => Err(AgentState::CannotDeploy {
            reason: "the operating system and architecture of the server could not be detected"
                .into(),
        }),
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
pub(super) fn cannot_deploy(error: &anyhow::Error) -> AgentState {
    AgentState::CannotDeploy {
        reason: format!("{error:#}"),
    }
}

/// 起動できなかった
pub(super) fn cannot_start(error: &anyhow::Error) -> AgentState {
    AgentState::CannotStart {
        reason: format!("{error:#}"),
    }
}

/// 起動の挨拶が通らなかった。通信の版が違えばその状態にする
pub(super) fn not_connected(error: &anyhow::Error) -> AgentState {
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

#[cfg(test)]
mod tests {
    use super::*;

    // 配布にないサーバと、OS とアーキテクチャを確かめられないサーバには、手元のターゲットのバイナリを
    // 置かずに、配置できないサーバとして止める（docs/ir/ssh/fallback.md の REQ-ssh-005）
    // @kotowari[REQ-ssh-005]
    #[test]
    fn a_server_outside_the_distribution_or_of_an_unknown_target_cannot_be_deployed_to() {
        let unknown = remote_target_of("web", Some(Err(anyhow::anyhow!("Linux aarch64"))));
        let undetected = remote_target_of("web", None);

        for state in [unknown, undetected] {
            assert!(
                matches!(state, Err(AgentState::CannotDeploy { .. })),
                "{state:?}"
            );
        }
        assert_eq!(
            remote_target_of("web", Some(Ok("x86_64-unknown-linux-musl"))).ok(),
            Some("x86_64-unknown-linux-musl")
        );
    }

    // ARM の Linux は配布にないので、uname の本当の答えから配置できないサーバとして止める
    // @kotowari[REQ-ssh-005]
    #[test]
    fn an_arm_linux_server_cannot_be_deployed_to() {
        use crate::agent::deploy::remote_target::parse_remote_target;
        let state = remote_target_of("web", Some(parse_remote_target("Linux\naarch64\n")));

        match state {
            Err(AgentState::CannotDeploy { reason }) => {
                assert!(reason.contains("not in the agent distribution"), "{reason}")
            }
            other => panic!("{other:?}"),
        }
    }
}
