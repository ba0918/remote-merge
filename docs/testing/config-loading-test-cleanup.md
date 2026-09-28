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
- 例外として、TOML として読めない設定ファイル（REQ-config-009 のテストと、REQ-config-006 の config_option_is_read_relative_to_the_current_directory_instead_of_the_project_config のカレントディレクトリの設定）にはこの確認をかけられない（確認が設定を TOML として読むため）。代わりに、書く前にその内容が `toml` クレートで読めないことを確かめる。製品も同じクレートで設定を読むため、このファイルから接続先を得ることはない。計画は全ての設定ファイルに確認をかけるとしていたため、計画からの逸れとして記録する。
- 起動するのは全て `status --left local --right develop` で、どの設定が読まれたかはその設定の [local] の root_dir に置いたファイルが status の JSON の "files" の "path" に出るかで見る（"files" は `docs/ir/cli/status-output.md` の要件が契約にしている）。ファイルはローカルの側にだけ置くため "left_only" になり、終了コードは 1 になる。
- エラーの行は --format を付けずに起動し、終了コード 2 と、標準出力と標準エラーをつないだものに文言が含まれることで確かめる（IR はエラーの出力先を契約にしていない）。パスはカレントディレクトリを正規化して比べる。

HOME と XDG_CONFIG_HOME を一時ディレクトリに向けると利用者のマシンの設定を読まないことは、どちらの設定も置かない REQ-config-008 のテストが "Config file not found." で止まることで確かめられる。

| 要件 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| REQ-config-005 | project_config_in_the_current_directory_is_read | src/config.rs の test_load_config_with_project_override_uses_specified_file の組み方と tests/contract/backup_rollback_cli_e2e.rs の実行ファイルの起動の組み方を写した。グローバル設定を置かず、カレントディレクトリの ".remote-merge.toml" の [local] の root_dir に置いたファイルだけが "files" に出る |
| REQ-config-005 | project_config_in_a_parent_directory_is_not_searched | 手本の単体テストはない。親のディレクトリにだけ ".remote-merge.toml" を置き、その子のディレクトリをカレントディレクトリにして、グローバル設定もないとき、終了コード 2 で "Config file not found." を含む |
| REQ-config-006 | config_option_is_read_relative_to_the_current_directory_instead_of_the_project_config | src/config.rs の test_load_config_with_project_override_uses_specified_file。カレントディレクトリに TOML として読めない ".remote-merge.toml" を置き、"./" を付けない相対パス "configs/chosen.toml" を --config に渡すと、止まらずに --config の設定の root_dir のファイルだけが出る。カレントディレクトリの設定を読めば止まるため、使わないだけでなく下の層として合成もしないことまで確かめる |
| REQ-config-006 | config_option_still_merges_servers_from_the_global_config | 手本の単体テストはない。グローバル設定だけが develop を持ち、--config の設定は develop を持たない（同じ試験サーバを staging として持つ）構成で、`--right develop` の status が --config の設定の root_dir のファイルを出す。グローバル設定を置くため `#[cfg(target_os = "linux")]` にした（FLAG-config-001） |
| REQ-config-007 | config_option_naming_a_missing_file_stops_with_its_absolute_path | src/config.rs の test_load_config_with_project_override_nonexistent_file、tests/cli_error_handling.rs の test_missing_config_exits_with_code_2。"./" を付けない "missing.toml" を渡し、終了コード 2 で、"Config file not found: " とカレントディレクトリから解決した絶対パスを含む |
| REQ-config-007 | config_option_naming_a_directory_stops_with_its_absolute_path | src/config.rs の test_load_config_with_project_override_directory_rejected。ディレクトリ "configs" を渡し、終了コード 2 で、"Config path is not a regular file: " と絶対パスを含む |
| REQ-config-008 | missing_global_and_project_config_stops_naming_the_project_path | src/config.rs の test_config_not_found。どちらの設定も置かず、終了コード 2 で、"Config file not found." とカレントディレクトリの ".remote-merge.toml" の絶対パスを含む。グローバル設定のパスの形は確かめない（FLAG-config-001 の範囲） |
| REQ-config-009 | unparsable_project_config_stops_with_a_parse_error | tests/cli_error_handling.rs の test_invalid_toml_exits_with_code_2。カレントディレクトリの ".remote-merge.toml" を TOML として読めない内容にし、終了コード 2 で "Failed to parse config file: " を含み、同じ行でその後に空でない理由が続く（理由の文言は toml クレートのもので契約ではないため固定しない） |
| REQ-config-009 | unparsable_config_option_file_stops_with_a_parse_error | 同じ元のテスト。--config に TOML として読めないファイルを渡し、同じく止まる |
| REQ-config-009 | unparsable_global_config_stops_with_a_parse_error_even_with_a_valid_project_config | 手本の単体テストはない。グローバル設定を TOML として読めない内容にし、カレントディレクトリに有効な ".remote-merge.toml" を置いても、同じく止まる（グローバル設定はプロジェクト設定と別の呼び出しで読まれる）。グローバル設定を置くため `#[cfg(target_os = "linux")]` にした（FLAG-config-001） |
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
| REQ-config-012 | global_defaults_keys_are_used_when_the_project_config_lacks_the_section | src/config.rs の test_defaults_section_in_config（グローバル設定だけの場合）。グローバル設定の [defaults] に既定値と違う二つのキーを書き、プロジェクト設定に [defaults] のセクション自体がない場合と、グローバル設定だけを置く場合の二通りで、二つのキーがグローバルの値になる。両方に [defaults] がある場合と実装の別の経路を通る |
| REQ-config-012 | defaults_key_in_neither_config_uses_the_default_value | 手本の単体テストはない。両方の [defaults] に同じ一方のキーだけを書くと、他方のキーは既定値（0o664 か 0o775）になる。二通り |
| REQ-config-013（[servers.名前] の 3 行） | omitted_server_keys_use_the_default_port_auth_and_sudo | src/config.rs の test_minimal_config。host・user・root_dir だけのサーバを、グローバル設定だけ・プロジェクト設定だけ・両方（プロジェクト側に書く）の三通りに置き、port が 22、auth が "key"、sudo が false になる |
| REQ-config-013（残る 11 行、セクションなし） | absent_sections_use_the_default_values | src/config.rs の test_agent_config_defaults_when_absent、test_defaults_absent_uses_hardcoded_fallback。[ssh]・[backup]・[agent]・[defaults] のない設定を三通りに置き、11 行の全てが既定値になる |
| REQ-config-013（残る 11 行、キーを全て省く） | sections_with_every_key_omitted_use_the_default_values | src/config.rs の test_ssh_config_strict_host_key_checking_default_when_omitted。キーを一つも書かない四つのセクションを、グローバル設定だけ、プロジェクト設定だけ、両方の設定でプロジェクト側だけ・グローバル側だけ・両方に置く五通りで、11 行の全てが既定値になる |

