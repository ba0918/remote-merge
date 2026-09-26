# バックアップと rollback のテスト整理の記録

バックアップと rollback の要件（REQ-backup-011 から REQ-backup-041）の根拠テストを整理した過程の記録。
変異テストの結果、各要件の根拠にしたテスト、削除候補と利用者の判断を残す。

## 整理前の変異テスト

整理を始める前のコミット 6e9cfe7 で、次のコマンドを一度だけ実行した。
三つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest）で整理後と比べられるようにした。

```sh
scripts/mutants.sh src/backup/mod.rs src/service/rollback.rs src/runtime/backup_store.rs
```

全体の集計は `mutants: caught=96 survived=16 timeout=0 unviable=14 equivalent=0`（実行時間は約 32 分）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/backup/mod.rs | 25 | 7 | 0 | 10 |
| src/service/rollback.rs | 42 | 0 | 0 | 2 |
| src/runtime/backup_store.rs | 29 | 9 | 0 | 2 |

### 見逃し

位置は変異が入る行、変異は kotowari mutants の出力の文言のまま。

| 位置 | 変異 |
|---|---|
| src/backup/mod.rs:45 | replace RemoteTargetIdentity::host -> &str with "" |
| src/backup/mod.rs:45 | replace RemoteTargetIdentity::host -> &str with "xyzzy" |
| src/backup/mod.rs:49 | replace RemoteTargetIdentity::port -> u16 with 0 |
| src/backup/mod.rs:49 | replace RemoteTargetIdentity::port -> u16 with 1 |
| src/backup/mod.rs:79 | replace backup_timestamp -> String with "xyzzy".into() |
| src/backup/mod.rs:79 | replace backup_timestamp -> String with String::new() |
| src/backup/mod.rs:112 | replace > with < in parse_session_id |
| src/runtime/backup_store.rs:117 | replace == with != in BackupStore::reserve_session |
| src/runtime/backup_store.rs:117 | replace match guard error.kind() == std::io::ErrorKind::AlreadyExists with true in BackupStore::reserve_session |
| src/runtime/backup_store.rs:242 | replace += with *= in BackupStore::cleanup_expired |
| src/runtime/backup_store.rs:310 | replace && with \|\| in BackupStore::list_sessions |
| src/runtime/backup_store.rs:347 | replace match guard path == rel_path with true in BackupStore::read_record |
| src/runtime/backup_store.rs:359 | replace match guard path == rel_path with true in BackupStore::read_record |
| src/runtime/backup_store.rs:383 | replace == with != in create_temporary_entry |
| src/runtime/backup_store.rs:383 | replace match guard error.kind() == std::io::ErrorKind::AlreadyExists with false in create_temporary_entry |
| src/runtime/backup_store.rs:383 | replace match guard error.kind() == std::io::ErrorKind::AlreadyExists with true in create_temporary_entry |

`src/backup/mod.rs:112` の `replace > with < in parse_session_id` は、この実行では見逃しだったが、スクリプトの確認のために直前に `src/backup/mod.rs` だけを対象にした実行（caught=26 survived=6 unviable=10）では検知されていた。
二つの実行のテストの集合は同じ（どちらも 2,846 件）で、検知の有無が実行ごとに変わる理由は確かめていない。
整理後の比較では、この変異を見逃しの基準に含める。

## 要件ごとの根拠テスト

根拠テストは `tests/contract/` の下に IR の文書ごとのモジュールとして置いた。
一つの要件に複数の経路や場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
「元のテスト」は移したテストの移し元で、移したものは移し元から消した。
移すときに要件の文の一部しか確かめていなかったものは、足りない確認を書き足した。

### 保存（docs/ir/backup/storage.md）

根拠テストは全て `tests/contract/backup_storage.rs` にある。

