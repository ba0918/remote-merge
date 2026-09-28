# 設定のフィルターのテスト整理の記録

設定のフィルターの取り込み（[決定記録](../decision/records/2026-09-29-adopt-config-filters.md)）で加えた要件（REQ-config-021 から REQ-config-026）の根拠テストを整えた過程の記録。
各要件の根拠にしたテスト、変異テストの結果、見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テスト 52 件のうち 1 件（src/config.rs の test_include_default_empty）が実装の中身をなぞるだけのものだったが、元の単体テストは消さない方針のため、この整理ではテストを消していない。
テストを削除しない整理のため、整理前の変異テストは回していない（REQ-testing-012、[決定記録 A2](../decision/records/2026-09-28-mutation-scope.md#A2)）。

## 要件ごとの根拠テスト

手本にした元のテストは消さず、書き換えていない。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた（既存の挙動を確かめるテストのため、失敗する段階はない）。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
FLAG-config-013 から 015 の挙動（全て無効な include、既定の sensitive を外す書き方、"../" を含む exclude のパターンとその警告）は確かめない。存在しない include、root_dir の外を指す include、TUI のツリーの祖先ディレクトリの表示も範囲の外のため確かめない。

### パターンの当て方・include・既定の sensitive（REQ-config-021 から 026）

根拠テストは `tests/contract/config_filters.rs` にある（feature は要らない）。
組み方は `tests/contract/filters.rs` の `fixture` と同じで、一時ディレクトリの設定ファイルを公開の関数 `remote_merge::config::load_config_from_paths` で読み（グローバル設定とプロジェクト設定の合成と include の整え方を通す）、`RuntimeTargets::with_local` でサーバ develop をローカルの一時ディレクトリに差し替え、`remote_merge::cli::status::execute_status` を全件表示（`all: true`）で実行する。
左右には同じ名前で中身の違うファイルを置く。
結果は `remote_merge::service::output::format_json` を通した JSON（`docs/ir/cli/status-output.md` が契約にしている形）の "files" の "path" の集合で確かめ、REQ-config-026 はパスごとの "sensitive" で確かめる。
期待する集合は一覧に出るべきパスの全体と一致すること（`assert_eq!`）で確かめるため、外れるべきパスが出たときも、出るべきパスが出ないときも落ちる。

| 要件 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| REQ-config-021（ファイル名） | name_pattern_excludes_files_whose_name_matches_at_any_depth | src/filter.rs の test_segment_pattern_matches_each_segment・test_segment_glob_pattern。exclude "*.log" で "app.log" と "logs/deep/trace.log" が外れ、同じ階層の "app.txt" と "logs/deep/readme.txt" が残る |
| REQ-config-021（ディレクトリ名） | name_pattern_matching_a_directory_name_excludes_everything_below_it | 同じ元のテスト。exclude "node_modules" で途中の要素に当たる "a/node_modules/b/c.txt" と先頭の要素に当たる "node_modules/top.txt" が外れ、"a/keep.txt" と名前が "node_modules" で始まるだけの "a/node_modules_x/c.txt" が残る |
| REQ-config-022（パス全体） | path_pattern_excludes_only_paths_whose_whole_relative_path_matches | src/filter.rs の test_segment_pattern_does_not_match_full_path・test_path_prefix_pattern。exclude "config/*.toml" で "config/a.toml" が外れ、ファイル名だけが同じで別の場所にある "other/a.toml"・"a.toml"、一つ深い "config/sub/a.toml"、拡張子の違う "config/a.txt" が残る |
| REQ-config-022（"**"） | path_pattern_ending_in_double_star_excludes_everything_below_the_directory | src/filter.rs の test_path_pattern_vendor_legacy・test_double_star_rs_pattern。exclude "vendor/legacy/**" で "vendor/legacy/a.txt" と "vendor/legacy/deep/b.txt" が外れ、"vendor/current/a.txt" と "legacy/a.txt" が残る |
| REQ-config-023（区切りの単位） | include_selects_the_directory_and_below_but_not_a_name_that_only_starts_with_it | src/filter.rs の test_is_path_included_descendant・test_is_path_included_no_false_prefix、src/local/mod.rs の test_resolve_scan_roots_with_existing_dirs・test_include_scans_only_specified_subdirs、tests/contract/filters.rs の include_restricts_status_and_sync_to_selected_files。include "src" で "src/a.txt" と "src/deep/b.txt" だけが出て、"srcx/a.txt" と "top.txt" は出ない |
| REQ-config-023（入れ子） | nested_include_selects_only_below_the_nested_directory | 同じ元のテスト。include "a/b" で "a/b/c.txt" と "a/b/d/e.txt" だけが出て、親の "a/other.txt"、"a/bx/c.txt"、"top.txt" は出ない |
| REQ-config-024（"./" と "/"） | include_with_leading_dot_slash_and_trailing_slash_selects_the_same_as_the_plain_value | src/filter.rs の test_normalize_trailing_slash・test_normalize_leading_dot_slash。"src" の外にもファイルがある構成で、include "./src/" の一覧が include "src" の一覧（"src/a.txt" と "src/deep/b.txt"）と同じになる |
| REQ-config-024（空の値） | empty_include_value_is_ignored | src/filter.rs の test_normalize_empty_strings_removed。include `["", "src"]` で "src" の下だけが出る。空の値は整えられなければ root_dir 全体を指すため、"src" の外の "top.txt" と "other/c.txt" が出ないことで無視を見分ける |
| REQ-config-024（無効な値の無視） | absolute_traversal_and_glob_include_values_are_ignored | src/filter.rs の test_normalize_rejects_absolute_path・test_normalize_rejects_traversal・test_normalize_glob_warning。include に左の root_dir の中の実在するディレクトリ "absolute" を指す絶対パス、"src/../traversal"、名前に glob 文字を含む実在するディレクトリ "lib[1]"、"src" を書き、"src/a.txt" だけが出る。三つの無効な値は整えられなければ root_dir の中の実在するディレクトリとして走査されるため、その下のファイルが出ないことで無視を見分ける。警告の文言は実行ファイルのテストで確かめる（下の節） |
| REQ-config-025 | exclude_removes_matching_paths_from_the_include_target | src/local/mod.rs の test_include_with_exclude_combined。include "src" と exclude "*.log" で "src/a.txt" だけが出て、include の中の "src/a.log"・"src/deep/b.log" と、include の外の "top.txt"・"top.log" は出ない |
| REQ-config-026（既定） | default_sensitive_patterns_apply_without_a_sensitive_setting | src/service/status.rs の test_sensitive_env_file・test_sensitive_nested_path・test_sensitive_wildcard。sensitive を書かない設定で、六つのパターンのそれぞれに名前が当たるファイル（".env"・"app/.env"・".env.production"・"certs/server.pem"・"server.key"・"credentials.json"・"config/credentials.yml"・"my-secret-notes.txt"）が "sensitive": true、当たらないファイル（"README.md"・"env.txt"・"certs/server.crt"・"keys.txt"、後の二つの場合で使う "token.confidential"・"data/token.confidential"）が false になる |
| REQ-config-026（グローバル設定に足す） | sensitive_pattern_in_the_global_config_is_added_to_the_defaults | sensitive の合成を確かめる単体テストはなく、合成の組み方は src/config.rs の test_include_merged_from_global_and_project と tests/contract/config_merging.rs の `load` を手本にした。グローバル設定だけに sensitive "*.confidential" を書くと、"token.confidential" と "data/token.confidential" も true になり、既定のパターンに当たるファイルは true のまま、他は false のまま |
| REQ-config-026（プロジェクト設定に足す） | sensitive_pattern_in_the_project_config_is_added_to_the_defaults | 同じ。プロジェクト設定（`load_config_from_paths` の第 2 引数）だけに同じパターンを書いて、同じ結果になる。グローバル設定とプロジェクト設定では合成の経路が別のため、両方で確かめる |

- どちらの設定にも同じ [local] を書く。[local] がないときの挙動は FLAG-config-002 の範囲のため避けた。
- 計画は設定に書く sensitive のパターンの例に "*.secretish" を挙げていたが、"secretish" は "secret" を含み既定の "*secret*" に当たるため、例として使えなかった（最初の実行で "token.secretish" が sensitive を書かなくても true になって落ちた）。計画の条件（既定のパターンにも "*secret*" にも当たらない名前）に合わせて "*.confidential" にした。計画からの逸れとして記録する。
- 絶対パスの include は左の root_dir の中を指す。右（develop）の root_dir は別の一時ディレクトリのため、整えられなかったときも右では root_dir の外として捨てられるが、左で走査されて一覧に出るため見分けられる。
