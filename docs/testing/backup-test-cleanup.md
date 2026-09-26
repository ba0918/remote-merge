# バックアップと rollback のテスト整理の記録

バックアップと rollback の要件（REQ-backup-011 から REQ-backup-041）の根拠テストを整理した過程の記録。
変異テストの結果、各要件の根拠にしたテスト、削除候補と利用者の判断、整理後の見逃しの決着を残す。

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

- 性質テストの N は 1000 までに限った。N は同じ一秒の間に作られたセッションの数で、それを超える入力を作る操作がないため。コミットに含めない一時的な実行で、既存の ID に N が u64 の最大値のものがあると `next_session_id` が加算のあふれで panic することを確かめた（`attempt to add with overflow`、src/backup/mod.rs:91）。集約先の予約ディレクトリを手で作らない限り起きないため性質の入力には含めていない。FLAG-backup-005 として記録した（利用者の判断）。
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

## 整理後の変異テスト

削除と根拠テストの追加を終えた後、整理前と同じコマンドで三つのファイルを一回の実行にまとめて実行した。
一回目の実行の途中で作業ツリーのテストを書き足したため（変異ごとのテストの数が 2,845 と 2,846 に分かれた）、テストと同等変異の一覧を確定させてから、作業ツリーに触れずにもう一度実行した。
比べる結果はこの二回目の実行（コミット 49cf9f8）で、一回目は検知の有無が変わった変異の確認にだけ使う。

```sh
scripts/mutants.sh src/backup/mod.rs src/service/rollback.rs src/runtime/backup_store.rs
```

二回目の全体の集計は、実行したときの同等変異の一覧で `mutants: caught=80 survived=8 timeout=3 unviable=14 equivalent=3`（実行時間は約 31 分）。
その後に src/service/rollback.rs:156 の変異を同等変異として登録し、同じ結果を読み直した集計は `mutants: caught=80 survived=7 timeout=3 unviable=14 equivalent=4`。下の表はこの読み直しの内訳。
変異の総数は整理前の 126 から 108 に減った。利用者の判断で消した `parse_batch_restore_output` と `extract_timestamp` の変異がなくなったため。

| ファイル | caught | survived | equivalent | timeout | unviable |
|---|---|---|---|---|---|
| src/backup/mod.rs | 23 | 1 | 2 | 3 | 10 |
| src/service/rollback.rs | 26 | 0 | 1 | 0 | 2 |
| src/runtime/backup_store.rs | 31 | 6 | 1 | 0 | 2 |

### 検知の中身の確かめ

変異ごとの実行ログ（`mutants.out/log/`）で、検知とされた変異のそれぞれについて失敗したテストを数えた。
一回目の実行では、次の五つの変異が、バックアップと関係のない tui_merge（PTY で TUI を動かすテスト）か agent_ssh_deploy のテストの失敗だけで検知とされていた。
二つの変異を同時に並べて走らせる負荷の下で、これらのテストが時間に依存して失敗したものと推測する（未確認）。

| 変異 | 一回目で失敗したテスト |
|---|---|
| src/runtime/backup_store.rs:117 replace == with != in BackupStore::reserve_session | tui_merge の三件 |
| src/runtime/backup_store.rs:242 replace += with *= in BackupStore::cleanup_expired | tui_merge の二件 |
| src/runtime/backup_store.rs:359 replace match guard path == rel_path with true in BackupStore::read_record | tui_merge の一件 |
| src/runtime/backup_store.rs:359 replace match guard path == rel_path with false in BackupStore::read_record | agent_ssh_deploy の一件 |
| src/service/rollback.rs:156 replace match guard force with true in plan_restore | tui_merge の一件 |

二回目の実行では、どの変異のログにも tui_merge と agent_ssh_deploy のテストの失敗はなく、失敗したテストは製品の単体テスト、tests/cli_merge.rs、tests/contract/ のものだけだった。
一回目の見かけの集計（caught=82 survived=9）はこのため二回目より検知が多く、二回目の集計を正とする。
これらの時間に依存するテストの修正はこの整理の範囲外で、手を入れていない。

`src/backup/mod.rs:112` の `replace > with < in parse_session_id` は、S1 の確認の実行で検知、整理前の実行で見逃し、整理後の一回目と二回目で見逃しだった。
この変異で違いが出るのは "-1" や "-0" のように N が 2 より小さい接尾辞を持つ名前を読むときだけで、製品はそのような ID を作らず、性質テストの入力にも含まれない。
S1 の実行で検知された理由は、上の表と同じく関係のないテストの失敗だった可能性が高いと推測する（S1 のログは残っておらず未確認）。