| 要件 | 根拠テスト | 元のテストと変えたところ |
|---|---|---|
| REQ-backup-011 | aggregate_store_entries_are_owner_only_regardless_of_the_original_permissions | tests/local_backup_store.rs の aggregate_store_entries_are_owner_only_and_describe_target。元のファイルの権限を 0664・0644 に明示し、--delete で保存したものも含めて全ての項目を確かめる |
| REQ-backup-012 | each_target_area_has_a_readable_description_of_the_write_target | 同じテストの説明ファイルの確認を分けた。元は root_dir を含むことだけを見ていたため、ホスト名とポート（既定値でない 2222）と、ローカルの書き込み先の絶対パスを確かめる |
| REQ-backup-013 | overwrite_backs_up_the_destination_content_read_immediately_before_writing、deletion_backs_up_the_destination_content_read_immediately_before_removing | tests/local_backup_store.rs の rollback_restores_the_content_seen_immediately_before_merge_writes。要件の「削除の直前」を確かめる削除の場合を書き足した |
| REQ-backup-014 | only_the_overwritten_file_on_the_written_side_is_backed_up | tests/local_backup_store.rs の read_only_side_has_no_session_after_one_way_merge。書き込んだ側のセッションに上書きしたファイルだけが入ることを書き足した |
| REQ-backup-015 | creating_a_new_file_records_nothing_and_leaves_no_session、creating_a_new_file_in_a_missing_directory_records_nothing_and_leaves_no_session | tests/local_backup_store.rs の new_file_merge_records_no_backup と new_file_in_a_missing_directory_records_no_backup。集約先に何も残らないことと一覧に出ないことを両方に書き足した |
| REQ-backup-016 | legacy_backup_directory_is_left_alone_and_hidden_from_list_and_status | tests/local_backup_store.rs の legacy_backup_directory_is_ignored_by_list_and_status。merge の後も既存の ".remote-merge-backup/" の中身が変わらないことと、status の全件の結果に出ないことを書き足した |

REQ-backup-012 のテストは、`RemoteTargetIdentity::port` を 1 を返すように書き換えると失敗することを、コミットに含めない一時的な書き換えで確かめた。

### 失敗時の扱い（docs/ir/backup/failure.md）

根拠テストは全て `tests/contract/backup_failure.rs` にある。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。

| 要件 | 根拠テスト | 元のテストと変えたところ |
|---|---|---|
| REQ-backup-017 | merge_reports_a_file_it_cannot_back_up_as_failed_and_leaves_it_unchanged | tests/local_backup_store.rs の backup_store_failure_leaves_target_unchanged_and_reports_file_failure。失敗したファイルのパスと、"backup failed: " の後に原因があることを書き足した |
| REQ-backup-017 | delete_reports_a_file_it_cannot_back_up_as_failed_and_keeps_it | 新しく書いた（--delete）。下の注を参照 |
| REQ-backup-017 | sync_reports_a_file_it_cannot_back_up_as_failed_and_leaves_it_unchanged | 新しく書いた（sync） |
| REQ-backup-018 | enabled_backup_without_store_location_stops_merge_and_sync_before_writing | tests/local_backup_store.rs の enabled_backup_without_store_location_stops_merge。sync の場合を書き足した |
| REQ-backup-018 | disabled_backup_without_store_location_lets_merge_and_sync_write_without_backup | tests/local_backup_store.rs の disabled_backup_without_store_location_allows_merge と disabled_backup_merge_without_store_location_proceeds_without_cleanup を一つにした。結果に backup が出ないことと sync の場合を書き足した |
| REQ-backup-019 | rollback_fails_in_every_mode_without_store_location_and_writes_nothing | tests/local_backup_store.rs の rollback_list_fails_when_backup_store_location_is_unavailable と、enabled_・disabled_ で始まる集約先なしの五件（補助関数 assert_rollback_location_error を含む）を、三つのモードとバックアップの有効・無効の六通りを回す一つのテストにした。書き込み先が変わらないことを書き足した |
| REQ-backup-020 | tui_merge_is_refused_with_a_status_message_when_backup_fails、tui_write_backs_up_both_sides_before_writing_them、tui_write_changes_neither_side_when_one_side_cannot_be_backed_up、tui_starts_and_shows_a_diff_without_store_location | 新しく書いた。handler の execute_merge と execute_write_changes、runtime の bootstrap_tui_with_targets を関数として呼ぶ |

- REQ-backup-017 の --delete は、計画では tests/contract/merge_paths.rs の deletion_fails_without_removing_a_file_when_backup_cannot_be_saved の印に REQ-backup-017 を足すことになっていた。このテストは error が "backup failed" を含むことだけを見ており、要件の "backup failed: " に続けて原因を示す形を確かめないため印を足さず、形まで確かめるテストを書いた。元のテストは EX-merge-033 の根拠として残す。
- REQ-backup-018 の「期限切れの整理もせずに」は、集約先の場所が決まらないときの話で、その実行からは整理する対象が見えないため、テストで観測できる違いがない。書き込むことと結果に backup が出ないことを確かめる。
- src/handler/merge_file_io.rs の decide_backup_write を直接呼ぶテストは根拠にしない（削除候補に挙げる）。

