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
fn a_usable_agent_is_chosen_with_or_without_sudo() {
    for sudo in [false, true] {
        assert_eq!(choose_route(sudo, &AgentState::Ready), RouteChoice::Agent);
    }
}

#[test]
fn a_disabled_agent_uses_ssh_without_a_warning() {
    assert_eq!(
        choose_route(false, &AgentState::Disabled),
        RouteChoice::Ssh { warning: None }
    );
}

#[test]
fn an_unsafe_location_uses_ssh_with_a_warning_naming_the_location() {
    let RouteChoice::Ssh {
        warning: Some(warning),
    } = choose_route(false, &unusable()[1])
    else {
        panic!("an unsafe location goes to SSH with a warning");
    };
    assert!(
        warning.contains("/home/u/.cache/remote-merge/bin"),
        "{warning}"
    );
}

#[test]
fn another_protocol_version_uses_ssh_with_a_warning_naming_both_versions() {
    let RouteChoice::Ssh {
        warning: Some(warning),
    } = choose_route(false, &unusable()[2])
    else {
        panic!("another version goes to SSH with a warning");
    };
    assert!(warning.to_lowercase().contains("protocol"), "{warning}");
    assert!(
        warning.contains("11") && warning.contains("12"),
        "{warning}"
    );
}

#[test]
fn an_agent_that_cannot_be_placed_or_started_uses_ssh_with_the_reason() {
    for (state, reason) in [
        (&unusable()[3], "no space left"),
        (&unusable()[4], "exec format error"),
    ] {
        let RouteChoice::Ssh {
            warning: Some(warning),
        } = choose_route(false, state)
        else {
            panic!("{state:?} goes to SSH with a warning");
        };
        assert!(warning.contains("not using the agent"), "{warning}");
        assert!(warning.contains(reason), "{warning}");
    }
}

#[test]
fn a_sudo_server_stops_whenever_the_agent_cannot_be_used() {
    for state in unusable() {
        let RouteChoice::Stop { reason } = choose_route(true, &state) else {
            panic!("{state:?} with sudo must stop");
        };
        assert!(reason.contains("sudo"), "{reason}");
    }
}
