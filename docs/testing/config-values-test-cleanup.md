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

### 警告・パスワード・鍵のパス（REQ-config-018 から 020）

根拠テストは `tests/contract/config_values_cli.rs` にある（`test-utils` の feature が要る。SSH の試験サーバを使うため）。
strict_host_key_checking の警告は標準エラーに出て関数呼び出しでは観測しにくく、パスワードと鍵のパスは SSH の接続で初めて使われるため、実行ファイルを起動して確かめる。

準備は同じファイルの `Workspace` を使う。前の回の `tests/contract/config_loading_cli.rs` の `Workspace` と同じ形だが、そちらの起動は既存の隔離の確認（`TestDirs::assert_isolated_config_at`）を通すため共有せず、同じ形の補助を新しいモジュールに置いた。

- `tests/common/mod.rs` の `TestDirs::new_2way` で一時ディレクトリと SSH の試験サーバを用意し、ローカルの側にだけファイル "only-local.txt" を置く。設定は `gen_config` の本文の一部を置き換えて作る。
- 実行ファイルは `env_clear` したうえで `HOME` を一時ディレクトリの "home" に、`XDG_CONFIG_HOME` を "home/.config" に、`XDG_DATA_HOME` を一時ディレクトリの "xdg-data" に向け、`PATH` とテストが明示的に渡す環境変数だけを設定し、標準入力を `Stdio::null()` にして `Command::output()` で `status --left local --right develop` を起動する。SSH エージェントの変数も渡らない。
- これらの設定は既存の確認を通らない（知らない strict_host_key_checking、正しくない password、auth が "key"）。そこで既存の確認を変えずに、別の確認 `TestDirs::assert_isolated_values_config` を `tests/common/mod.rs` に足し、起動の前に必ずかける。確かめるのは、全てのサーバの host が "127.0.0.1"、port がテスト自身の試験サーバのもので 22 でない、user が "fixture-user"、サーバの root_dir と [local] の root_dir が一時ディレクトリの下、[agent] の enabled が false、strict_host_key_checking が "no"（REQ-config-018 のテストだけは求めない）、auth が "password" か "key" で、"key" のときは起動に使う HOME で解決した鍵のパス（key を省いたら "HOME/.ssh/id_rsa"、"~/" で始まるならその HOME での展開、それ以外はそのパス）が一時ディレクトリの下、であること。password の値は問わない。一時ディレクトリの下かどうかは、絶対パスで ".." を含まず一時ディレクトリで始まることで見る。

| 要件 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| REQ-config-018（警告） | unknown_strict_host_key_checking_value_warns_and_falls_back_to_ask | src/config.rs の test_parse_strict_host_key_checking_values。strict_host_key_checking を "maybe" にした設定で起動し、標準出力と標準エラーをつないだもの（ANSI のエスケープを除く）に "Unknown strict_host_key_checking value: 'maybe', falling back to 'ask'" が含まれる。終了コードとその後の接続の結果は確かめない（ask のもとでの未知のホスト鍵の扱いは REQ-ssh-001 の範囲） |
| REQ-config-018（ask は警告しない） | ask_value_in_any_case_does_not_warn | 変異テストの見逃し（src/config.rs:761:9）を受けて足した。同じ準備で "maybe" には警告が出ることと並べて、"ask" と "ASK" では出力に "Unknown strict_host_key_checking value" が含まれない |
| REQ-config-019（(a) と (d)） | password_from_the_uppercase_server_env_var_wins_over_the_config_password | src/ssh/client.rs の test_resolve_password_*。設定の password が正しくない "wrong-password" のとき、REMOTE_MERGE_PASSWORD_DEVELOP が "fixture-password" なら接続でき、環境変数の名前だけを REMOTE_MERGE_PASSWORD_develop に変えると接続できない |
| REQ-config-019（(c) と (b)） | empty_server_env_var_is_treated_as_unset | 同じ元のテスト。設定の password が正しい "fixture-password" のとき、REMOTE_MERGE_PASSWORD_DEVELOP が空なら接続でき、同じ環境変数の値だけを "wrong-password" に変えると接続できない |
| REQ-config-020（key を省く） | omitted_key_uses_the_default_path_and_names_it_when_it_cannot_be_read | 手本の単体テストはない（鍵のパスを扱う単体テストは src/ssh/client.rs の test_expand_tilde_home_dir）。auth を "key" にし password の行と key を省いた設定で起動し、実行が成功で終わらず、出力に "Failed to load SSH private key: ~/.ssh/id_rsa" が含まれる。一時ディレクトリの HOME に鍵ファイルは置かない |
| REQ-config-020（"~/" の展開） | key_starting_with_tilde_is_expanded_under_home_and_named_when_it_cannot_be_read | 同じ。key を "~/keys/missing" にした設定で起動し、実行が成功で終わらず、出力に "Failed to load SSH private key: " と一時ディレクトリの HOME の下の "keys/missing" の絶対パスが含まれる |