### セッションと期限（docs/ir/backup/sessions.md）

根拠テストは全て `tests/contract/backup_sessions.rs` にある。
REQ-backup-022 と REQ-backup-023 の性質テストは proptest を使い、公開された関数 `remote_merge::backup::next_session_id` と `compare_session_ids` を通して確かめる。
試す入力の数は上書きせず、proptest の既定の 256 件のまま。
これまでの実行で失敗入力は見つかっておらず、`proptest-regressions/` のファイルはできていない。

| 要件 | 根拠テスト | 元のテストと変えたところ |
|---|---|---|
| REQ-backup-021 | remote_targets_differing_only_in_port_keep_separate_sessions、remote_targets_differing_only_in_host_keep_separate_sessions | 新しく書いた。書いた時点の実装で通ることを最初の実行で確かめ、`RemoteTargetIdentity` の host と port を定数に書き換える一時的な変更では両方とも失敗することを確かめた |
| REQ-backup-021 | aliases_of_one_host_are_separate_write_targets、remote_targets_differing_only_in_root_dir_keep_separate_sessions、login_user_does_not_distinguish_remote_targets | 新しく書いた。src/backup/mod.rs の書き込み先の識別の単体テスト（ホストの別名、ユーザー名）を一覧を通した形にしたもの |
| REQ-backup-021 | rollback_target_uses_only_the_write_target_its_name_now_points_to | 新しく書いた。設定でサーバ名の指す書き込み先を変えると、前の書き込み先のセッションを一覧にも rollback にも使わない |
| REQ-backup-021 | relative_local_root_is_identified_from_the_directory_the_config_was_loaded_in | 新しく書いた。src/backup/mod.rs の relative_local_root_is_resolved_from_startup_directory を公開された入口から確かめる形にしたもの。相対の root_dir でローカルへ書き込むため、結合テスト用の一時ディレクトリ（CARGO_TARGET_TMPDIR）を使う |
| REQ-backup-021 | sessions_for_two_write_targets_remain_separate、local_root_symlink_retargeting_keeps_existing_sessions_visible | tests/local_backup_store.rs から移した。前者は各一覧のセッションとファイルが一件だけであることを書き足した |
| REQ-backup-022 | created_session_ids_follow_the_timestamp_and_suffix_format、session_ids_order_by_time_then_numeric_suffix | 新しく書いた性質テスト。前者は任意の既存 ID に対して作られる ID が日時で始まり、同じ日時の ID があるときだけ 2 以上の "-N" が付くこと、後者は任意の二つの ID の比較が日時、同じ日時なら N の数値の順（"-N" なしが最初）に一致すること |
| REQ-backup-022 | tenth_session_sorts_after_ninth_session、same_time_uses_second_session_suffix | src/backup/mod.rs の単体テストを移した（決定記録 2026-09-27-backup-session-id-properties の A3 で残す具体例） |
| REQ-backup-022 | same_second_sessions_are_listed_newest_first、rollback_accepts_a_same_second_session_id_with_numeric_suffix、rollback_without_session_uses_the_newest_numeric_suffix | tests/local_backup_store.rs から移した。一覧の順と --session の受け付けと省略時の最新の選択。前者の元のテストにあった「何も記録しない操作は一覧に出ない」確認は REQ-backup-015 の根拠テストが受け持つ |
| REQ-backup-023 | next_session_id_differs_from_every_existing_id | 新しく書いた性質テスト。同じ日時や前後一秒の任意の既存 ID の集合に対して、次に作る ID がどれとも重ならない |
| REQ-backup-023 | same_time_uses_second_session_suffix、merges_started_in_the_same_second_use_distinct_session_ids、concurrent_merges_use_distinct_session_ids | 前者は上の行と同じ。後の二つは tests/local_backup_store.rs から移した（集約先を通した同じ秒・並行の作成） |
| REQ-backup-024 | sync_uses_one_session_id_for_all_targets | tests/local_backup_store.rs から移した。各書き込み先の一覧にそのセッションが出ることを書き足した |
| REQ-backup-025 | merge_removes_expired_sessions_when_it_starts、sync_removes_expired_sessions_when_it_starts、dry_run_merge_and_sync_keep_expired_sessions、rollback_in_any_mode_keeps_expired_sessions、disabled_backup_merge_still_removes_expired_sessions、tui_start_removes_expired_sessions | tests/local_backup_store.rs の期限切れの整理の六件を移した。dry-run は sync の場合を書き足した |
| REQ-backup-026 | listing_and_cleanup_share_the_retention_boundary | 新しく書いた。src/backup/mod.rs の session_expires_at_retention_boundary と src/service/rollback.rs の mark_expired の四件を、一覧と整理を通して境界の一秒前と境界ちょうどで確かめる形にしたもの |
| REQ-backup-026 | expired_session_is_marked_in_text_and_json_at_the_injected_boundary | tests/local_backup_store.rs から移した |