### 整理前との比べ

整理後の見逃し（同等変異として登録したものを含む）は、`src/service/rollback.rs:156` の一件を除いて整理前の見逃しに含まれる。

- `src/service/rollback.rs:156` の `replace match guard force with true in plan_restore` は整理前には検知だったが、整理前の実行のログは残っておらず、何で検知されたかは確かめられない。整理後の一回目では関係のないテストの失敗だけで検知とされていた。この変異は、全てのセッションが期限切れで --force がないときに最新のセッションを選ぶが、直後の「期限切れのセッションは --force なしでは戻さない」の確かめで同じエラーになる。消したテストはどれもこの関数を呼んでおらず（消した単体テストは parse_batch_restore_output と extract_timestamp と resolve_target のものだけ、plan_restore の単体テストは全て残した）、削除が生んだ見逃しではないと判断した。戻したテストはない。
- 整理前の見逃しのうち次の七件は整理後に検知された。
  - src/backup/mod.rs:45 と 49 の四件（書き込み先の識別のホストとポート）: REQ-backup-021 のポートだけ・ホストだけが違う書き込み先のテストと、REQ-backup-012 のテストが検知した。
  - src/runtime/backup_store.rs:117 の `replace == with !=`: REQ-backup-023 の同時に始めた多数の merge のテスト（下の表）が検知した。
  - src/runtime/backup_store.rs:310 の `replace && with ||`: REQ-backup-041 の二つのファイルのどちらかの内容が欠けたセッションのテスト（下の表）が検知した。
  - src/runtime/backup_store.rs:359 の `replace match guard path == rel_path with true` は整理後の一回目では関係のないテストだけで検知、二回目で見逃しで、整理前と同じく見逃しとして扱う。

時間切れの三件（`src/backup/mod.rs:84`、`88`、`91` の next_session_id の変異）は見逃しではない（kotowari mutants の notice）。
これらの変異は既存の ID と重なる ID を返し、集約先の予約が同じ ID を作り直し続けて終わらなくなる。
整理前はこれを src/backup/mod.rs の単体テストがすぐに失敗させて検知になっていたが、それらのテストを tests/contract/ へ移した後は、終わらなくなるテストの時間切れが先に記録されるようになったと推測する。

### 見逃しの決着

| 位置 | 変異 | 決着 |
|---|---|---|
| src/backup/mod.rs:79 | replace backup_timestamp -> String with String::new() | 同等変異として登録した。下の注 1 |
| src/backup/mod.rs:79 | replace backup_timestamp -> String with "xyzzy".into() | 同等変異として登録した。下の注 1 |
| src/runtime/backup_store.rs:242 | replace += with *= in BackupStore::cleanup_expired | 同等変異として登録した。下の注 2 |
| src/service/rollback.rs:156 | replace match guard force with true in plan_restore | 同等変異として登録した。下の注 5 |
| src/runtime/backup_store.rs:117 | replace match guard error.kind() == std::io::ErrorKind::AlreadyExists with true in BackupStore::reserve_session | FLAG-backup-006 として記録（利用者の判断）。下の注 3 |
| src/runtime/backup_store.rs:383 | replace == with != in create_temporary_entry | FLAG-backup-006 として記録（利用者の判断）。下の注 3 |
| src/runtime/backup_store.rs:383 | replace match guard error.kind() == std::io::ErrorKind::AlreadyExists with true in create_temporary_entry | FLAG-backup-006 として記録（利用者の判断）。下の注 3 |
| src/runtime/backup_store.rs:383 | replace match guard error.kind() == std::io::ErrorKind::AlreadyExists with false in create_temporary_entry | FLAG-backup-007 として記録（利用者の判断）。下の注 4 |
| src/backup/mod.rs:112 | replace > with < in parse_session_id | FLAG-backup-007 として記録（利用者の判断）。下の注 4 |
| src/runtime/backup_store.rs:347 | replace match guard path == rel_path with true in BackupStore::read_record | FLAG-backup-007 として記録（利用者の判断）。下の注 4 |
| src/runtime/backup_store.rs:359 | replace match guard path == rel_path with true in BackupStore::read_record | FLAG-backup-007 として記録（利用者の判断）。下の注 4 |

同等変異かどうかの判断のため、別の文脈のエージェントに、変異を当てると落ちて今のコードでは通るテストを公開された入口から書かせた（製品コードは変えず、結果は採用するかをこちらで決めた）。

