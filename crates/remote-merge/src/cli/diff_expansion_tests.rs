use super::*;

fn tree() -> FileTree {
    let mut tree = FileTree::new("/r");
    let mut link = FileNode::new_symlink("l", "d");
    link.link_is_dir = true;
    link.children = Some(Default::default());
    tree.nodes = vec![
        FileNode::new_dir_with_children("d", vec![FileNode::new_file("a")]),
        link,
        FileNode::new_file("f"),
    ];
    tree
}

#[test]
fn children_come_from_the_scanned_tree() {
    assert!(matches!(children_in(&tree(), "d"), ChildListing::Children(c) if c.len() == 1));
    assert!(matches!(children_in(&tree(), "l"), ChildListing::Children(c) if c.is_empty()));
    assert!(matches!(
        children_in(&tree(), "f"),
        ChildListing::NotDirectory
    ));
    assert!(matches!(
        children_in(&tree(), "gone"),
        ChildListing::NotDirectory
    ));
}

#[test]
fn a_path_below_a_scanned_directory_is_answered_from_its_scan() {
    assert!(is_within("d/x", "d"));
    assert!(is_within("d", "d"));
    assert!(!is_within("dx", "d"));
    assert_eq!(normalized("./d//x/"), "d/x");
}

/// サーバ "develop" に、最初の問い合わせに走査の失敗 `failure` を返す試験用のエージェントを
/// つないだ runtime
#[cfg(unix)]
fn runtime_whose_agent_fails_with(
    failure: remote_merge_core::scan::outcome::ScanFailure,
) -> CoreRuntime {
    let mut core = CoreRuntime::new_for_test();
    core.config.servers.insert(
        "develop".into(),
        crate::config::ServerConfig {
            host: "10.0.0.1".into(),
            port: 22,
            user: "deploy".into(),
            auth: crate::config::AuthMethod::Key,
            password: None,
            key: None,
            root_dir: "/srv/app".into(),
            ssh_options: None,
            sudo: false,
            file_permissions: None,
            dir_permissions: None,
        },
    );
    crate::runtime::scripted_agent::connect(
        &mut core,
        "develop",
        Some(crate::agent::protocol::AgentResponse::TreeFailed { failure }),
    );
    core
}

// @kotowari[REQ-cli-096]
#[cfg(unix)]
#[test]
fn an_entry_changed_during_the_scan_makes_the_named_path_unlistable() {
    use remote_merge_core::scan::outcome::ScanFailure;
    let mut core = runtime_whose_agent_fails_with(ScanFailure::ChangedDuringScan {
        path: "dir/x".into(),
    });
    let listing = Expansions::new(100)
        .list(&mut core, &Side::Remote("develop".into()), "dir")
        .unwrap();
    assert!(
        matches!(&listing, ChildListing::Unlistable(reason) if reason.contains("dir/x")),
        "{listing:?}"
    );
}

// @kotowari[REQ-cli-096]
#[cfg(unix)]
#[test]
fn a_failure_of_the_collection_itself_stops_the_diff() {
    use remote_merge_core::scan::outcome::ScanFailure;
    for failure in [
        ScanFailure::InconsistentAnswer { detail: "x".into() },
        ScanFailure::CollectFailed {
            message: "y".into(),
        },
    ] {
        let mut core = runtime_whose_agent_fails_with(failure.clone());
        let error = Expansions::new(100)
            .list(&mut core, &Side::Remote("develop".into()), "dir")
            .unwrap_err();
        assert_eq!(error.downcast_ref::<ScanFailure>(), Some(&failure));
    }
}