- 性質テストの N は 1000 までに限った。N は同じ一秒の間に作られたセッションの数で、それを超える入力を作る操作がないため。コミットに含めない一時的な実行で、既存の ID に N が u64 の最大値のものがあると `next_session_id` が加算のあふれで panic することを確かめた（`attempt to add with overflow`、src/backup/mod.rs:91）。集約先の予約ディレクトリを手で作らない限り起きないため性質の入力には含めていない。FLAG として記録する（利用者の判断）。
- REQ-backup-025 の「集約先の場所が決まらないときは整理しない」は、その実行から整理の対象が見えないため観測できる違いがなく、テストにしていない。
- tests/local_backup_store.rs の merge_keeps_expired_sessions_for_targets_absent_from_config は REQ-backup-025 の文にない「設定から外れた書き込み先の履歴は残す」を確かめるもので、tests/contract/backup_cleanup.rs の EX-backup-017 のテストと重なるため移さず、削除候補に挙げる。

### 書き戻しと rollback コマンド（docs/ir/backup/rollback-path.md、docs/ir/backup/rollback-cli.md）

根拠テストは、書き戻し方を `tests/contract/backup_rollback_path.rs`、コマンドの振る舞いのうち関数呼び出しで確かめるものを `tests/contract/backup_rollback_cli.rs`、実行ファイルを起動して確かめるものを `tests/contract/backup_rollback_cli_e2e.rs` に置いた。
最後のモジュールは隔離された SSH fixture を使うため、ほかの SSH のテストと同じく `test-utils` の feature があるときだけ組み込む。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。

