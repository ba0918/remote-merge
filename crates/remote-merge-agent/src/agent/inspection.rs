//! エージェントが答えたパスの確認を、確認の結果の型に読む

use remote_merge_core::path_resolution::{ResolveError, ResolvedPath};

use super::client::AgentRefused;
use super::protocol::AgentPathInspection;

/// 確認の結果。辿れなかったときの原因は、一覧にある文言だけを受け取る
pub fn resolved_from_agent(inspection: AgentPathInspection) -> anyhow::Result<ResolvedPath> {
    match inspection {
        AgentPathInspection::Missing {
            real_parent,
            parent_exists,
        } => Ok(ResolvedPath::Missing {
            real_parent,
            parent_exists,
        }),
        AgentPathInspection::Exists { real_path } => Ok(ResolvedPath::Exists { real_path }),
        AgentPathInspection::Symlink {
            link_target,
            real_path,
        } => Ok(ResolvedPath::Symlink {
            link_target,
            real_path,
        }),
        AgentPathInspection::AncestorNotDirectory {
            real_path,
            regular_file,
        } => Ok(ResolvedPath::AncestorNotDirectory {
            real_path,
            regular_file,
        }),
        AgentPathInspection::Unresolved { cause } => match ResolveError::from_cause(&cause) {
            Some(error) => Err(error.into()),
            None => Err(AgentRefused(format!("unknown path inspection cause: {cause}")).into()),
        },
        AgentPathInspection::Error { message } => Err(AgentRefused(message).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cause_outside_the_list_is_not_taken_as_a_resolution() {
        let error = resolved_from_agent(AgentPathInspection::Unresolved {
            cause: "something else".into(),
        })
        .unwrap_err();
        assert!(error.downcast_ref::<ResolveError>().is_none(), "{error:#}");
        let known = resolved_from_agent(AgentPathInspection::Unresolved {
            cause: "permission denied".into(),
        })
        .unwrap_err();
        assert_eq!(
            known.downcast_ref::<ResolveError>(),
            Some(&ResolveError::PermissionDenied)
        );
    }
}
