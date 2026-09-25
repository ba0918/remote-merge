# Ignored-test migration inventory

Source: `cargo nextest list --all-features --message-format json`, selecting
`rust-suites[].testcases[]` where `ignored == true` at the start of this change.
The key in the first column is the suite suffix and testcase name in that JSON.
There are 108 rows: 18 diff, 5 exit, 14 merge, 12 rollback, 13 status,
1 contract, 6 old-container, 6 three-way TUI, 5 diff TUI, 8 merge TUI,
8 navigation TUI, 5 search TUI, 1 server-switch TUI, and 6 startup TUI.

"Keep" means migrate to a real outcome oracle, **not** merely remove `#[ignore]`.
"Fold" names the test whose outcome will detect the same failure; remove the
old test only after that replacement is running. "Drop" is reserved for an
assertion with no distinct product behavior. Normal = isolated Rust SSH fixture
plus CLI or PTY; container = actual OpenSSH, not the Rust fixture. Existing
comments in the old tests describe the intended probe but are not evidence
that it currently detects the failure.

| Ignored case (`suite::testcase`) | Regression it must detect | Overlap and decision | Destination / replacement |
|---|---|---|---|
| cli_diff::test_diff_binary_file | Changed binary reported as text rather than hashes | Keep, distinct binary path | Normal CLI diff, SHA-256 result |
| cli_diff::test_diff_directory | Child differences omitted for directory argument | Keep | Normal CLI diff, child paths and content |
| cli_diff::test_diff_dot_path_resolves | `.` fails to expand to the comparison tree | Keep | Normal CLI diff, affected file |
| cli_diff::test_diff_empty_file | Two zero-byte files reported different | Keep, boundary of equal | Normal CLI diff, exit and output |
| cli_diff::test_diff_equal_file | Identical files produce differences | Fold with `cli_exit_codes::test_diff_exit_0_when_equal` | Normal CLI diff, equality and exit |
| cli_diff::test_diff_json_format | JSON diff omits actual changed file | Keep; merely parsing JSON is insufficient | Normal CLI diff, parsed file and change |
| cli_diff::test_diff_left_only_file | Left-only content omitted from diff | Keep | Normal CLI diff, file and deletion content |
| cli_diff::test_diff_max_lines | Diff ignores output line limit | Keep | Normal CLI diff, bounded lines from real changes |
| cli_diff::test_diff_multiple_files | Only first requested file gets a diff | Keep | Normal CLI diff, both contents |
| cli_diff::test_diff_nonexistent_file | Missing path treated as a successful comparison | Keep | Normal CLI diff, nonzero status and path |
| cli_diff::test_diff_null_bytes_detected_as_binary | Embedded NUL leaked as text rather than binary hash | Keep, distinct from extension-based detection | Normal CLI diff, hash output |
| cli_diff::test_diff_right_only_file | Right-only content omitted from diff | Keep | Normal CLI diff, file and added content |
| cli_diff::test_diff_sensitive_file_force | Explicit force still suppresses requested content | Keep | Normal CLI diff, forced real content |
| cli_diff::test_diff_sensitive_file_warning | Default diff leaks sensitive contents | Keep | Normal CLI diff, absence of content and visible warning |
| cli_diff::test_diff_symlink | A link change compares target bytes rather than link destination | Keep | Normal CLI diff, link destinations |
| cli_diff::test_diff_text_shows_unified_diff | Modified lines are missing from unified diff | Keep | Normal CLI diff, old/new lines |
| cli_diff::test_diff_trailing_slash_normalized | Trailing slash changes selected files | Keep | Normal CLI diff, same real results both forms |
| cli_diff::test_diff_with_ref | Reference comparison ignores the third content | Keep | Normal CLI three-way diff, reference result |
| cli_exit_codes::test_diff_exit_0_when_equal | Equality exits nonzero | Keep; absorbs equal-file duplicate | Normal CLI diff, no changes and code 0 |
| cli_exit_codes::test_diff_exit_1_when_diff_found | Changed file exits zero | Keep | Normal CLI diff, changed content and code 1 |
| cli_exit_codes::test_merge_exit_0_on_success | Successful write exits nonzero | Fold with `cli_merge::test_merge_writes_file` | Normal CLI merge, code 0 and destination bytes |
| cli_exit_codes::test_status_exit_0_when_no_diff | Equal trees exit nonzero | Keep | Normal CLI status, no change and code 0 |
| cli_exit_codes::test_status_exit_1_when_diff_found | Changed tree exits zero | Keep | Normal CLI status, change and code 1 |
| cli_merge::test_merge_binary_file | Binary merge corrupts bytes | Keep | Normal CLI merge, destination bytes |
| cli_merge::test_merge_directory | Directory merge omits child files | Keep | Normal CLI merge, all destination children |
| cli_merge::test_merge_dry_run_does_not_modify | Preview overwrites the destination | Fold with `cli_merge::test_merge_dry_run_shows_plan` | Normal CLI merge, plan and unchanged bytes |
| cli_merge::test_merge_dry_run_shows_plan | Preview hides intended write or writes it | Keep; absorbs dry-run duplicate | Normal CLI merge, planned paths and unchanged bytes |
| cli_merge::test_merge_duplicate_paths_deduplicated | Duplicate path causes repeated write/backup | Keep | Normal CLI merge, one resulting backup and file |
| cli_merge::test_merge_equal_file_skipped | Equal file creates a spurious write/backup | Keep | Normal CLI merge, unchanged destination and no session |
| cli_merge::test_merge_json_format | JSON reports success for a missing write | Keep; parsed result alone is insufficient | Normal CLI merge, JSON and destination bytes |
| cli_merge::test_merge_multiple_files | Multi-path merge silently omits a path | Keep | Normal CLI merge, both destination files |
| cli_merge::test_merge_r2r_with_dry_run_skips_guard | Preview incorrectly writes across remotes | Fold into dry-run no-write plus remote-to-remote refusal; guard order has no specified output | Normal CLI merge, two distinct refusals/unchanged bytes |
| cli_merge::test_merge_records_backup_only_in_aggregate_store | Backup appears inside target root or is missing from store | Keep | Normal CLI merge, directory listing and backup contents |
| cli_merge::test_merge_remote_to_remote_requires_force | Remote-to-remote write proceeds without confirmation | Keep | Normal CLI merge, nonzero and unchanged destination |
| cli_merge::test_merge_sensitive_file_requires_force | Sensitive destination overwritten without force | Keep | Normal CLI merge, nonzero/skipped and unchanged bytes |
| cli_merge::test_merge_sensitive_file_with_force | Explicitly approved sensitive merge fails to write | Keep | Normal CLI merge, destination bytes |
| cli_merge::test_merge_writes_file | Merge reports success without updating destination | Keep; absorbs merge exit duplicate | Normal CLI merge, exit and destination bytes |
| cli_rollback::test_merge_then_rollback_restores_content | Rollback reports success but leaves merged bytes | Keep; shared with force-restores case | Normal CLI rollback, destination original bytes |
| cli_rollback::test_rollback_dry_run_shows_plan_without_changes | Preview restores before approval | Keep | Normal CLI rollback, plan and unchanged bytes |
| cli_rollback::test_rollback_exit_code_no_sessions | No available session reports success | Keep | Normal CLI rollback, code 2 and no write |
| cli_rollback::test_rollback_exit_code_success | Successful restore reports an error | Fold with `test_merge_then_rollback_restores_content` | Normal CLI rollback, code 0 and restored bytes |
| cli_rollback::test_rollback_force_restores_content | Forced restore does not restore | Fold with `test_merge_then_rollback_restores_content` | Normal CLI rollback, restored bytes |
| cli_rollback::test_rollback_json_output_structure | JSON says restored for an unchanged file | Keep; schema-only assertion is insufficient | Normal CLI rollback, parsed result and actual restored bytes |
| cli_rollback::test_rollback_list_after_merge | Merge creates no discoverable session | Keep | Normal CLI rollback list, created session visible |
| cli_rollback::test_rollback_list_json_after_merge | JSON list omits a real session | Keep | Normal CLI rollback list, parsed session ID |
| cli_rollback::test_rollback_multiple_files | Restore recovers only one of several files | Keep | Normal CLI rollback, both original bytes |
| cli_rollback::test_rollback_nested_directory | Restore loses a nested path | Keep | Normal CLI rollback, nested original bytes |
| cli_rollback::test_rollback_skips_sensitive_without_force | Preview promises an unsafe sensitive restore | Keep, dry-run is a plan oracle only | Normal CLI rollback, skipped entry and unchanged bytes |
| cli_rollback::test_rollback_specific_older_session | Explicit old session restores newest contents | Keep | Normal CLI rollback, old content and selected session |
| cli_status::test_status_all_includes_equal | `--all` omits unchanged file | Keep | Normal CLI status, equal entry |
| cli_status::test_status_empty_tree_both_sides | Empty trees return a false difference | Keep | Normal CLI status, code 0 and no entries |
| cli_status::test_status_exclude_filter_works | Excluded `.git` files leak into listing | Keep | Normal CLI status, included control and excluded path |
| cli_status::test_status_excludes_equal_by_default | Default output lists equal file | Keep | Normal CLI status, equal count without entry |
| cli_status::test_status_json_format | JSON has no actual changed entry | Keep; shape-only assertion is insufficient | Normal CLI status, parsed changed path and state |
| cli_status::test_status_json_special_chars_in_path | Filename with space is lost/escaped incorrectly | Keep | Normal CLI status, parsed exact path |
| cli_status::test_status_sensitive_files_included | Status hides a changed sensitive filename | Keep; do not assert on secret contents | Normal CLI status, filename and state |
| cli_status::test_status_summary_shows_counts | Summary miscounts changed/one-sided files | Keep; category-name assertion is insufficient | Normal CLI status, exact counts |
| cli_status::test_status_text_shows_left_only | Left-only entry is omitted | Keep | Normal CLI status, correct file and state |
| cli_status::test_status_text_shows_modified_files | Modified entry is omitted | Keep | Normal CLI status, correct file and state |
| cli_status::test_status_text_shows_right_only | Right-only entry is omitted | Keep | Normal CLI status, correct file and state |
| cli_status::test_status_with_directory_filter | Status drops one of two directory paths | Fold with `test_status_text_shows_modified_files` plus directory diff; status has no path filter | Normal CLI status of both files and directory diff |
| cli_status::test_status_with_ref_shows_badges | Reference result does not reflect third file | Keep; header-only assertion is insufficient | Normal CLI status, three-way state |
| contract::ssh_sudo::privileged_merge_preserves_root_ownership_and_backs_up_the_old_contents | Privileged write loses owner/mode or old contents | Keep; fixture cannot model OS owner | Container CLI, owner, mode, bytes, backup |
| e2e_testenv::agent_setting_selects_the_requested_remote_transport | Agent toggle silently selects wrong transport | Keep, distinguish installed-agent and direct SSH effects | Container CLI, resulting bytes and agent deployment |
| e2e_testenv::remote_merge_keeps_backups_outside_the_target_root | Merge pollutes remote root with backup | Keep, overlaps aggregate-store test but checks actual sshd | Container CLI, remote listing and aggregate contents |
| e2e_testenv::rollback_follows_an_unchanged_symlink_root | Stable root symlink prevents legitimate restoration | Keep | Container CLI, resolved target original bytes |
| e2e_testenv::rollback_preserves_existing_remote_owner_and_permissions | Restore changes existing owner or mode | Keep, actual OS metadata needed | Container CLI, metadata and bytes |
| e2e_testenv::rollback_restores_through_an_intermediate_symlink_outside_the_root | Restore misses a symlinked file outside root | Keep | Container CLI, actual resolved file bytes |
| e2e_testenv::rollback_skips_a_repointed_symlink_root | Restore writes into a new symlink target | Keep | Container CLI, rejected status and untouched new target |
| tui_3way::test_3way_conflict_badge_survives_reenter | Conflict indication disappears after revisiting file | Keep | Normal PTY, conflict indicator after re-entry |
| tui_3way::test_3way_reconnect_then_dir_merge_no_3minus_badge | Reconnect and directory write leave stale three-way state | Keep | Normal PTY, updated badge and destination bytes |
| tui_3way::test_3way_ref_only_file_shows_badge | Reference-only file vanishes from summary | Keep | Normal PTY, visible reference-only state |
| tui_3way::test_3way_right_side_content_loads | Three-way selection shows wrong side's bytes | Keep | Normal PTY, distinct side content |
| tui_3way::test_3way_summary_panel_toggle_with_w | Three-way summary fails to show current states | Keep | Normal PTY, actual summary entries |
| tui_3way::test_3way_swap_with_x | Swap leaves sides unchanged | Keep | Normal PTY, changed labels and contents |
| tui_diff_view::test_enter_spam_does_not_lose_diff | Repeated selection loses file diff | Keep | Normal PTY, diff remains visible |
| tui_diff_view::test_enter_spam_with_directory | Repeated nested selection loses child diff | Keep | Normal PTY, child diff remains visible |
| tui_diff_view::test_equal_file_shows_content | Equal file cannot display its content | Keep | Normal PTY, equal content visible |
| tui_diff_view::test_file_select_shows_diff | Selected file shows no changed lines | Keep | Normal PTY, distinct old/new lines |
| tui_diff_view::test_toggle_unified_sidebyside_with_d | Layout toggle fails to change representation | Keep; crash-only smoke is insufficient | Normal PTY, both visible layouts |
| tui_merge::test_file_merge_with_m_and_confirm | Confirmed merge does not update destination | Keep | Normal PTY, prompt and destination bytes |
| tui_merge::test_hunk_merge_left_to_right_with_l | One-direction hunk applies wrong side | Keep | Normal PTY, destination bytes with selected hunk |
| tui_merge::test_hunk_merge_right_to_left_with_h_key | Reverse hunk applies wrong side | Keep | Normal PTY, destination bytes with selected hunk |
| tui_merge::test_merge_cancel_with_n | Cancel still writes destination | Keep; crash-only smoke is insufficient | Normal PTY, prompt and unchanged bytes |
| tui_merge::test_merge_on_equal_file_ignored | Equal file creates a needless write | Keep | Normal PTY, unchanged file and no backup |
| tui_merge::test_merge_undo_multiple_times | Multiple undo operations leave one merge applied | Keep; crash-only smoke is insufficient | Normal PTY, both original files |
| tui_merge::test_merge_undo_with_u | Undo leaves a merge applied | Keep; crash-only smoke is insufficient | Normal PTY, original file bytes |
| tui_merge::test_sensitive_file_merge_shows_warning | Sensitive write proceeds without warning | Keep | Normal PTY, warning and unchanged file until confirmation |
| tui_navigation::test_cursor_does_not_go_above_first | Up at first row wraps to last | Keep; crash-only smoke is insufficient | Normal PTY, selected first file |
| tui_navigation::test_cursor_does_not_go_below_last | Down at last row wraps to first | Keep | Normal PTY, selected last file |
| tui_navigation::test_cursor_down_with_j | Down fails to select next file | Keep | Normal PTY, second file's distinct diff |
| tui_navigation::test_cursor_up_with_k | Up fails to return to previous file | Keep | Normal PTY, first file's distinct diff |
| tui_navigation::test_deep_directory_expand_collapse | Deep child cannot be selected | Keep | Normal PTY, deepest file and diff |
| tui_navigation::test_directory_collapse_with_h | Collapsed directory still exposes child | Keep; crash-only smoke is insufficient | Normal PTY, child disappears |
| tui_navigation::test_directory_expand_with_enter | Expanded directory hides child | Keep | Normal PTY, child appears and can be selected |
| tui_navigation::test_tab_switches_focus | Tab leaves keyboard actions in old pane | Keep | Normal PTY, action in newly focused pane |
| tui_search::test_search_cancel_with_esc | Escape leaves active search filtering selection | Keep | Normal PTY, restored tree/selection |
| tui_search::test_search_empty_query_does_nothing | Empty search moves selection | Keep; crash-only smoke is insufficient | Normal PTY, same selected file |
| tui_search::test_search_file_by_name | Search fails to select matching filename | Keep | Normal PTY, matched file's distinct diff |
| tui_search::test_search_next_with_n | Next match never advances selection | Keep | Normal PTY, next match's distinct diff |
| tui_search::test_search_no_match_shows_message | No-match search silently presents stale match | Keep; crash-only smoke is insufficient | Normal PTY, visible no-match state |
| tui_server_switch::test_invalid_server_name_rejected_at_startup | Unknown server launches against an unintended target | Keep; no SSH fixture needed | Normal PTY/CLI, nonzero exit and error |
| tui_startup::test_tui_help_dialog_with_question_mark | Help key fails to display usable dialog | Keep | Normal PTY, visible help text |
| tui_startup::test_tui_quit_with_q | Quit fails to terminate session | Keep; buffered bytes are not process-exit evidence | Normal PTY, actual process exit |
| tui_startup::test_tui_shows_badge_for_left_only | Left-only badge absent in TUI | Keep; CLI badge is not a TUI oracle | Normal PTY, visible left-only badge |
| tui_startup::test_tui_shows_badge_for_modified_file | Modified badge absent in TUI | Keep; CLI badge is not a TUI oracle | Normal PTY, visible modified badge |
| tui_startup::test_tui_shows_header_with_server_names | Header names wrong endpoints | Keep | Normal PTY, both configured names |
| tui_startup::test_tui_starts_and_shows_file_tree | Connected file tree fails to load | Keep | Normal PTY, actual remote filename |

The Docker setup under `testenv/` is separate from the in-process Rust SSH
fixture. The former must not become a hidden prerequisite of normal tests.