| 要件 | 根拠テスト | 元のテストと変えたところ |
|---|---|---|
| REQ-backup-027 | enabled_rollback_saves_the_current_content_in_a_new_session_before_restoring | tests/local_backup_store.rs の enabled_rollback_reports_the_backup_taken_before_restore。元は pre_rollback_backup の ID だけを見ていたため、その ID のセッションが一覧にあり、それを戻すと書き戻す前の内容になることを書き足した |
| REQ-backup-027 | rollback_does_not_restore_a_file_when_its_current_content_cannot_be_backed_up | tests/local_backup_store.rs から移した。パスと "backup failed: " の後の原因を書き足した |
| REQ-backup-027, REQ-backup-030 | rollback_recreates_a_file_removed_by_merge_delete_without_a_pre_rollback_backup | tests/local_backup_store.rs の rollback_restores_a_file_removed_by_merge_delete。戻す先にファイルがないとき新しいセッションを作らないことを書き足した |
| REQ-backup-028 | disabled_rollback_restores_without_saving_the_current_content | 新しく書いた |
| REQ-backup-029 | rollback_restores_file_content_without_changing_existing_permissions | tests/local_backup_store.rs から移した。所有者（uid）が変わらないことを書き足した |
| REQ-backup-031 | rollback_skips_deleted_file_when_its_parent_no_longer_exists | tests/local_backup_store.rs から移した |
| REQ-backup-032 | a_recorded_symlink_is_not_restored_with_or_without_force | 新しく書いた。src/service/rollback.rs の replaced_symlink_takes_priority_over_a_changed_destination を、CoreRuntime::save_backup で symlink を記録し、その後に通常ファイルへ置き換えた状態で rollback を呼ぶ形にしたもの。--force なしは確認を避けるため dry-run で確かめる |
| REQ-backup-033 | rollback_skips_a_file_after_the_target_root_symlink_is_retargeted、rollback_keeps_a_symlink_that_replaced_the_recorded_regular_file、rollback_skips_a_dangling_symlink_as_a_changed_destination、rollback_skips_a_deleted_file_after_its_parent_symlink_is_retargeted、rollback_skips_a_recorded_symlink_changed_by_a_third_party | tests/local_backup_store.rs のパスの変化のスキップ五件を移した（最後のものの元の名前は rollback_skips_a_recorded_symlink_without_replacing_the_current_file）。スキップが一件で何も戻さないことをそろえて確かめる |
| REQ-backup-034 | rollback_restores_every_file_of_the_newest_session、rollback_restores_only_the_selected_older_session | tests/cli_rollback.rs の test_rollback_multiple_files と test_rollback_specific_older_session を移した。後者は一秒待つ代わりに同じ秒の "-N" 付きの ID を使う |
| REQ-backup-035 | rollback_asks_before_restoring_and_restores_only_after_y_or_yes、rollback_restores_after_yes | 新しく書いた。標準入力から "n"、空の応答、"y"、"yes" を与え、確認の文言と書き戻しの有無を確かめる。断ったときの終了コードは FLAG-backup-004 の範囲のため確かめない |
| REQ-backup-036 | sensitive_files_are_skipped_without_force_and_restored_with_it | 新しく書いた。--force なしは確認を避けるため dry-run で理由 "sensitive" のスキップを、--force ありは書き戻しを確かめる |
| REQ-backup-037 | rollback_text_marks_each_file_and_summarises_the_counts、rollback_text_summary_has_no_counts_when_nothing_was_skipped_or_failed、rollback_json_has_the_result_fields | 新しく書いた。戻したもの・スキップ・失敗を一つずつ含む実際の書き戻しの結果を使う。後者の JSON は tests/local_backup_store.rs の aggregate_rollback_json_keeps_the_existing_field_names_and_types を置き換えた。集計行は "Restored N file(s)" の後の言い回しを IR が定めていないため、数が続くことだけを確かめる |
| REQ-backup-038 | rollback_exit_codes_follow_the_result、rollback_exits_with_2_when_every_file_is_skipped_or_failed | 新しく書いた。src/service/rollback.rs の exit_code で始まる五件と src/cli/rollback.rs の test_rollback_exit_code で始まる三件を、execute_rollback の終了コードで確かめる形にしたもの |
| REQ-backup-038 | dry_run_reports_the_same_changed_path_skip_with_exit_code_0 | tests/local_backup_store.rs の dry_run_reports_the_same_changed_path_skip_without_writing を移した（スキップがあっても dry-run は 0） |
| REQ-backup-038 | rollback_exits_with_2_when_the_session_is_not_found | tests/cli_rollback.rs の test_rollback_exit_code_no_sessions を移し、存在しない --session を指定した場合を書き足した |
| REQ-backup-039 | target_is_required_except_for_list_which_defaults_to_local | 新しく書いた。--target なしの書き戻しがエラーで何も書かないことと、--list の --target 省略がローカルのセッションを一覧することを確かめる |
| REQ-backup-040 | written_target_lists_its_session_with_file_sizes_in_text_and_json | tests/local_backup_store.rs の written_target_lists_its_aggregate_backup_session。テキストの "path (N bytes)" と JSON の値を書き足した |
| REQ-backup-040 | updated_symlink_is_listed_as_a_symlink_in_text_and_json | tests/local_backup_store.rs から移した |
| REQ-backup-040 | same_second_sessions_are_listed_newest_first、expired_session_is_marked_in_text_and_json_at_the_injected_boundary | tests/contract/backup_sessions.rs の既存の根拠テストに印を足した（新しい順と "[expired]"・"expired": true） |
| REQ-backup-041 | rollback_treats_a_session_with_missing_content_as_not_found、session_with_missing_content_is_omitted_without_failing_the_list | tests/local_backup_store.rs から移した |

- REQ-backup-038 の「戻したファイルがないとき 2」のうち、スキップも失敗もなく戻したファイルもない結果は、ファイルのないセッションが一覧に出ない（REQ-backup-015）ため公開された入口から作れない。src/service/rollback.rs の exit_code_empty と src/cli/rollback.rs の test_rollback_exit_code_empty_restored が確かめるのはこの場合で、公開された入口を通した形にはしていない。
- 同じ振る舞いを確かめる CLI のテストと関数呼び出しのテストのうち、根拠にしなかった方は削除候補の節に挙げる。

## 削除候補と利用者の判断

整理の計画で削除を利用者が一括で判断する段の入力と、その判断の結果。

### 判断の結果

利用者が一覧を一度に見て、次のとおり決めた。決まったものだけを消し、消した後に `cargo nextest run --all-features` が通ることを確かめた（2,845 件）。

