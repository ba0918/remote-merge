//! エージェントのパスの確認の応答を、確認の結果に読む。

use crate::agent::protocol::AgentPathInspection;
use remote_merge_core::path_resolution::{ResolveError, ResolvedPath};

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
