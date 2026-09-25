use remote_merge::config::StrictHostKeyChecking;
use remote_merge::ssh::host_key_verifier::verifier_from_policy;

fn accepts_unknown_key(policy: StrictHostKeyChecking, auto_yes: bool, is_tui: bool) -> bool {
    verifier_from_policy(policy, auto_yes, is_tui).verify_host_key(
        "example.invalid",
        22,
        "ssh-ed25519",
        "SHA256:example",
    )
}

// @kotowari[EX-ssh-001, EX-ssh-004]
#[test]
fn unknown_key_without_an_interactive_approval_channel_is_rejected() {
    assert!(!accepts_unknown_key(
        StrictHostKeyChecking::Ask,
        false,
        true
    ));
    assert!(!accepts_unknown_key(
        StrictHostKeyChecking::Yes,
        false,
        true
    ));
}

// @kotowari[EX-ssh-003]
#[test]
fn explicit_yes_option_accepts_the_unknown_key() {
    assert!(accepts_unknown_key(StrictHostKeyChecking::Ask, true, false));
}

// @kotowari[REQ-ssh-002]
#[test]
fn explicit_no_policy_accepts_the_unknown_key_in_both_interfaces() {
    assert!(accepts_unknown_key(StrictHostKeyChecking::No, false, false));
    assert!(accepts_unknown_key(StrictHostKeyChecking::No, false, true));
}