1. backup_timestamp の値は、バックアップが無効なときの merge・sync のセッション ID にだけ使われる。その ID が流れる保存、削除の結果の backup 表示、セッションの片付けは全てバックアップが有効なときの分岐の中にあり、結果の出力にも集約先にも届かない。別の文脈のエージェントも観測できる違いを見つけられなかった。
2. 変わるのは cleanup_expired が返す削除した数だけで、削除そのものは変わらない。この戻り値を使う製品コード（TUI の起動、merge と sync の開始）はエラーかどうかしか見ない。別の文脈のエージェントは CoreRuntime::cleanup_expired_backups の戻り値を直接確かめるテストで落とせたが、削除した数は IR に定めがなく、根拠にならないテストのため採らなかった。
3. 元のコードでは、ID の予約や一時的な保存場所の作成が AlreadyExists 以外の理由で失敗するとエラーで終わり、変異では作り直しを続けて終わらなくなる。直前に同じプロセスが権限を 0700 に直すため、root 権限なしで起こせる失敗は、集約先のパスがおよそ 4,000 バイトのときのパス長の上限（ENAMETOOLONG）だけだった。別の文脈のエージェントは、集約先のパスの長さを 3,900 から 4,094 バイトまで変えて merge が終わることと書き込み先が変わらないことを確かめるテストを書き、元のコードで通り三つの変異で落ちる（終わらない）ことを確かめた。XDG_DATA_HOME は境界で受け取る入力だが、4,000 バイトの集約先が実際に使われる場面として認めるか、また REQ-backup-002 と REQ-backup-017 がこの失敗をファイルごとの失敗として求めているか（予約の失敗は元のコードでもコマンド全体のエラーになる）は仕様の読み方の判断になるため、テストを採らず、同等変異としても登録していない（観測できる違いがあるため）。
4. 違いが出るのは、集約先の中に製品が作らない名前や中身があるときだけ。"-1"・"-0" のような接尾辞の ID（mod.rs:112）、record.json の path が保存場所と食い違う記録（backup_store.rs:347・359）、同じセッションの一時的な保存場所に同じプロセス番号と連番の名前が残っているとき（backup_store.rs:383 の false）がそれにあたる。どれも人が集約先を書き換えない限り起きない。REQ-backup-041 が定めるのは中身が消されたときで、書き換えられたときや他の名前があるときの扱いは IR にない。テストを書くには IR にない扱いと集約先の内部の配置を固定することになるため書かず、観測できる違いはあるため同等変異としても登録していない。利用者の判断で FLAG-backup-007 として残した。
5. この腕に来るのは --session がなく全てのセッションが期限切れのときだけで、変異で選ばれる最新のセッションも期限切れのため、直後の「期限切れのセッションは --force なしでは戻さない」の確かめで元のコードと同じエラーになる。別の文脈のエージェントに execute_rollback と plan_restore の直接の呼び出しから落ちるテストを書かせたが、エラーの種類、文言、出力、終了コードが同じで書けなかった。

### 整理後に書き足した根拠テスト

| 要件 | テスト | 検知するようになった変異 |
|---|---|---|
| REQ-backup-041 | tests/contract/backup_rollback_cli.rs の session_missing_the_content_of_one_of_its_files_is_omitted_from_the_list | src/runtime/backup_store.rs:310 の replace && with \|\|。二つのファイルのどちらの内容を消しても一覧に出ないことを確かめる。読む順番によらず片方の場合で必ず落ちる |
| REQ-backup-023 | tests/contract/backup_sessions.rs の many_simultaneous_merges_all_succeed_with_distinct_session_ids | src/runtime/backup_store.rs:117 の replace == with != と guard を false にする変異。十六本の merge を同じ時刻で同時に始めることを五回繰り返し、全てが成功して ID が重ならないことを確かめる |

どちらも今のコードで通ることを確かめ、コミットに含めない一時的な書き換えで変異を当てて落ちることを確かめた（REQ-backup-023 のテストは guard を false にした変異で八回、!= の変異で五回実行して全て失敗、元のコードで十回実行して全て成功）。

### 要件ごとの verification

具体的な場面で結果が決まる挙動は unit、入力の全体で成り立つべき性質は property という選び方（docs/ir/testing/methods.md#REQ-testing-009）に照らした。

