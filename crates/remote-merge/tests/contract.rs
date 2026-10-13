#[path = "contract/backup_cleanup.rs"]
mod backup_cleanup;
#[path = "contract/backup_failure.rs"]
mod backup_failure;
#[path = "contract/backup_location.rs"]
mod backup_location;
#[path = "contract/backup_record_paths.rs"]
mod backup_record_paths;
#[path = "contract/backup_rollback_cli.rs"]
mod backup_rollback_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/backup_rollback_cli_e2e.rs"]
mod backup_rollback_cli_e2e;
#[path = "contract/backup_rollback_path.rs"]
mod backup_rollback_path;
#[path = "contract/backup_sessions.rs"]
mod backup_sessions;
#[path = "contract/backup_storage.rs"]
mod backup_storage;
#[path = "contract/backup_store_failure.rs"]
mod backup_store_failure;
#[path = "contract/backup_support.rs"]
mod backup_support;
#[cfg(feature = "test-utils")]
#[path = "contract/change_failures_agree.rs"]
mod change_failures_agree;
#[path = "contract/cli_results.rs"]
mod cli_results;
#[path = "contract/cli_usage.rs"]
mod cli_usage;
#[path = "contract/config_filters.rs"]
mod config_filters;
#[cfg(feature = "test-utils")]
#[path = "contract/config_filters_cli.rs"]
mod config_filters_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/config_include_cli.rs"]
mod config_include_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/config_include_skip_cli.rs"]
mod config_include_skip_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/config_include_targets_cli.rs"]
mod config_include_targets_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/config_include_value_kinds.rs"]
mod config_include_value_kinds;
#[cfg(feature = "test-utils")]
#[path = "contract/config_loading_cli.rs"]
mod config_loading_cli;
#[path = "contract/config_merging.rs"]
mod config_merging;
#[path = "contract/config_precedence.rs"]
mod config_precedence;
#[cfg(feature = "test-utils")]
#[path = "contract/config_suspicious_exclude.rs"]
mod config_suspicious_exclude;
#[path = "contract/config_values.rs"]
mod config_values;
#[cfg(feature = "test-utils")]
#[path = "contract/config_values_cli.rs"]
mod config_values_cli;
#[path = "contract/diagnostics.rs"]
mod diagnostics;
#[cfg(all(unix, feature = "test-utils"))]
#[path = "contract/diagnostics_sent_data.rs"]
mod diagnostics_sent_data;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_directory_against_file.rs"]
mod diff_directory_against_file;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_entry_limit.rs"]
mod diff_entry_limit;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_excluded_directory.rs"]
mod diff_excluded_directory;
#[path = "contract/diff_file_kinds.rs"]
mod diff_file_kinds;
#[path = "contract/diff_format.rs"]
mod diff_format;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_identical_links.rs"]
mod diff_identical_links;
#[path = "contract/diff_max_files.rs"]
mod diff_max_files;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_named_directory_scan.rs"]
mod diff_named_directory_scan;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_named_files.rs"]
mod diff_named_files;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_one_sided.rs"]
mod diff_one_sided;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_output_cli.rs"]
mod diff_output_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_ref_cli.rs"]
mod diff_ref_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_ref_kinds.rs"]
mod diff_ref_kinds;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_ref_scope_cli.rs"]
mod diff_ref_scope_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_root_link_cli.rs"]
mod diff_root_link_cli;
#[path = "contract/diff_selection.rs"]
mod diff_selection;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_strategy_support.rs"]
mod diff_strategy_support;
#[path = "contract/diff_support.rs"]
mod diff_support;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_unlistable.rs"]
mod diff_unlistable;
#[cfg(feature = "test-utils")]
#[path = "contract/dry_run_failures.rs"]
mod dry_run_failures;
#[cfg(feature = "test-utils")]
#[path = "contract/entry_path_routes.rs"]
mod entry_path_routes;
#[path = "contract/filters.rs"]
mod filters;
#[cfg(feature = "test-utils")]
#[path = "contract/flow_support.rs"]
mod flow_support;
#[path = "contract/init.rs"]
mod init;
#[cfg(feature = "test-utils")]
#[path = "contract/listing_limit.rs"]
mod listing_limit;
#[cfg(feature = "test-utils")]
#[path = "contract/listing_support.rs"]
mod listing_support;
#[path = "contract/logs_level.rs"]
mod logs_level;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_agent_outside_links.rs"]
mod merge_agent_outside_links;
#[path = "contract/merge_cli.rs"]
mod merge_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_cli_ssh.rs"]
mod merge_cli_ssh;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_created_directory_routes.rs"]
mod merge_created_directory_routes;
#[path = "contract/merge_dry_run_paths.rs"]
mod merge_dry_run_paths;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_force_cli.rs"]
mod merge_force_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_hard_links.rs"]
mod merge_hard_links;
#[path = "contract/merge_hunks.rs"]
mod merge_hunks;
#[path = "contract/merge_kind_subtree.rs"]
mod merge_kind_subtree;
#[path = "contract/merge_links.rs"]
mod merge_links;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_missing_paths.rs"]
mod merge_missing_paths;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_missing_root_dir.rs"]
mod merge_missing_root_dir;
#[path = "contract/merge_new_link_paths.rs"]
mod merge_new_link_paths;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_new_owner.rs"]
mod merge_new_owner;
#[path = "contract/merge_paths.rs"]
mod merge_paths;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_permission_copy.rs"]
mod merge_permission_copy;
#[path = "contract/merge_ref_hunks_fix.rs"]
mod merge_ref_hunks_fix;
#[path = "contract/merge_ref_kinds.rs"]
mod merge_ref_kinds;
#[path = "contract/merge_results.rs"]
mod merge_results;
#[path = "contract/merge_support.rs"]
mod merge_support;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_symlink_changed_routes.rs"]
mod merge_symlink_changed_routes;
#[path = "contract/merge_write.rs"]
mod merge_write;
#[cfg(unix)]
#[path = "contract/op_failure_kinds.rs"]
mod op_failure_kinds;
#[cfg(feature = "test-utils")]
#[path = "contract/operation_failures_agree.rs"]
mod operation_failures_agree;
#[cfg(feature = "test-utils")]
#[path = "contract/path_arguments_cli.rs"]
mod path_arguments_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/path_operations_agree.rs"]
mod path_operations_agree;
#[path = "contract/path_resolution.rs"]
mod path_resolution;
#[cfg(feature = "test-utils")]
#[path = "contract/path_routes_agree.rs"]
mod path_routes_agree;
#[cfg(feature = "test-utils")]
#[path = "contract/paths_not_regular.rs"]
mod paths_not_regular;
#[cfg(feature = "test-utils")]
#[path = "contract/rollback_path_agent.rs"]
mod rollback_path_agent;
#[cfg(feature = "test-utils")]
#[path = "contract/rollback_path_ssh.rs"]
mod rollback_path_ssh;
#[path = "contract/rollback_paths.rs"]
mod rollback_paths;
#[cfg(feature = "test-utils")]
#[path = "contract/rollback_saved_record.rs"]
mod rollback_saved_record;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_entry_count.rs"]
mod scan_entry_count;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_excluded_dangling_link.rs"]
mod scan_excluded_dangling_link;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_excluded_unresolvable_link.rs"]
mod scan_excluded_unresolvable_link;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_filter_scope.rs"]
mod scan_filter_scope;
#[path = "contract/scan_limit_scope.rs"]
mod scan_limit_scope;
#[path = "contract/scan_limits.rs"]
mod scan_limits;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_limits_cli.rs"]
mod scan_limits_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_listing_cli.rs"]
mod scan_listing_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_missing_root.rs"]
mod scan_missing_root;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_names_as_read_cli.rs"]
mod scan_names_as_read_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_non_utf8_names.rs"]
mod scan_non_utf8_names;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_other_kinds.rs"]
mod scan_other_kinds;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_partial_starts.rs"]
mod scan_partial_starts;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_routes_agree.rs"]
mod scan_routes_agree;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_routes_metadata.rs"]
mod scan_routes_metadata;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_routes_same_tree.rs"]
mod scan_routes_same_tree;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_timeout.rs"]
mod scan_timeout;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_times_agree.rs"]
mod scan_times_agree;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_unreadable_directory.rs"]
mod scan_unreadable_directory;
#[path = "contract/scan_unreadable_entry.rs"]
mod scan_unreadable_entry;
#[path = "contract/scan_unreadable_link.rs"]
mod scan_unreadable_link;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_unresolvable_link_cli.rs"]
mod scan_unresolvable_link_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_unsearchable_directory.rs"]
mod scan_unsearchable_directory;
#[cfg(feature = "test-utils")]
#[path = "contract/size_limit.rs"]
mod size_limit;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_agent_deploy.rs"]
mod ssh_agent_deploy;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_compatibility.rs"]
mod ssh_compatibility;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_disconnect.rs"]
mod ssh_disconnect;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_external_link.rs"]
mod ssh_external_link;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_fallback.rs"]
mod ssh_fallback;
#[path = "contract/ssh_host_key.rs"]
mod ssh_host_key;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_login_shell.rs"]
mod ssh_login_shell;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_reconnect.rs"]
mod ssh_reconnect;
#[cfg(feature = "test-utils")]
use common::server_faults;
#[cfg(feature = "test-utils")]
use common::ssh_server;
#[cfg(feature = "test-utils")]
#[path = "contract/agent_support.rs"]
mod agent_support;
#[cfg(feature = "test-utils")]
#[path = "contract/agent_unreadable_answer.rs"]
mod agent_unreadable_answer;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_agent_operations.rs"]
mod ssh_agent_operations;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_agent_version.rs"]
mod ssh_agent_version;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_connection_loss.rs"]
mod ssh_connection_loss;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_find_required.rs"]
mod ssh_find_required;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_interrupted_changes.rs"]
mod ssh_interrupted_changes;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_interrupted_rollback.rs"]
mod ssh_interrupted_rollback;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_remote_tools.rs"]
mod ssh_remote_tools;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_route_choice.rs"]
mod ssh_route_choice;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_strict_exec.rs"]
mod ssh_strict_exec;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_sudo.rs"]
mod ssh_sudo;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_unmarked_answer.rs"]
mod ssh_unmarked_answer;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_write_in_place.rs"]
mod ssh_write_in_place;
#[cfg(feature = "test-utils")]
#[path = "contract/status_agent_cli.rs"]
mod status_agent_cli;
#[path = "contract/status_judgement.rs"]
mod status_judgement;
#[path = "contract/status_kinds.rs"]
mod status_kinds;
#[cfg(feature = "test-utils")]
#[path = "contract/status_output.rs"]
mod status_output;
#[path = "contract/status_results.rs"]
mod status_results;
#[path = "contract/status_support.rs"]
mod status_support;
#[cfg(feature = "test-utils")]
#[path = "contract/status_targets.rs"]
mod status_targets;
#[cfg(feature = "test-utils")]
#[path = "contract/status_time_precision.rs"]
mod status_time_precision;
#[path = "contract/status_unreadable.rs"]
mod status_unreadable;
#[cfg(feature = "test-utils")]
#[path = "contract/sync_cli.rs"]
mod sync_cli;
#[path = "contract/sync_results.rs"]
mod sync_results;
#[path = "contract/sync_support.rs"]
mod sync_support;
#[path = "contract/sync_targets.rs"]
mod sync_targets;
#[path = "contract/sync_text.rs"]
mod sync_text;

