use super::*;

fn unusable() -> Vec<AgentState> {
    vec![
        AgentState::Disabled,
        AgentState::UnsafeLocation {
            location: "/home/u/.cache/remote-merge/bin".into(),
        },
        AgentState::VersionMismatch {
            remote: 11,
            local: 12,
        },
        AgentState::CannotDeploy {
            reason: "no space left".into(),
        },
        AgentState::CannotStart {
            reason: "exec format error".into(),
        },
    ]
}

#[test]
fn a_usable_agent_is_chosen() {
    assert_eq!(choose_route(&AgentState::Ready), RouteChoice::Agent);
}

#[test]
fn every_unusable_agent_stops_with_a_reason_that_names_what_is_wrong() {
    let shown = [
        "[agent] enabled",
        "/home/u/.cache/remote-merge/bin",
        "protocol version 11",
        "no space left",
        "exec format error",
    ];
    for (state, shown) in unusable().iter().zip(shown) {
        let RouteChoice::Stop { reason } = choose_route(state) else {
            panic!("{state:?} must stop");
        };
        assert!(reason.contains(shown), "{reason}");
        assert!(reason.contains("agent"), "{reason}");
    }
}

#[test]
fn another_protocol_version_names_both_versions() {
    let RouteChoice::Stop { reason } = choose_route(&unusable()[2]) else {
        panic!("another version must stop");
    };
    assert!(reason.contains("11") && reason.contains("12"), "{reason}");
}