- REQ-config-019 は --format json で起動し、接続できたことは JSON の "files" に "only-local.txt" が出ることで、接続できなかったことは、出力が JSON として解釈できること（REQ-cli-018 の、失敗しても JSON で返す契約）と、"files" に "only-local.txt" を含む項目がないこと（"files" がない場合も含む）で確かめる。失敗時の JSON のキー（"error" など）は IR が契約にしていないため固定しない。終了コードと認証のエラーの文言は IR が契約にしていないため確かめない。
- 計画は否定側の組を「(b) は (a) と、(d) は (c) と」としていたが、その目的は「環境変数だけが違う組にして、否定がパスワードと関係のない理由で成り立たないようにする」ことである。文字どおりの組では設定の password も違ってしまうため、目的に合わせて、設定の password が同じで環境変数だけが違う (a) と (d)、(c) と (b) を組にした。計画からの逸れとして記録する。
- 否定側の失敗の理由は、テストを書くときに一度だけ JSON を表示して確かめた（コミットには含めていない）。(b) と (d) のどちらも "SSH authentication failed (user: fixture-user@127.0.0.1)" で、パスワード認証の失敗だった。
- REQ-config-020 の二つの起動は、どちらも終了コード 2 で "Error: Failed to load SSH private key: パス" を標準エラーに出した（同じく一度だけ表示して確かめた）。HOME を一時ディレクトリに向けると、"~/" は利用者のホームではなくその HOME に展開される。テストは「エラーで止まる」ことを実行が成功で終わらないことで確かめ、終了コードの値（2）は IR が定めていないため固定しない。
- REQ-config-019 の (c) と (d) では設定の password を使うため平文の警告（FLAG-config-011）が出るが、その有無は確かめない。パスワードが環境変数にも設定にもない組み合わせは FLAG-config-011 に当たるため使わない。

## 整理後の変異テスト

変異テストは決着の対象の関数に絞って実行した。並列数は既定の 2 である。
計画の Approach and why の形（`authenticate\b`。`authenticate_or_disconnect` を含めない）で実行した。計画の S3 の Shown by の行は `\b` のない形だが、Approach and why が理由とともに `\b` の形を定めているため、そちらに合わせた。

```sh
scripts/mutants.sh --re '(parse_permissions|resolve_file_permissions|resolve_dir_permissions|validate_max_scan_entries|validate_badge_scan_max_files|resolve_max_entries|parse_strict_host_key_checking|convert_server_config|convert_defaults_config|resolve_password|authenticate\b)' src/config.rs src/ssh/client.rs
```

一度目はコミット b6cfe81（S2 までのテストと記録）で実行し、`mutants: caught=36 survived=6 timeout=0 unviable=9 equivalent=0`（51 件、約 13 分）だった。
見逃しのうち src/config.rs:761:9（delete match arm "ask" in parse_strict_host_key_checking）は決着の対象だったため、テストを足して（下の「見逃しと決着」）、そのコミット e01e78d で作業ツリーに変更のない状態で回し直した。
二度目の結果は `mutants: caught=37 survived=5 timeout=0 unviable=9 equivalent=0`（51 件、約 14 分）だった。
一度目と二度目で結果が変わったのは 761:9 が survived から caught になった一件だけである。
その後、差分のレビューを受けて REQ-config-020 のテストに実行が成功で終わらないことの確認を足し（d1e294f）、REQ-config-019 の否定側の判定を失敗時の JSON のキーに頼らない形に変えた（8c5ad49）。
テストの判定が変わったため、コミット 8c5ad49 で作業ツリーに変更のない状態で三度目を回した。
最後の結果はこの三度目のもので、`mutants: caught=38 survived=4 timeout=0 unviable=9 equivalent=0`（51 件、約 14 分）である。
スクリプトの終了コードは三度とも 1（kotowari mutants が見逃しを error として報告したため。メモリ上限での停止ではない）。