- A の 11 件は全て消した。
- B の 41 件は全て残した。公開された入口を通さない純粋関数の単体テストで、速く結果が分かることと、plan_restore_empty_session・exit_code_empty・test_rollback_exit_code_empty_restored・missing_backup_store_allows_write_when_backup_is_disabled のように公開された入口から作れない場合を確かめるものがあるため。印は付けない。
- C の 15 件のうち 13 件を消した。tests/cli_rollback_local.rs の test_rollback_list_no_backups と test_rollback_list_format_json_empty の二件は、利用者が目にする空の一覧の出力（テキストと JSON）をほかに確かめるテストがないため残した。印は付けない。C のうち tests/cli_rollback.rs の七件を消したことで、このファイルにはテストがなくなったため、ファイルごと消した。
- 計画の範囲を超える変更として、利用者の承認を得て、製品コードから呼ばれていない関数 `parse_batch_restore_output`（src/service/rollback.rs）と `extract_timestamp`（src/backup/mod.rs）を消した。消す前に `rg` でテスト以外の呼び出し元がないことを確かめた。呼び出し元がないため挙動は変わらない。A のうちこの二つの関数のテスト八件は、関数と同じコミットで消した。

以下は判断の入力にした一覧で、判断前の内容のまま残す。
一覧は、取り込みの決定記録（docs/decision/records/2026-09-27-adopt-backup.md）の Context で「実装詳細をなぞるだけ」と数えた十一件（A）と、上の審査で根拠にしなかった純粋関数の比較だけのテスト（B）と重複テスト（C）を合わせた 67 件。
「代わりの根拠」は、そのテストが確かめていた振る舞いを今確かめている根拠テスト。
FLAG の挙動を確かめる三件（trailing_slash_does_not_change_remote_target_identity、rollback_reports_a_cyclic_symlink_as_an_unresolvable_path、rollback_reports_a_cyclic_parent_symlink_as_an_unresolvable_path）と、この計画の対象外の要件（集約先の場所、REQ-backup-001 から 010）を確かめる src/backup/mod.rs の backup_store_uses_home_ で始まる二件と backup_store_is_unavailable_without_xdg_data_home_or_home は候補にしていない。

### A. 実装詳細をなぞるだけのテスト（11 件）

| ファイル | テスト | 理由 |
|---|---|---|
| src/service/rollback.rs | parse_batch_output_ok_and_fail | parse_batch_restore_output の出力の分け方を見るだけ。この関数は製品コードから呼ばれていない |
| src/service/rollback.rs | parse_batch_output_only_ok | 同上 |
| src/service/rollback.rs | parse_batch_output_only_fail | 同上 |
| src/service/rollback.rs | parse_batch_output_empty | 同上 |
| src/service/rollback.rs | parse_batch_output_ignores_noise | 同上 |
| src/service/rollback.rs | parse_batch_output_fail_with_colon_in_reason | 同上 |
| src/backup/mod.rs | test_extract_timestamp_valid | extract_timestamp の戻り値を見るだけ。この関数は製品コードから呼ばれていない |
| src/backup/mod.rs | test_extract_timestamp_invalid | 同上 |
| src/backup/mod.rs | test_parse_backup_timestamp | 非公開の parse_backup_timestamp の年月日を見るだけ。形式と順序は REQ-backup-022 の性質テストが確かめる |
| src/cli/rollback.rs | test_resolve_target_with_name | 非公開の resolve_target がサーバ名を Side に変えることを見るだけ。rollback --target の振る舞いは REQ-backup-021・039 の根拠テストが確かめる |
| src/cli/rollback.rs | test_resolve_target_local | 同上 |

parse_batch_restore_output と extract_timestamp は呼び出し元がないため、テストを消すと整理後の変異テストでこれらの関数の変異が見逃しとして現れる見込みだった。利用者の判断で関数そのものを消したため、これらの変異は生じない。

### B. 純粋関数の比較だけのテスト（41 件）

