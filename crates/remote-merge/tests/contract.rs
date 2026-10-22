#[path = "contract/args_support.rs"]
mod args_support;
#[cfg(feature = "test-utils")]
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
#[cfg(feature = "test-utils")]
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
#[cfg(feature = "test-utils")]
#[path = "contract/cli_launch_support.rs"]
mod cli_launch_support;
#[path = "contract/cli_results.rs"]
mod cli_results;
#[path = "contract/cli_usage.rs"]
mod cli_usage;
#[cfg(feature = "test-utils")]
#[path = "contract/compare_links.rs"]
mod compare_links;
#[cfg(feature = "test-utils")]
#[path = "contract/compare_paths_agree.rs"]
mod compare_paths_agree;
#[cfg(feature = "test-utils")]
#[path = "contract/compare_reading.rs"]
mod compare_reading;
#[cfg(feature = "test-utils")]
#[path = "contract/compare_reading_kinds.rs"]
mod compare_reading_kinds;
#[cfg(feature = "test-utils")]
#[path = "contract/compare_recheck.rs"]
mod compare_recheck;
#[cfg(feature = "test-utils")]
#[path = "contract/compare_scopes.rs"]
mod compare_scopes;
#[cfg(feature = "test-utils")]
#[path = "contract/compare_support.rs"]
mod compare_support;
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
#[cfg(feature = "test-utils")]
#[path = "contract/config_plaintext_password.rs"]
mod config_plaintext_password;
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
#[path = "contract/created_dir_removal.rs"]
mod created_dir_removal;
#[path = "contract/diagnostics.rs"]
mod diagnostics;
#[cfg(all(unix, feature = "test-utils"))]
#[path = "contract/diagnostics_sent_data.rs"]
mod diagnostics_sent_data;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_conflict_count.rs"]
mod diff_conflict_count;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_directory_against_file.rs"]
mod diff_directory_against_file;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_entry_limit.rs"]
mod diff_entry_limit;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_entry_limit_named.rs"]
mod diff_entry_limit_named;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_excluded_directory.rs"]
mod diff_excluded_directory;
#[path = "contract/diff_file_kinds.rs"]
mod diff_file_kinds;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_flow_cli.rs"]
mod diff_flow_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_flow_kinds_cli.rs"]
mod diff_flow_kinds_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_flow_support.rs"]
mod diff_flow_support;
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
#[path = "contract/diff_ref_flow_cli.rs"]
mod diff_ref_flow_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_ref_kinds.rs"]
mod diff_ref_kinds;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_ref_scope_cli.rs"]
mod diff_ref_scope_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/diff_ref_support.rs"]
mod diff_ref_support;
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
#[path = "contract/diff_vanished_cli.rs"]
mod diff_vanished_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/dry_run_failures.rs"]
mod dry_run_failures;
#[cfg(feature = "test-utils")]
#[path = "contract/entry_path_routes.rs"]
mod entry_path_routes;
#[path = "contract/filters.rs"]
mod filters;
#[cfg(feature = "test-utils")]
#[path = "contract/flow_routes.rs"]
mod flow_routes;
#[cfg(feature = "test-utils")]
#[path = "contract/flow_support.rs"]
mod flow_support;
#[path = "contract/init.rs"]
mod init;
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
#[path = "contract/merge_cli_remote.rs"]
mod merge_cli_remote;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_content_cli.rs"]
mod merge_content_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_created_directory_routes.rs"]
mod merge_created_directory_routes;
#[path = "contract/merge_dry_run_paths.rs"]
mod merge_dry_run_paths;
#[path = "contract/merge_filtered_named_paths.rs"]
mod merge_filtered_named_paths;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_flow_cli.rs"]
mod merge_flow_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_flow_support.rs"]
mod merge_flow_support;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_force_cli.rs"]
mod merge_force_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_hard_links.rs"]
mod merge_hard_links;
#[path = "contract/merge_hunks.rs"]
mod merge_hunks;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_hunks_failures.rs"]
mod merge_hunks_failures;
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
#[cfg(feature = "test-utils")]
#[path = "contract/merge_recheck_cli.rs"]
mod merge_recheck_cli;
#[path = "contract/merge_ref_hunks_fix.rs"]
mod merge_ref_hunks_fix;
#[path = "contract/merge_ref_kinds.rs"]
mod merge_ref_kinds;
#[path = "contract/merge_results.rs"]
mod merge_results;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_root_gone.rs"]
mod merge_root_gone;
#[path = "contract/merge_steps.rs"]
mod merge_steps;
#[path = "contract/merge_support.rs"]
mod merge_support;
#[cfg(feature = "test-utils")]
#[path = "contract/merge_symlink_changed_routes.rs"]
mod merge_symlink_changed_routes;
#[path = "contract/merge_write.rs"]
mod merge_write;
#[cfg(feature = "test-utils")]
#[path = "contract/old_commands_other_kinds.rs"]
mod old_commands_other_kinds;
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
#[path = "contract/process_routes_support.rs"]
mod process_routes_support;
#[cfg(feature = "test-utils")]
#[path = "contract/reference_other_kinds.rs"]
mod reference_other_kinds;
#[cfg(feature = "test-utils")]
#[path = "contract/rollback_connect_first.rs"]
mod rollback_connect_first;
#[cfg(feature = "test-utils")]
#[path = "contract/rollback_path_agent.rs"]
mod rollback_path_agent;
#[cfg(feature = "test-utils")]
#[path = "contract/rollback_path_remote.rs"]
mod rollback_path_remote;
#[cfg(feature = "test-utils")]
#[path = "contract/rollback_paths.rs"]
mod rollback_paths;
#[cfg(feature = "test-utils")]
#[path = "contract/rollback_saved_record.rs"]
mod rollback_saved_record;
#[path = "contract/rollback_target_states.rs"]
mod rollback_target_states;
#[cfg(feature = "test-utils")]
#[path = "contract/root_gone_before_changes.rs"]
mod root_gone_before_changes;
#[cfg(feature = "test-utils")]
#[path = "contract/root_gone_support.rs"]
mod root_gone_support;
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
#[path = "contract/scan_unresolvable_unreadable_name.rs"]
mod scan_unresolvable_unreadable_name;
#[cfg(feature = "test-utils")]
#[path = "contract/scan_unsearchable_directory.rs"]
mod scan_unsearchable_directory;
#[cfg(feature = "test-utils")]
#[path = "contract/sides_support.rs"]
mod sides_support;
#[cfg(feature = "test-utils")]
#[path = "contract/size_limit.rs"]
mod size_limit;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_agent_deploy.rs"]
mod ssh_agent_deploy;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_agent_deploy_support.rs"]
mod ssh_agent_deploy_support;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_compatibility.rs"]
mod ssh_compatibility;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_external_link.rs"]
mod ssh_external_link;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_fallback.rs"]
mod ssh_fallback;
#[path = "contract/ssh_host_key.rs"]
mod ssh_host_key;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_ignored_algorithms.rs"]
mod ssh_ignored_algorithms;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_login_shell.rs"]
mod ssh_login_shell;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_lost_destination_checks.rs"]
mod ssh_lost_destination_checks;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_lost_destination_hashes.rs"]
mod ssh_lost_destination_hashes;
#[path = "contract/ssh_silence_timeout.rs"]
mod ssh_silence_timeout;
#[cfg(feature = "test-utils")]
use common::server_faults;
#[cfg(feature = "test-utils")]
use common::server_removal;
#[cfg(feature = "test-utils")]
use common::ssh_server;
#[cfg(feature = "test-utils")]
#[path = "contract/agent_support.rs"]
mod agent_support;
#[cfg(feature = "test-utils")]
#[path = "contract/agent_unreadable_answer.rs"]
mod agent_unreadable_answer;
#[cfg(feature = "test-utils")]
#[path = "contract/source_root_gone.rs"]
mod source_root_gone;
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
#[path = "contract/ssh_interrupted_changes.rs"]
mod ssh_interrupted_changes;
#[cfg(feature = "test-utils")]
#[path = "contract/ssh_interrupted_rollback.rs"]
mod ssh_interrupted_rollback;
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
#[path = "contract/status_agent_cli.rs"]
mod status_agent_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/status_flow_cli.rs"]
mod status_flow_cli;
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
#[cfg(feature = "test-utils")]
#[path = "contract/sync_flow_cli.rs"]
mod sync_flow_cli;
#[cfg(feature = "test-utils")]
#[path = "contract/sync_partial_status.rs"]
mod sync_partial_status;
#[path = "contract/sync_results.rs"]
mod sync_results;
#[cfg(feature = "test-utils")]
#[path = "contract/sync_root_gone.rs"]
mod sync_root_gone;
#[cfg(feature = "test-utils")]
#[path = "contract/sync_source_once.rs"]
mod sync_source_once;
#[path = "contract/sync_support.rs"]
mod sync_support;
#[path = "contract/sync_targets.rs"]
mod sync_targets;
#[path = "contract/sync_text.rs"]
mod sync_text;
#[cfg(feature = "test-utils")]
#[path = "contract/testing_isolation.rs"]
mod testing_isolation;
#[path = "contract/write_gone_parent_agree.rs"]
mod write_gone_parent_agree;

mod common;
#[cfg(feature = "test-utils")]
#[path = "contract/content_encoding/mod.rs"]
mod content_encoding;