二度目（e01e78d）と三度目（8c5ad49）で結果が変わったのは次の三件で、他の 48 件は同じ結果だった。

| 位置 | 変異 | e01e78d | 8c5ad49 |
|---|---|---|---|
| src/config.rs:886:13 | replace == with != in convert_server_config | survived | caught（tests/tui_merge.rs の test_hunk_merge_left_to_right_with_l だけが失敗） |
| src/ssh/client.rs:215:13 | delete field keepalive_max from struct client::Config expression in SshClient::build_client_config | survived | caught（同じく test_hunk_merge_left_to_right_with_l だけが失敗） |
| src/ssh/client.rs:379:27 | replace == with != in SshClient::authenticate | caught（tests/tui_merge.rs の二つのテストだけが失敗） | survived |

三件とも、検知したかどうかが tui_merge のテストの失敗だけで決まっている。
886:13 は tracing の警告を出すかどうか、215:13 は keepalive の最大回数だけを変える変異で、TUI のテストはどちらも観測しない。
そのため、三件の違いは変異によるものではなく、待ち時間に頼るテストが負荷の下で落ちたかどうかの違いと推測する（変異の下でそのテストを単独で回し直してはいない）。
REQ-config-019 と 020 のテストの判定の変更は、この三件の結果に関わらない。

関数ごとの内訳（8c5ad49 の `outcomes.json` から数えた）は次のとおり。

| 関数 | caught | survived | unviable |
|---|---|---|---|
| validate_badge_scan_max_files | 6 | 0 | 0 |
| validate_max_scan_entries | 6 | 0 | 0 |
| resolve_max_entries | 2 | 0 | 0 |
| parse_permissions | 6 | 0 | 0 |
| parse_permissions_field | 2 | 0 | 0 |
| resolve_file_permissions | 2 | 0 | 0 |
| resolve_dir_permissions | 2 | 0 | 0 |
| parse_strict_host_key_checking | 3 | 0 | 1 |
| convert_defaults_config | 1 | 0 | 0 |
| convert_server_config | 2 | 1 | 1 |
| resolve_password | 2 | 0 | 7 |
| SshClient::authenticate | 3 | 1 | 0 |
| SshClient::build_client_config（正規表現に一致しない。構造体のフィールドを消す変異） | 1 | 2 | 0 |

unviable の 9 件は、`parse_strict_host_key_checking` を `Default::default()` にする変異、`convert_server_config` を `Ok(Default::default())` にする変異、`resolve_password` の返り値を `Some((…, Default::default()))` にする 7 件で、`StrictHostKeyChecking`・`ServerConfig`・`PasswordSource` が `Default` を実装しないため組み立てられない。

### 前の回との比較（convert_server_config と convert_defaults_config）

前の回（[記録](./config-loading-test-cleanup.md)の d962aaf）では `convert_server_config` が caught 1・survived 2・unviable 1、`convert_defaults_config` が caught 1 だった。この回の二度目（e01e78d）も同じ数で、見逃しは同じ src/config.rs:886:13 と 886:32 の二件だった。
三度目（8c5ad49）では 886:13 が caught と数えられ、`convert_server_config` は caught 2・survived 1・unviable 1 になった。ただし上のとおり、886:13 は tui_merge のテストの失敗だけで検知されたもので、実質は前の回と同じ二件の見逃しと推測する。
前の回で値の検査の回に回したこの二件は、この回の取り込みで FLAG-config-010（auth が "key" のサーバの password の警告）になったため、その範囲として記録する（下の表）。

### 検知したテスト

検知した 38 件の変異ごとに（8c5ad49 の実行）、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗から少し進んで止まるため、集めた名前は検知したテストの全てではなく、先に失敗したものである。