| ファイル | テスト | 代わりの根拠 |
|---|---|---|
| src/service/rollback.rs | replaced_symlink_takes_priority_over_a_changed_destination | a_recorded_symlink_is_not_restored_with_or_without_force（REQ-backup-032） |
| src/service/rollback.rs | changed_existing_destination_is_skipped | rollback_skips_a_file_after_the_target_root_symlink_is_retargeted ほか REQ-backup-033 の五件 |
| src/service/rollback.rs | unchanged_existing_destination_is_restored | enabled_rollback_saves_the_current_content_in_a_new_session_before_restoring ほか既存ファイルを戻す根拠テスト |
| src/service/rollback.rs | missing_parent_is_skipped_before_its_location_is_compared | rollback_skips_deleted_file_when_its_parent_no_longer_exists（REQ-backup-031） |
| src/service/rollback.rs | missing_file_under_changed_parent_is_skipped | rollback_skips_a_deleted_file_after_its_parent_symlink_is_retargeted（REQ-backup-033） |
| src/service/rollback.rs | missing_file_under_unchanged_parent_is_restored | rollback_recreates_a_file_removed_by_merge_delete_without_a_pre_rollback_backup（REQ-backup-030） |
| src/service/rollback.rs | mark_expired_within_retention | listing_and_cleanup_share_the_retention_boundary（REQ-backup-026） |
| src/service/rollback.rs | mark_expired_past_retention | 同上 |
| src/service/rollback.rs | mark_expired_boundary | 同上 |
| src/service/rollback.rs | session_is_not_expired_one_second_before_retention_boundary | 同上 |
| src/service/rollback.rs | plan_restore_auto_select_latest | rollback_restores_every_file_of_the_newest_session、rollback_without_session_uses_the_newest_numeric_suffix（REQ-backup-034・022） |
| src/service/rollback.rs | plan_restore_specific_session | rollback_restores_only_the_selected_older_session（REQ-backup-034） |
| src/service/rollback.rs | plan_restore_all_expired_error | tests/contract/rollback_paths.rs の expired_backup_is_not_restored_without_force（EX-backup-013） |
| src/service/rollback.rs | plan_restore_expired_force | tests/contract/rollback_paths.rs の expired_backup_with_saved_data_is_restored_when_forced（EX-backup-014） |
| src/service/rollback.rs | plan_restore_sensitive_skipped | sensitive_files_are_skipped_without_force_and_restored_with_it（REQ-backup-036） |
| src/service/rollback.rs | plan_restore_sensitive_force | 同上 |
| src/service/rollback.rs | plan_restore_no_sessions_error | rollback_exits_with_2_when_the_session_is_not_found（REQ-backup-038） |
| src/service/rollback.rs | plan_restore_session_not_found | 同上（存在しない --session） |
| src/service/rollback.rs | plan_restore_empty_session | なし。ファイルのないセッションは一覧に出ない（REQ-backup-015）ため、公開された入口から作れない場合 |
| src/service/rollback.rs | exit_code_all_success | rollback_exit_codes_follow_the_result（REQ-backup-038） |
| src/service/rollback.rs | exit_code_partial_failure | 同上 |
| src/service/rollback.rs | exit_code_is_error_when_a_path_is_skipped | rollback_exits_with_2_when_every_file_is_skipped_or_failed（REQ-backup-038） |
| src/service/rollback.rs | exit_code_all_failed | 同上 |
| src/service/rollback.rs | exit_code_empty | なし。plan_restore_empty_session と同じく公開された入口から作れない場合 |
| src/cli/rollback.rs | test_resolve_target_none_list_mode_defaults_to_local | target_is_required_except_for_list_which_defaults_to_local（REQ-backup-039） |
| src/cli/rollback.rs | test_resolve_target_none_non_list_mode_errors | 同上 |
| src/cli/rollback.rs | test_rollback_exit_code_success | rollback_exit_codes_follow_the_result（REQ-backup-038） |
| src/cli/rollback.rs | test_rollback_exit_code_failure_with_failed | 同上 |
| src/cli/rollback.rs | test_rollback_exit_code_empty_restored | なし。exit_code_empty と同じ場合 |
| src/backup/mod.rs | remote_targets_with_different_users_have_the_same_identity | login_user_does_not_distinguish_remote_targets（REQ-backup-021） |
| src/backup/mod.rs | remote_targets_with_different_hosts_have_different_identities | remote_targets_differing_only_in_host_keep_separate_sessions（REQ-backup-021） |
| src/backup/mod.rs | remote_targets_with_different_ports_have_different_identities | remote_targets_differing_only_in_port_keep_separate_sessions（REQ-backup-021） |
| src/backup/mod.rs | host_aliases_have_different_remote_target_identities | aliases_of_one_host_are_separate_write_targets（REQ-backup-021） |
| src/backup/mod.rs | relative_local_root_is_resolved_from_startup_directory | relative_local_root_is_identified_from_the_directory_the_config_was_loaded_in（REQ-backup-021） |
| src/backup/mod.rs | local_target_identity_does_not_follow_root_symlink | local_root_symlink_retargeting_keeps_existing_sessions_visible（REQ-backup-021） |
| src/backup/mod.rs | session_id_with_sequence_is_parsed | session_ids_order_by_time_then_numeric_suffix（REQ-backup-022 の性質テスト） |
| src/backup/mod.rs | session_expires_at_retention_boundary | listing_and_cleanup_share_the_retention_boundary（REQ-backup-026） |
| src/handler/merge_file_io.rs | backup_failure_refuses_write_with_status_message | tui_merge_is_refused_with_a_status_message_when_backup_fails（REQ-backup-020） |
| src/handler/merge_file_io.rs | write_to_both_sides_is_refused_when_either_backup_fails | tui_write_changes_neither_side_when_one_side_cannot_be_backed_up（REQ-backup-020） |
| src/handler/merge_file_io.rs | missing_backup_store_refuses_write_when_backup_is_enabled | tui_merge_is_refused_with_a_status_message_when_backup_fails（REQ-backup-020。集約先の場所が決まらない場合そのものではない） |
| src/handler/merge_file_io.rs | missing_backup_store_allows_write_when_backup_is_disabled | なし。TUI でバックアップ無効のときに書き込むことは REQ-backup-020 の文にない |