## 整理後の変異テスト

変異テストは決着の対象の関数に絞って一度実行した。
記録の最初のコミット c98fceb（テストの最後のコミットは f40f05e）で、作業ツリーに変更のない状態で実行し、実行中は作業ツリーに触れていない。
その後、レビューを受けて根拠テストを強め、足した（af87288 から 380a820）が、変異テストは回し直していない。
コマンドは次のとおりで、並列数は既定の 2 である。

```sh
scripts/mutants.sh --re '(global_config_path|project_config_path|load_config_with_project_override|load_config_from_paths|load_raw_config|merge_configs|convert_agent_config|convert_raw_ssh_config|merge_raw_defaults|convert_defaults_config|convert_server_config|expand_tilde)' src/config.rs
```

全体の集計は `mutants: caught=17 survived=4 timeout=0 unviable=6 equivalent=0`（27 件、実行時間は約 7 分）。
スクリプトの終了コードは 1（kotowari mutants が 4 件の見逃しを error として報告したため。メモリ上限での停止ではない）。
27 件は全て正規表現に名前の一致する関数の変異で、構造体のフィールドを消す変異は一件も出なかった（他の関数のものも混ざらなかった）。

関数ごとの内訳（`outcomes.json` から数えた）は次のとおり。

| 関数 | caught | survived | unviable |
|---|---|---|---|
| global_config_path | 2 | 0 | 0 |
| project_config_path | 1 | 0 | 0 |
| load_config_with_project_override | 2 | 0 | 1 |
| load_config_from_paths | 1 | 0 | 1 |
| load_raw_config | 0 | 0 | 1 |
| merge_configs | 5 | 2 | 1 |
| convert_agent_config | 1 | 0 | 0 |
| convert_raw_ssh_config | 1 | 0 | 0 |
| merge_raw_defaults | 1 | 0 | 1 |
| convert_defaults_config | 1 | 0 | 0 |
| convert_server_config | 1 | 2 | 1 |
| expand_tilde | 1 | 0 | 0 |

unviable の 6 件は、`Result<AppConfig>`・`Result<RawConfig>`・`Result<ServerConfig>` を `Ok(Default::default())` にする変異と `merge_raw_defaults` を `Some(Default::default())` にする変異で、これらの型が `Default` を実装しないため組み立てられない。

### 検知したテスト

検知した 17 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗から少し進んで止まるため、集めた名前は検知したテストの全てではなく、先に失敗したものである。
負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）だけで検知された変異はなかった。

- この回で足したテストが先に失敗したもの: `global_config_path` の 2 件（config_option_still_merges_servers_from_the_global_config）、`project_config_path` の 1 件（project_config_in_the_current_directory_is_read、local_root_starting_with_tilde_is_resolved_under_home）。
- 他の変異は src/config.rs の既存の単体テストか、既存の契約テスト（config_precedence の filter_patterns_from_both_levels_are_combined、merge_hunks の hunks_on_a_configured_sensitive_file_without_force_stops_without_writing）が先に失敗した。この回で足したテストが同じ変異で落ちるかは、nextest が先に止まったため、このログからは分からない。

