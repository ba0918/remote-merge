//! 経路ごとのパスの確認（docs/ir/paths/routes.md）の契約テスト。

use remote_merge::runtime::path_route::{path_inspection_route, PathInspectionRoute};

// @kotowari[REQ-paths-010, EX-paths-021]
#[test]
fn ex_paths_021_an_agent_older_than_protocol_5_leaves_path_inspection_to_ssh() {
    assert_eq!(
        path_inspection_route(Some(4), false),
        PathInspectionRoute::Ssh
    );
    assert_eq!(
        path_inspection_route(Some(5), false),
        PathInspectionRoute::Agent
    );
    assert_eq!(path_inspection_route(None, false), PathInspectionRoute::Ssh);
}

// @kotowari[EX-paths-024]
#[test]
fn ex_paths_024_an_agent_older_than_protocol_5_on_a_sudo_server_must_be_redeployed() {
    assert_eq!(
        path_inspection_route(Some(5), true),
        PathInspectionRoute::Agent
    );
    let route = path_inspection_route(Some(4), true);

    let PathInspectionRoute::RedeployAgent(message) = route else {
        panic!("{route:?}");
    };
    let message = message.to_lowercase();
    assert!(
        message.contains("agent") && message.contains("redeploy"),
        "{message}"
    );
}
