# 設定の読み込みと合成のテスト整理の記録

設定の読み込みと合成の取り込み（[決定記録](../decision/records/2026-09-28-adopt-config-loading.md)）で加えた要件（REQ-config-005 から REQ-config-013）の根拠テストを整えた過程の記録。
各要件の根拠にしたテスト、変異テストの結果、見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テスト 47 件のうち 8 件が実装の中身をなぞるだけのものだったが、元の単体テストは消さない方針のため、この整理ではテストを消していない。
テストを削除しない整理のため、整理前の変異テストは回していない（REQ-testing-012、[決定記録 A2](../decision/records/2026-09-28-mutation-scope.md#A2)）。
src/config.rs はこれまで変異テストを回していないため、見逃しは比べる相手なしに新しい見逃しとして決着させる。

## 要件ごとの根拠テスト

手本にした元のテストは消さず、書き換えていない。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
FLAG-config-001 から 008 の挙動（グローバル設定の場所のディレクトリ、[local] がないときと root_dir のない [local]、max_scan_entries・badge_scan_max_files と [scan]、相対パスの [local] の root_dir、root_dir が存在しないとき、知らないキー、init・logs・events の --config の警告）は確かめない。
テストの設定には常に root_dir のある [local] を書き、その root_dir は "~/" で始まるもの以外は絶対パスにする。
値の検査とフィルターの一致の規則は後の回で扱うため、テストの設定には正しい値だけを書く。

### 設定ファイルの場所・--config・読み込みのエラー・ホームの展開（REQ-config-005 から 010）

根拠テストは `tests/contract/config_loading_cli.rs` にある（`test-utils` の feature が要る。SSH の試験サーバを使うため）。
どの設定を読むかはカレントディレクトリと環境変数で決まり、テストのプロセスでそれらを変えると並列に走る他のテストと干渉するため、実行ファイルを起動して確かめる。

準備は同じファイルの `Workspace` を使う。

- `tests/common/mod.rs` の `TestDirs::new_2way` で一時ディレクトリと SSH の試験サーバ（`ssh_server::TestServer::filesystem_without_agent`）を用意し、設定は `gen_config` で作って [local] の root_dir だけをテストごとに選ぶ。
- 実行ファイルは `env_clear` したうえで `HOME` を一時ディレクトリの "home" に、`XDG_CONFIG_HOME` を "home/.config" に、`XDG_DATA_HOME` を一時ディレクトリの "xdg-data" に向け、カレントディレクトリをテストごとに選んで起動する。--config はテストが渡すときだけ付ける。グローバル設定の場所は Linux で "HOME/.config/remote-merge/config.toml" になる。`CliEnv::cmd` と同じ考え方の隔離だが、`CliEnv::cmd` は常に --config を付け、`XDG_CONFIG_HOME` を HOME の外に向けている点が違う。
- 起動の前に、テストが書いた全ての設定ファイル（カレントディレクトリ、--config の指定先、グローバル設定）に `TestDirs::assert_isolated_config` の確認をかける。この確認は `TestDirs` の設定ファイルだけを見る非公開の関数のため、確認の対象の設定ファイルと [local] の root_dir を一時的に差し替えて呼ぶ補助 `TestDirs::assert_isolated_config_at` と、試験サーバのポートを返す `TestDirs::server_port` を `tests/common/mod.rs` に足した。確認そのものは書き直していない。
- 例外として、REQ-config-009 の TOML として読めない設定ファイルにはこの確認をかけられない（確認が設定を TOML として読むため）。代わりに、書く前にその内容が `toml` クレートで読めないことを確かめる。製品も同じクレートで設定を読むため、このファイルから接続先を得ることはない。計画は全ての設定ファイルに確認をかけるとしていたため、計画からの逸れとして記録する。
- 起動するのは全て `status --left local --right develop` で、どの設定が読まれたかはその設定の [local] の root_dir に置いたファイルが status の JSON の "files" の "path" に出るかで見る（"files" は `docs/ir/cli/status-output.md` の要件が契約にしている）。ファイルはローカルの側にだけ置くため "left_only" になり、終了コードは 1 になる。
- エラーの行は --format を付けずに起動し、終了コード 2 と、標準出力と標準エラーをつないだものに文言が含まれることで確かめる（IR はエラーの出力先を契約にしていない）。パスはカレントディレクトリを正規化して比べる。

HOME と XDG_CONFIG_HOME を一時ディレクトリに向けると利用者のマシンの設定を読まないことは、どちらの設定も置かない REQ-config-008 のテストが "Config file not found." で止まることで確かめられる。

| 要件 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| REQ-config-005 | project_config_in_the_current_directory_is_read | src/config.rs の test_load_config_with_project_override_uses_specified_file の組み方と tests/contract/backup_rollback_cli_e2e.rs の実行ファイルの起動の組み方を写した。グローバル設定を置かず、カレントディレクトリの ".remote-merge.toml" の [local] の root_dir に置いたファイルだけが "files" に出る |
| REQ-config-005 | project_config_in_a_parent_directory_is_not_searched | 手本の単体テストはない。親のディレクトリにだけ ".remote-merge.toml" を置き、その子のディレクトリをカレントディレクトリにして、グローバル設定もないとき、終了コード 2 で "Config file not found." を含む |
| REQ-config-006 | config_option_is_read_relative_to_the_current_directory_instead_of_the_project_config | src/config.rs の test_load_config_with_project_override_uses_specified_file。カレントディレクトリに別の root_dir の ".remote-merge.toml" を置き、"./" を付けない相対パス "configs/chosen.toml" を --config に渡すと、--config の設定の root_dir のファイルだけが出る |
| REQ-config-006 | config_option_still_merges_servers_from_the_global_config | 手本の単体テストはない。グローバル設定だけが develop を持ち、--config の設定は develop を持たない（同じ試験サーバを staging として持つ）構成で、`--right develop` の status が --config の設定の root_dir のファイルを出す。グローバル設定を置くため `#[cfg(target_os = "linux")]` にした（FLAG-config-001） |
| REQ-config-007 | config_option_naming_a_missing_file_stops_with_its_absolute_path | src/config.rs の test_load_config_with_project_override_nonexistent_file、tests/cli_error_handling.rs の test_missing_config_exits_with_code_2。"./" を付けない "missing.toml" を渡し、終了コード 2 で、"Config file not found: " とカレントディレクトリから解決した絶対パスを含む |
| REQ-config-007 | config_option_naming_a_directory_stops_with_its_absolute_path | src/config.rs の test_load_config_with_project_override_directory_rejected。ディレクトリ "configs" を渡し、終了コード 2 で、"Config path is not a regular file: " と絶対パスを含む |
| REQ-config-008 | missing_global_and_project_config_stops_naming_the_project_path | src/config.rs の test_config_not_found。どちらの設定も置かず、終了コード 2 で、"Config file not found." とカレントディレクトリの ".remote-merge.toml" の絶対パスを含む。グローバル設定のパスの形は確かめない（FLAG-config-001 の範囲） |
| REQ-config-009 | unparsable_project_config_stops_with_a_parse_error | tests/cli_error_handling.rs の test_invalid_toml_exits_with_code_2。カレントディレクトリの ".remote-merge.toml" を TOML として読めない内容にし、終了コード 2 で "Failed to parse config file: " を含む |
| REQ-config-009 | unparsable_config_option_file_stops_with_a_parse_error | 同じ元のテスト。--config に TOML として読めないファイルを渡し、同じく止まる |
| REQ-config-010 | local_root_starting_with_tilde_is_resolved_under_home | src/ssh/client.rs の test_expand_tilde_home_dir。[local] の root_dir を "~/project" にし、HOME の下の "project" に置いたファイルが "files" に出る |

- REQ-config-007 に "./" を付けない相対パスを渡すのは、絶対パスを渡すと相対かどうかの判定を変えた変異を落とせないためである。REQ-config-006 のテストはカレントディレクトリと実行ファイルのカレントディレクトリが同じため、相対パスの解決の違いは REQ-config-007 の文言で落とす。
- src/config.rs の `load_config_with_project_override` を呼ぶ既存の単体テストは、グローバル設定の場所を差し替えないため、利用者のマシンのグローバル設定を読む。単体テストの書き換えは計画の範囲外のため、ここに書くだけにする。

### セクションごとの合成・キーごとの合成・既定値（REQ-config-011 から 013）

根拠テストは `tests/contract/config_merging.rs` にある（feature は要らない）。
カレントディレクトリにも環境変数にも依存しないため、tests/contract/config_precedence.rs の `load_pair` と同じく、公開の関数 `remote_merge::config::load_config_from_paths` にグローバル設定とプロジェクト設定のパスを渡し、返った `AppConfig` の値で確かめる。
同じファイルの `load` は、グローバル設定だけ・プロジェクト設定だけ・両方のどれを渡すかを選べる。

実装は、プロジェクト設定だけのときはそれをグローバル設定の側に置き換えて合成するため、「プロジェクト設定だけ」と「グローバル設定だけ」は同じ経路を通り、「両方」はプロジェクト設定の有無で分かれる経路を通る。
さらに [ssh]・[backup]・[agent] は、セクションがないときと、セクションはあるがキーを省いたときとで別の箇所で既定値を決める。
そのため既定値は、セクションなしとセクションはあるがキーを全て省くものの両方を、上の三通りの置き方で確かめる。セクションを置く設定は、両方の場合にプロジェクト設定だけ・グローバル設定だけ・両方の三通りにした。

| 要件 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| REQ-config-011（[ssh]） | project_ssh_section_replaces_the_global_one_and_omitted_keys_use_defaults | src/config.rs の test_ssh_config_strict_host_key_checking_default_when_omitted。グローバル設定の [ssh] の全てのキーを既定値と違う値にし、プロジェクト設定の [ssh] で一方のキーだけを書くと、書いたキーはプロジェクトの値、省いたキーは既定値（300 と "ask"）になる。二つのキーのそれぞれを省く二通り |
| REQ-config-011（[backup]） | project_backup_section_replaces_the_global_one_and_omitted_keys_use_defaults | 手本の単体テストはない。同じ構成で [backup] の一方のキーだけを書くと、省いたキーは既定値（true と 7）になる。二通り |
| REQ-config-011（[agent]） | project_agent_section_replaces_the_global_one_and_omitted_keys_use_defaults | src/config.rs の test_agent_config_project_overrides_global。プロジェクト設定の [agent] に deploy_dir だけを書くと、他の四つのキーは既定値になる |
| REQ-config-011（セクションがない） | global_sections_are_used_when_the_project_config_lacks_them | 同じ元のテスト。プロジェクト設定に [ssh]・[backup]・[agent] がないとき（プロジェクト設定を置くがセクションがない場合と、プロジェクト設定を置かない場合）、三つのセクションの全てのキーがグローバル設定の値になる |
| REQ-config-012 | defaults_key_in_the_project_config_wins_over_the_global_one | src/config.rs の test_defaults_merge_field_level_via_config_load。両方の [defaults] に二つのキーを書くと、どちらもプロジェクトの値になる |
| REQ-config-012 | defaults_key_missing_from_the_project_config_comes_from_the_global_one | 同じ元のテストと test_defaults_merge_field_level_file_only_override。グローバル設定に二つのキーを書き、プロジェクト設定に一方だけを書くと、書いていないキーはグローバルの値になる。どちらのキーを省くかの二通り |
| REQ-config-012 | defaults_key_in_neither_config_uses_the_default_value | 手本の単体テストはない。両方の [defaults] に同じ一方のキーだけを書くと、他方のキーは既定値（0o664 か 0o775）になる。二通り |
| REQ-config-013（[servers.名前] の 3 行） | omitted_server_keys_use_the_default_port_auth_and_sudo | src/config.rs の test_minimal_config。host・user・root_dir だけのサーバを、グローバル設定だけ・プロジェクト設定だけ・両方（プロジェクト側に書く）の三通りに置き、port が 22、auth が "key"、sudo が false になる |
| REQ-config-013（残る 11 行、セクションなし） | absent_sections_use_the_default_values | src/config.rs の test_agent_config_defaults_when_absent、test_defaults_absent_uses_hardcoded_fallback。[ssh]・[backup]・[agent]・[defaults] のない設定を三通りに置き、11 行の全てが既定値になる |
| REQ-config-013（残る 11 行、キーを全て省く） | sections_with_every_key_omitted_use_the_default_values | src/config.rs の test_ssh_config_strict_host_key_checking_default_when_omitted。キーを一つも書かない四つのセクションを、グローバル設定だけ、プロジェクト設定だけ、両方の設定でプロジェクト側だけ・グローバル側だけ・両方に置く五通りで、11 行の全てが既定値になる |