### 見逃しと決着

計画の区別により、`merge_configs` のフィルターの重複除き・バックアップ領域の除外・include の正規化と、`convert_server_config` の値の検査（port が 0、auth の不明な値、root_dir が空、パーミッション、password の警告）の変異は、値の検査とフィルターの回で要件になる振る舞いのため、この回では要件に基づくテストを足さない。
`merge_configs` の max_scan_entries と badge_scan_max_files の変異（FLAG-config-003）と、[local] がないときのエラーの変異（FLAG-config-002）は、この回の結果に見逃しとして出なかった。

| 位置 | 変異 | 行の中身 | 決着の対象 | 決着 |
|---|---|---|---|---|
| src/config.rs:585:24 | delete ! in merge_configs | プロジェクト設定の [filter] の sensitive を足すときの重複除き（`if !filter.sensitive.contains(&s)`） | 対象でない（フィルター） | 後の回に回す候補（下の注 1） |
| src/config.rs:607:8 | delete ! in merge_configs | include が空でないときだけ正規化する分岐（`if !filter.include.is_empty()`） | 対象でない（フィルター） | 後の回に回す候補（下の注 2） |
| src/config.rs:886:13 | replace == with != in convert_server_config | auth が "key" で password があるときの警告（`if auth == AuthMethod::Key && password.is_some()`） | 対象でない（値の検査） | 後の回に回す候補（下の注 3） |
| src/config.rs:886:32 | replace && with \|\| in convert_server_config | 同じ行 | 対象でない（値の検査） | 同上 |

決着の対象の見逃しはない。見逃しや新しいテストが不具合の疑いを示したものもない。

1. 585:24 は、変異で同じパターンが sensitive に二度入るか、新しいパターンが入らなくなる。後者は config_precedence の filter_patterns_from_both_levels_are_combined の "*.pem" で落ちそうに見えるが、"*.pem" は既定の sensitive に含まれるため、変異の下でも一覧に残り落ちない。既定にない新しいパターンをプロジェクト設定の sensitive にだけ書く構成なら落とせる見込みだが、確かめていない。フィルターの回で要件（REQ-config-002 の和集合の範囲か、フィルターの要件）に基づくテストにするか判断する。
2. 607:8 は、変異で include が空のときだけ正規化を呼び、空でないときは呼ばなくなる。正規化はパストラバーサル・絶対パス・glob の include を拒否するため、それらを include に書いた構成で落とせる見込みだが、確かめていない。フィルターの回の候補。
3. 886 の二件は、変わるのが tracing の警告を出すかどうかだけで、設定の値・標準出力・終了コードは変わらない。警告はログにだけ出るため、値の検査の回でこの警告を要件にするか（要件にしないなら同等変異として登録するか、製品コードから除くか）を判断する。

## 要件の verification の見直し

REQ-config-005 から 013 の verification は全て unit で、いずれも具体的な設定ファイルの置き方と中身で結果が決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-config-005 | unit | カレントディレクトリとその親に置いた設定の組み合わせごとに、読まれる設定が決まる |
| REQ-config-006 | unit | --config の相対パスとカレントディレクトリの設定・グローバル設定の組み合わせごとに、読まれる設定が決まる |
| REQ-config-007 | unit | 存在しないパスとディレクトリの二つの場面で、エラーの文言と終了コードが決まる |
| REQ-config-008 | unit | どちらの設定もない一つの場面で、エラーの文言と終了コードが決まる |
| REQ-config-009 | unit | TOML として読めない設定の場面で、エラーの文言と終了コードが決まる |
| REQ-config-010 | unit | "~/" で始まる root_dir と HOME の組み合わせで、使うディレクトリが決まる |
| REQ-config-011 | unit | セクションの有無と省いたキーの組み合わせごとに、合成した値が決まる |
| REQ-config-012 | unit | 二つのキーのそれぞれがどちらの設定にあるかの組み合わせごとに、値が決まる |
| REQ-config-013 | unit | TBL-config-001 の有限の行と、セクションの置き方の有限の組み合わせで、値が決まる |

## 利用者の判断

2026-09-28 に利用者が次のとおり判断した。

1. 変異テストの見逃し 4 件は後の回に回す。src/config.rs:585:24 と 607:8 はフィルターの回、886:13 と 886:32 は値の検査の回で扱う。この計画ではテストを足さず、同等変異としても登録しない。この判断により変異テストは回し直していない。
2. REQ-config-009 の TOML として読めない設定ファイルに、隔離の確認の代わりに、書く前に toml クレートで読めないことを確かめるやり方を認める。これは隔離の確認を新しく書くことには当たらないものとする。

新しい FLAG の候補と verification の見直しの候補はない。