- 足したテストが先に失敗したもの: src/config.rs:761:9（delete match arm "ask"）の ask_value_in_any_case_does_not_warn。
- 他の変異は src/config.rs と src/ssh/client.rs の既存の単体テストか、SSH の試験サーバを使う既存の契約テストが先に失敗した。足したテストが同じ変異で落ちるかは、nextest が先に止まったため、このログからは分からない。
- 負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）だけで検知された変異は次の三件である。
  - src/config.rs:886:13（replace == with != in convert_server_config）と src/ssh/client.rs:215:13（delete field keepalive_max in SshClient::build_client_config）: どちらも tests/tui_merge.rs の test_hunk_merge_left_to_right_with_l だけで検知された。上の二度目と三度目の比較のとおり、負荷による失敗と推測し、実質は見逃しとして扱う。886:13 は FLAG-config-010 の範囲、215:13 は下の「利用者の判断」の 1 の範囲である。
  - e01e78d の実行では、src/ssh/client.rs:379:27（replace == with != in SshClient::authenticate、設定の password を使うときの平文の警告の条件 `if source == PasswordSource::Config`）が tests/tui_merge.rs の test_hunk_merge_left_to_right_with_l と test_hunk_merge_right_to_left_with_h_key だけで検知されていた。この変異で変わるのは tracing の警告を出す場合（設定の password のときに出さず、環境変数のときに出す）だけで、TUI のテストは警告を観測しない。そのため、その検知は負荷による失敗と推測していた。8c5ad49 の実行ではこの変異は survived になった（下の表）。
  - src/ssh/client.rs:363:20（delete ! in SshClient::authenticate、鍵認証の結果の判定 `if !auth_res.success()`）: tests/agent_ssh_deploy.rs の agent_ssh_exec_handshake・agent_ssh_list_tree_roundtrip・agent_ssh_ping_pong・agent_ssh_read_files_roundtrip の四つがそろって失敗した。変異は成功した鍵認証を失敗として扱うため、鍵認証で接続するこれらのテストが落ちるのは変異によるものと見られる（負荷による失敗かどうかは確かめていない）。鍵認証の成否は IR に要件がなく、この回の要件の範囲の外である。

### 見逃しと決着

計画の区別により、auth が "key" のサーバの password の警告（FLAG-config-010）、平文のパスワードの警告とパスワードがないとき（FLAG-config-011）、IR に要件のない認証の成否は決着の対象から外し、対象の関数の外の構造体のフィールドを消す変異は記録だけする。それ以外の見逃しは全て決着の対象にした。

| 位置 | 変異 | 行の中身 | 決着の対象 | 決着 |
|---|---|---|---|---|
| src/config.rs:761:9 | delete match arm "ask" in parse_strict_host_key_checking | "ask" を ask として読む分岐 | 対象（REQ-config-018） | テストを足した（e01e78d で caught）。分岐を消しても知らない値の分岐が ask を返すため値は変わらず、警告が出るかどうかだけが変わる。REQ-config-018 は知らない値のときだけ警告を出すとするため区別できる。tests/contract/config_values_cli.rs の ask_value_in_any_case_does_not_warn（`@kotowari[REQ-config-018]`）で、同じ準備で "maybe" に警告が出ることと並べて、"ask" と "ASK" に警告が出ないことを確かめる。変異を一時的に書き入れてこのテストが落ちることを確かめ、`git diff --stat src/` が空に戻ることを確かめた |
| src/config.rs:886:13 | replace == with != in convert_server_config | auth が "key" で password があるときの警告（`if auth == AuthMethod::Key && password.is_some()`） | 対象でない（FLAG-config-010） | 記録だけする。変わるのは tracing の警告を出すかどうかだけで、設定の値・標準出力・終了コードは変わらない。8c5ad49 では tui_merge のテストの失敗だけで caught と数えられた（負荷によるものと推測） |
| src/config.rs:886:32 | replace && with \|\| in convert_server_config | 同じ行 | 対象でない（FLAG-config-010） | 同上 |
| src/ssh/client.rs:213:13 | delete field inactivity_timeout from struct client::Config expression in SshClient::build_client_config | SSH の接続設定の無通信のタイムアウト | 対象でない（正規表現に一致しない関数の、構造体のフィールドを消す変異） | 記録だけする（下の「利用者の判断」の 1） |
| src/ssh/client.rs:214:13 | delete field keepalive_interval from struct client::Config expression in SshClient::build_client_config | keepalive の間隔 | 同上 | 同上 |
| src/ssh/client.rs:215:13 | delete field keepalive_max from struct client::Config expression in SshClient::build_client_config | keepalive の最大回数 | 同上 | 同上。8c5ad49 では tui_merge のテストの失敗だけで caught と数えられた（負荷によるものと推測） |
| src/ssh/client.rs:379:27 | replace == with != in SshClient::authenticate | 設定の password を使うときの平文の警告の条件（`if source == PasswordSource::Config`） | 対象でない（FLAG-config-011） | e01e78d では caught、8c5ad49 では survived。記録済みの「利用者の判断」の 2 はこの変異を実質の見逃しと推測したうえでの判断だが、前の実行で caught だったものが見逃しに変わったため、改めて利用者の判断を待つ（下の「利用者の判断」の 3） |