| 要件 | verification | 要件の性質に合う理由 |
|---|---|---|
| REQ-backup-011 | unit | 保存した項目の権限という、一回の保存の結果で決まる状態で、元の権限の組み合わせは 0664・0644 と --delete の場面で足りる |
| REQ-backup-012 | unit | 書き込み先ごとの説明ファイルの中身で、リモートとローカルの二つの場面で結果が決まる |
| REQ-backup-013 | unit | 上書きと削除の直前に読み直した内容が保存されるかは、差分表示の後に内容を変える場面で決まる |
| REQ-backup-014 | unit | 一方向の merge の後に両側のどちらにセッションがあるかという、一つの場面の結果 |
| REQ-backup-015 | unit | 元のファイルがない場面（親ディレクトリの有無の二通り）で記録と一覧がどうなるかで決まる |
| REQ-backup-016 | unit | 既存の ".remote-merge-backup/" がある場面で、その中身・一覧・status の結果を見れば決まる |
| REQ-backup-017 | unit | merge・--delete・sync の三つの経路それぞれでバックアップが失敗する場面の結果 |
| REQ-backup-018 | unit | 集約先が決まらない場面で、バックアップの有効・無効と merge・sync の組み合わせの結果 |
| REQ-backup-019 | unit | 三つのモードと有効・無効の六通りの場面で結果が決まり、入力の全体にわたる性質ではない |
| REQ-backup-020 | unit | TUI の書き込みと起動の具体的な場面（片側の失敗、両側の w、集約先なし）の結果 |
| REQ-backup-021 | unit | 書き込み先の識別は、ポート・ホスト・root_dir・ユーザー名・別名・相対パス・symlink のそれぞれが違う場面で決まる |
| REQ-backup-022 | property | ID の形式と順序は任意の日時と N の組で成り立つべき性質で、十番目と九番目の並びのような具体例だけでは確かめきれない |
| REQ-backup-023 | property | 重複しないことは任意の既存 ID の集合に対して成り立つべき性質。同じ秒・同時の作成は集約先を通した unit のテストでも確かめる |
| REQ-backup-024 | unit | 一回の sync が二つの書き込み先に書く場面で、両方のセッション ID を比べれば決まる |
| REQ-backup-025 | unit | 整理する時点は TUI の起動・merge・sync・dry-run・rollback という有限の場面ごとに決まる |
| REQ-backup-026 | unit | 境界の一秒前と境界ちょうどの二つの場面で、一覧と整理の判定がそろうかで決まる |
| REQ-backup-027 | unit | 有効時の rollback が退避する場面、退避できない場面、戻す先がない場面の結果 |
| REQ-backup-028 | unit | 無効時の rollback の一つの場面の結果 |
| REQ-backup-029 | unit | 権限と所有者を持つ既存ファイルへ書き戻す場面の結果 |
| REQ-backup-030 | unit | --delete で消したファイルを戻す場面の結果 |
| REQ-backup-031 | unit | 親ディレクトリが消えた場面のスキップ理由とディレクトリの有無 |
| REQ-backup-032 | unit | symlink を置き換えた記録を --force の有無で戻す二つの場面 |
| REQ-backup-033 | unit | 要件が挙げる場所の変化の種類ごとの場面で、スキップ理由が決まる |
| REQ-backup-034 | unit | 最新のセッションと古いセッションを選ぶ場面の書き戻しの結果 |
| REQ-backup-035 | unit | 確認への応答（"y"・"yes"・"n"・空）ごとの場面の結果 |
| REQ-backup-036 | unit | 機密ファイルを --force の有無で戻す二つの場面 |
| REQ-backup-037 | unit | 戻したもの・スキップ・失敗を含む一つの結果の出力形式 |
| REQ-backup-038 | unit | 結果の種類（全て成功、一部失敗、全てスキップか失敗、セッションなし、dry-run）ごとの終了コード |
| REQ-backup-039 | unit | --target と --list の有無の組み合わせの場面 |
| REQ-backup-040 | unit | 通常ファイル・symlink・期限切れ・同じ秒の複数セッションを含む一覧の出力形式 |
| REQ-backup-041 | unit | 保存内容が欠けた場面（一ファイルのセッション、二ファイルのどちらかが欠けたセッション）の一覧と rollback の結果 |

### 性質テストの確かめ

- REQ-backup-022 と REQ-backup-023 の性質テスト（created_session_ids_follow_the_timestamp_and_suffix_format、session_ids_order_by_time_then_numeric_suffix、next_session_id_differs_from_every_existing_id）は tests/contract/backup_sessions.rs にあり、公開された関数 `next_session_id` と `compare_session_ids` を通して確かめる。
- どれも `proptest!` を設定なしで使い、試す入力の数を上書きしていない（proptest の既定の 256 件）。
- `proptest-regressions/` は、これまでの実行で失敗入力が見つかっていないため存在せず、コミットするファイルはない。
- 既存の ID の N が u64 の最大値のときに next_session_id が加算のあふれで panic することは、FLAG-backup-005 として記録した（利用者の判断）。
