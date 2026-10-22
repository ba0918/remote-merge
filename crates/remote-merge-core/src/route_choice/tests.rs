use super::*;

#[test]
fn another_protocol_version_names_both_versions() {
    let state = AgentState::VersionMismatch {
        remote: 11,
        local: 12,
    };
    let RouteChoice::Stop { reason } = choose_route(&state) else {
        panic!("another version must stop");
    };
    assert!(reason.contains("11") && reason.contains("12"), "{reason}");
}