計画は decide_backup_write のテストを三件としていたが、decide_backup_write を直接呼ぶテストは四件あるため四件とも挙げた。

### C. 根拠にしなかった重複テスト（15 件）

| ファイル | テスト | 理由 |
|---|---|---|
| tests/local_backup_store.rs | merge_keeps_expired_sessions_for_targets_absent_from_config | tests/contract/backup_cleanup.rs の cleanup_retains_expired_history_of_a_server_no_longer_configured（EX-backup-017）と同じ振る舞い |
| tests/local_backup_store.rs | rollback_restores_through_an_unchanged_intermediate_symlink | tests/contract/rollback_paths.rs の unchanged_parent_link_allows_every_file_in_the_session_to_be_restored（EX-backup-006）と同じ振る舞い |
| tests/cli_rollback.rs | test_merge_then_rollback_restores_content | rollback_restores_every_file_of_the_newest_session（REQ-backup-034）と重なる |
| tests/cli_rollback.rs | test_rollback_nested_directory | 同上（ネストしたパスでも同じ経路） |
| tests/cli_rollback.rs | test_rollback_list_after_merge | written_target_lists_its_session_with_file_sizes_in_text_and_json（REQ-backup-040）と重なり、見出しとファイル数だけを見る |
| tests/cli_rollback.rs | test_rollback_list_json_after_merge | 同上（JSON） |
| tests/cli_rollback.rs | test_rollback_dry_run_shows_plan_without_changes | dry_run_reports_the_same_changed_path_skip_with_exit_code_0（REQ-backup-038）と tests/contract/rollback_paths.rs の previewing_a_two_file_rollback_reports_both_and_leaves_them_unchanged（EX-backup-015）と重なる |
| tests/cli_rollback.rs | test_rollback_skips_sensitive_without_force | sensitive_files_are_skipped_without_force_and_restored_with_it（REQ-backup-036）と重なり、理由 "sensitive" と --force の場合を見ない |
| tests/cli_rollback.rs | test_rollback_json_output_structure | rollback_json_has_the_result_fields（REQ-backup-037）と重なる |
| tests/cli_rollback_local.rs | test_rollback_list_no_backups | 空の一覧の "(no backup sessions found)" は IR にない文言で、どの要件の根拠でもない |
| tests/cli_rollback_local.rs | test_rollback_target_required_without_list | target_is_required_except_for_list_which_defaults_to_local（REQ-backup-039）と重なる |
| tests/cli_rollback_local.rs | test_rollback_list_default_target_local | 同上。終了コード 0 だけを見てローカルが対象になったかを見ない |
| tests/cli_rollback_local.rs | test_rollback_list_format_json_empty | 空の一覧の JSON が空の配列であることだけを見る。どの要件の根拠でもない |
| tests/cli_rollback_local.rs | test_rollback_no_sessions_error | rollback_exits_with_2_when_the_session_is_not_found（REQ-backup-038）と重なる |
| tests/cli_rollback_local.rs | test_rollback_invalid_session_id | 同上（存在しない --session のエラー） |