決着の対象の見逃しは残っていない。見逃しや新しいテストが不具合の疑いを示したものはない。同等変異の登録はしていない。

## 要件の verification の見直し

REQ-config-014 から 020 の verification は全て unit で、いずれも具体的な設定の値や環境変数の組み合わせで結果が決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-config-014 | unit | TBL-config-002 の三行のそれぞれの値で、エラーの文言が決まる |
| REQ-config-015 | unit | 四つのキーと、有限の書き方・三つの理由の組み合わせで、読んだ値かエラーの文言が決まる |
| REQ-config-016 | unit | サーバと [defaults] のどちらに値があるかの三通りで、作られるものの権限が決まる |
| REQ-config-017 | unit | TBL-config-003 の三行と範囲の両端・両外の値で、読んだ値かエラーの文言が決まる |
| REQ-config-018 | unit | 有限の値の一覧と知らない値で、読んだ値と警告の有無が決まる |
| REQ-config-019 | unit | 環境変数の有無・空・名前と設定の password の組み合わせで、使うパスワードが決まる |
| REQ-config-020 | unit | key を省く・"~/" で始まるの二つの場面と HOME で、使う鍵のパスとエラーの文言が決まる |

## 利用者の判断

次の二つは計画の区別では記録だけのものだが、計画の Stop conditions（決着の対象でない見逃しが前の回の記録より増えた）に当たりうるため、利用者の判断を待つ。
新しい FLAG の候補と verification の見直しの候補はない。

1. src/ssh/client.rs:213:13・214:13・215:13 の構造体のフィールドを消す変異 3 件は、前の回に回していない src/ssh/client.rs を今回対象に含めたことで新しく出た、決着の対象でない見逃しである。前の回の記録の決着の対象でない見逃し（src/config.rs の 4 件）と比べると、この回の決着の対象でない見逃しは 5 件（886 の二件とこの三件）で、件数が増えた。IR に SSH の keepalive と無通信のタイムアウトの要件はない。推奨は、記録だけにとどめ、ssh の話題（REQ-ssh-*）を扱う回で要件にするかを判断すること。
2. src/ssh/client.rs:379:27 は caught と数えられたが、上の「検知したテスト」のとおり実質は見逃しと推測され、FLAG-config-011 の範囲である。推奨は、FLAG-config-011 の決着のときに扱うこととし、この回ではテストを足さず、同等変異としても登録しないこと。

2026-09-29 に利用者が次のとおり判断した。

1. src/ssh/client.rs:213:13・214:13・215:13（build_client_config の構造体のフィールドを消す変異）の 3 件は記録だけにとどめ、要件にするかどうかは ssh の話題を扱う回で決める。
2. src/ssh/client.rs:379:27（平文の警告）は、FLAG-config-011 を決着させるときに扱う。この回ではテストを足さず、同等変異としても登録しない。

この判断はテストもコードも変えないため、変異テストは回し直していない。

その後、REQ-config-019 と 020 のテストの判定を変えたため、8c5ad49 で変異テストを回し直した（上の「整理後の変異テスト」）。
886:32・213:13・214:13 の見逃しは記録済みの判断をそのまま当てはめる。886:13 と 215:13 は caught と数えられたが、負荷による検知と推測し、記録済みの判断（FLAG-config-010 の範囲、判断の 1）を当てはめる。
次の一件は判断を待つ。

3. src/ssh/client.rs:379:27（平文の警告）は、e01e78d の実行で caught（tui_merge のテストの失敗だけ）、8c5ad49 の実行で survived だった。前の実行で caught だったものが見逃しに変わったため、決着させずに記録する。推奨は、判断の 2 がこの変異を実質の見逃しとして扱っていたため、その判断をそのまま当てはめることである。
