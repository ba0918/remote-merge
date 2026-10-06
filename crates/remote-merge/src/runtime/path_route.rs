//! パスの確認をどの経路で行うか（docs/ir/paths/routes.md の REQ-paths-010）。

use crate::agent::protocol::{AgentPathInspection, PATH_RESOLUTION_PROTOCOL_VERSION};
use remote_merge_core::path_resolution::{ResolveError, ResolvedPath};

/// パスの確認の経路
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathInspectionRoute {
    Agent,
    Ssh,
    /// 確認せずに止める。値は利用者に示すエラーの文言
    RedeployAgent(String),
}

/// 交渉したエージェントの通信の版（エージェントがなければ `None`）と sudo を使うかから選ぶ。
/// 古いエージェントは古い辿り方をするので使わない。sudo を使うサーバでは SSH の経路が
/// 読み書きをする root と違う利用者で動くので、SSH にも回さずエージェントの置き直しを求める
pub fn path_inspection_route(agent_protocol: Option<u32>, sudo: bool) -> PathInspectionRoute {
    match agent_protocol {
        Some(version) if version >= PATH_RESOLUTION_PROTOCOL_VERSION => PathInspectionRoute::Agent,
        Some(version) if sudo => PathInspectionRoute::RedeployAgent(format!(
            "the agent speaks protocol v{version}, which cannot inspect paths with sudo = true; \
             redeploy the agent (protocol v{PATH_RESOLUTION_PROTOCOL_VERSION} or later)"
        )),
        _ => PathInspectionRoute::Ssh,
    }
}

/// エージェントの確認の応答を確認の結果に読む。原因の文言は一覧にあるものだけを受け取る
pub fn resolved_from_agent(inspection: AgentPathInspection) -> anyhow::Result<ResolvedPath> {
    Ok(match inspection {
        AgentPathInspection::Missing {
            real_parent,
            parent_exists,
        } => ResolvedPath::Missing {
            real_parent,
            parent_exists,
        },
        AgentPathInspection::Exists { real_path } => ResolvedPath::Exists { real_path },
        AgentPathInspection::Symlink {
            link_target,
            real_path,
        } => ResolvedPath::Symlink {
            link_target,
            real_path,
        },
        AgentPathInspection::AncestorNotDirectory {
            real_path,
            regular_file,
        } => ResolvedPath::AncestorNotDirectory {
            real_path,
            regular_file,
        },
        AgentPathInspection::Unresolved { cause } => {
            return Err(match ResolveError::from_cause(&cause) {
                Some(error) => error.into(),
                None => anyhow::anyhow!("unknown path inspection cause from agent: {cause}"),
            })
        }
        AgentPathInspection::Error { message } => anyhow::bail!(message),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_cause_outside_the_closed_list_is_an_error_not_a_resolution_result() {
        let known = resolved_from_agent(AgentPathInspection::Unresolved {
            cause: "dangling symbolic link".into(),
        })
        .unwrap_err();
        assert_eq!(
            known.downcast_ref::<ResolveError>(),
            Some(&ResolveError::DanglingSymlink)
        );

        let unknown = resolved_from_agent(AgentPathInspection::Unresolved {
            cause: "No such file or directory".into(),
        })
        .unwrap_err();
        assert!(
            unknown.downcast_ref::<ResolveError>().is_none(),
            "{unknown}"
        );
        assert!(unknown.to_string().contains("No such file or directory"));
    }
}
