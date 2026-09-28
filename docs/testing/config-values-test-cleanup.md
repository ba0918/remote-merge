# 設定の値の検査のテスト整理の記録

設定の値の検査の取り込み（[決定記録](../decision/records/2026-09-28-adopt-config-values.md)）で加えた要件（REQ-config-014 から REQ-config-020）の根拠テストを整えた過程の記録。
各要件の根拠にしたテスト、変異テストの結果、見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テスト 56 件のうち 1 件（src/config.rs の test_parse_permissions_field_wrapper）が実装の中身をなぞるだけのものだったが、元の単体テストは消さない方針のため、この整理ではテストを消していない。
テストを削除しない整理のため、整理前の変異テストは回していない（REQ-testing-012、[決定記録 A2](../decision/records/2026-09-28-mutation-scope.md#A2)）。

## 要件ごとの根拠テスト

手本にした元のテストは消さず、書き換えていない。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた（既存の挙動を確かめるテストのため、失敗する段階はない）。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
FLAG-config-009 から 012 の挙動（3 桁でないパーミッション、auth が "key" のサーバの password の警告、平文のパスワードの警告とパスワードがどこにもないときの扱い、sudo と agent の組み合わせ）は確かめない。

### 値の読み方・検査・権限の選び方・走査の上限（REQ-config-014 から 018）

根拠テストは `tests/contract/config_values.rs` にある（feature は要らない）。
値の読み方とエラーの文言は、前の回の `tests/contract/config_merging.rs` と同じく、公開の関数 `remote_merge::config::load_config_from_paths` に一時ディレクトリの設定ファイルを渡し、返った `AppConfig` の値か、エラーを `format!("{err:#}")`（原因の連鎖を含めた表示）にした文字列で確かめる。
エラーの文字列は要件の文言の全体と一致すること（`assert_eq!`）で確かめる。
設定にはサーバ develop（正しい値）と、本文の後ろに root_dir のある [local] を書く。[local] を後ろに書くのは、トップレベルのキー（max_scan_entries・badge_scan_max_files）が [local] の中のキーとして読まれないようにするためである。
status・diff・merge・sync のエラーが終了コード 2 になることは `src/main.rs` の `handle_with_format` の共通の経路で決まり、前の回の REQ-config-007 から 009 の実行ファイルのテストがその経路を確かめているため、ここでは確かめ直さない。

| 要件 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| REQ-config-014（port） | server_port_zero_stops_with_the_port_reason | src/config.rs の test_invalid_port・test_error_messages_are_english。port = 0 で "Invalid config value: servers.develop.port - port must be >= 1" |
| REQ-config-014（auth） | server_auth_other_than_key_or_password_stops_with_the_auth_reason | src/config.rs の test_password_auth・test_error_messages_are_english。auth が "token"・"Password"・空の文字列のそれぞれで "Invalid config value: servers.develop.auth - invalid auth value: '値' (expected 'key' or 'password')" |
| REQ-config-014（root_dir） | server_empty_root_dir_stops_with_the_root_dir_reason | src/config.rs の test_empty_root_dir。root_dir が空で "Invalid config value: servers.develop.root_dir - root_dir must not be empty" |
| REQ-config-015（読み方） | permissions_with_or_without_the_0o_prefix_are_read_as_octal | src/config.rs の test_parse_permissions_with_0o_prefix・test_parse_permissions_with_leading_zero・test_parse_permissions_bare_digits。servers.develop.file_permissions・servers.develop.dir_permissions・defaults.file_permissions・defaults.dir_permissions の四つのキーのそれぞれで、"0o664"・"0664"・"664" が 0o664 として読まれる。0o664 は defaults.file_permissions の既定値でもあり、キーを無視しても同じ値になるため、既定値と違う "0o751"・"0751"・"751" が 0o751 として読まれることも確かめる |
| REQ-config-015（8 進数以外） | permissions_with_a_non_octal_digit_stop_with_the_key_name | src/config.rs の test_parse_permissions_invalid_strings・test_invalid_server_file_permissions_rejected・test_invalid_defaults_file_permissions_rejected。四つのキーのそれぞれで "0o668"・"rw-r--r--"・"0o-64" が "Invalid config value: キー名 - invalid permissions string: '値' (must contain only octal digits 0-7)" |
| REQ-config-015（数字がない） | permissions_without_digits_stop_with_the_key_name | 同じ元のテスト。四つのキーのそれぞれで "0o" と空の文字列が "Invalid config value: キー名 - invalid permissions string: '値' (empty octal digits)" |
| REQ-config-015（0o777 を超える） | permissions_above_0o777_stop_with_the_key_name | 同じ元のテスト。四つのキーのそれぞれで "0o1000" と "4755" が "Invalid config value: キー名 - invalid permissions value: '値' (must be <= 0o777, got 0o1000)"（"4755" は got 0o4755）。u32 に収まらない値は IR にない文言のエラーになるため使わない |
| REQ-config-016（ファイル） | merge_creates_a_new_file_with_the_server_then_defaults_then_default_mode | src/config.rs の test_resolve_file_permissions_* と tests/contract/merge_paths.rs の new_file_uses_destination_configured_mode。`RuntimeTargets::with_local` で develop をローカルのディレクトリに差し替え、`execute_merge` で新しいファイルを書き込み、その mode が、サーバと [defaults] の両方に書いたときはサーバの 0o640、[defaults] にだけ書いたときは [defaults] の 0o604、どちらにもないときは既定値の 0o664 になる |
| REQ-config-016（ディレクトリ） | sync_creates_a_new_directory_with_the_server_then_defaults_then_default_mode | tests/contract/merge_paths.rs の sync_creates_missing_parent_directory_with_destination_mode。`execute_sync` で書き込み先にない親ディレクトリを作り、その mode が同じ三通りで 0o750・0o705・既定値の 0o775 になる |
| REQ-config-017（設定の max_scan_entries） | config_max_scan_entries_accepts_its_range_and_stops_outside_it | src/config.rs の test_validate_max_scan_entries_*。1 と 1000000 が読まれ、0 と 1000001 が "Invalid config value: max_scan_entries - max_scan_entries must be between 1 and 1,000,000, got 値" |
| REQ-config-017（設定の badge_scan_max_files） | config_badge_scan_max_files_accepts_its_range_and_stops_outside_it | src/config.rs の test_validate_badge_scan_max_files_*。1 と 10000 が読まれ、0 と 10001 が "Invalid config value: badge_scan_max_files - badge_scan_max_files must be between 1 and 10,000, got 値" |
| REQ-config-017（status の --max-entries） | status_max_entries_outside_its_range_stops | src/config.rs の test_resolve_max_entries_*。`tests/contract/status_support.rs` の準備で `execute_status` に max_entries を 0 と 1000001 で渡し、"max_scan_entries must be between 1 and 1,000,000, got 0"・"... got 1000001" のエラーで止まる（値はカンマなしで出る） |
| REQ-config-017（diff の --max-entries） | diff_max_entries_outside_its_range_stops | 同じ元のテスト。同じ準備で `remote_merge::cli::diff::execute_diff` に渡し、同じエラーで止まる |
| REQ-config-017（merge の --max-entries） | merge_max_entries_outside_its_range_stops | 同じ元のテスト。`tests/contract/merge_support.rs` の準備で `execute_merge` に渡し、同じエラーで止まり、書き込み先にファイルができない |
| REQ-config-017（sync の --max-entries） | sync_max_entries_outside_its_range_stops | 同じ元のテスト。`tests/contract/sync_support.rs` の準備で `execute_sync` に渡し、同じエラーで止まり、書き込み先にファイルができない |
| REQ-config-018（値の読み方） | strict_host_key_checking_values_are_read_ignoring_case | src/config.rs の test_parse_strict_host_key_checking_values。[ssh] の strict_host_key_checking の "ask"・"ASK" が ask、"yes"・"Yes"・"true"・"TRUE" が yes、"no"・"NO"・"false"・"False" が no として読まれる |
| REQ-config-018（知らない値） | unknown_strict_host_key_checking_value_is_read_as_ask | 同じ元のテスト。"maybe"・"on"・空の文字列が ask として読まれる。警告の文言は実行ファイルのテストで確かめる（下の節） |

- REQ-config-016 の設定は `AppConfig` を組み立てず、`load_config_from_paths` で読んだものを使い、[defaults] の読み込みからサーバの値の選び方までの全体を通す。サーバと [defaults] の値は既定値とも一般的な umask の結果とも違う値にした。
- この作業環境の umask は 0002 で、umask だけで作られるファイルとディレクトリの mode が既定値の 0o664 と 0o775 に一致する。そのため「どちらにもないとき」の場合だけでは、既定値で chmod したかどうかを区別できない（umask が 0022 の環境では区別できる）。サーバと [defaults] の場合は、どちらの umask でも chmod がなければ期待する mode にならない。