// @kotowari[REQ-testing-002]
#[cfg(feature = "test-utils")]
#[test]
fn two_synchronous_cli_sessions_compare_and_write_only_their_own_remote_files() {
    use std::fs;

    let first = common::CliEnv::new(
        &[("same.txt", "first local long version\n")],
        &[("same.txt", "first remote\n")],
    );
    let second = common::CliEnv::new(
        &[("same.txt", "second local long version\n")],
        &[("same.txt", "second remote\n")],
    );

    for env in [&first, &second] {
        let config: toml::Value = fs::read_to_string(&env.config_path)
            .unwrap()
            .parse()
            .unwrap();
        let server = &config["servers"]["develop"];
        assert_eq!(server["host"].as_str(), Some("127.0.0.1"));
        let port = server["port"].as_integer().expect("fixture port missing");
        assert!(
            port > 0 && port != 22,
            "refusing an unowned SSH port: {port}"
        );
        assert_eq!(server["auth"].as_str(), Some("password"));
        assert!(server.get("key").is_none(), "refusing a HOME key path");
        assert_eq!(config["local"]["root_dir"].as_str(), env.local_dir.to_str());
        assert_eq!(server["root_dir"].as_str(), env.remote_dir.to_str());
        assert!(env.local_dir.starts_with(env.temp_root()));
        assert!(env.remote_dir.starts_with(env.temp_root()));
    }

    for (env, expected, other) in [
        (&first, "first local long version\n", "second remote\n"),
        (&second, "second local long version\n", "first remote\n"),
    ] {
        let status = env.cmd_with("diff").arg("same.txt").output().unwrap();
        let text = String::from_utf8_lossy(&status.stdout);
        assert!(text.contains(expected.trim()), "{status:?}");
        assert!(!text.contains(other.trim()), "{status:?}");

        let result = env
            .cmd_with("merge")
            .args(["same.txt", "--left", "local", "--right", "develop"])
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        assert_eq!(
            fs::read_to_string(env.remote_dir.join("same.txt")).unwrap(),
            expected
        );
    }
    assert_ne!(first.remote_dir, second.remote_dir);
}

// @kotowari[REQ-testing-001, REQ-testing-002]
#[cfg(feature = "test-utils")]
#[test]
fn unsafe_ssh_configuration_is_rejected_before_a_cli_process_can_start() {
    use std::fs;

    let env = common::CliEnv::new(&[], &[]);
    let safe = fs::read_to_string(&env.config_path).unwrap();
    let config: toml::Value = safe.parse().unwrap();
    let owned_port = config["servers"]["develop"]["port"].as_integer().unwrap();
    assert_ne!(owned_port, 22);

    let unsafe_port = safe.replace(&format!("port = {owned_port}"), "port = 22");
    fs::write(&env.config_path, unsafe_port).unwrap();
    assert!(std::panic::catch_unwind(|| env.cmd()).is_err());

    fs::write(
        &env.config_path,
        safe.replace(
            "auth = \"password\"",
            "auth = \"key\"\nkey = \"~/.ssh/id_ed25519\"",
        ),
    )
    .unwrap();
    assert!(std::panic::catch_unwind(|| env.cmd()).is_err());
}
#[cfg(feature = "test-utils")]
mod common;
