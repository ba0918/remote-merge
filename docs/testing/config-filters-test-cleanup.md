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
| REQ-config-022（パス全体） | path_pattern_excludes_only_paths_whose_whole_relative_path_matches | src/filter.rs の test_segment_pattern_does_not_match_full_path・test_path_prefix_pattern。exclude "config/*.toml" で "config/a.toml" が外れ、ファイル名だけが同じで別の場所にある "other/a.toml"・"a.toml"、拡張子の違う "config/a.txt" が残る。"*" が "/" をまたがないこと（一つ深い "config/sub/a.toml" が外れないこと）は IR が契約にしていないため確かめない |
| REQ-config-022（"**"） | path_pattern_ending_in_double_star_excludes_everything_below_the_directory | src/filter.rs の test_path_pattern_vendor_legacy・test_double_star_rs_pattern。exclude "vendor/legacy/**" で "vendor/legacy/a.txt" と "vendor/legacy/deep/b.txt" が外れ、"vendor/current/a.txt" と "legacy/a.txt" が残る |
| REQ-config-023（区切りの単位） | include_selects_the_directory_and_below_but_not_a_name_that_only_starts_with_it | src/filter.rs の test_is_path_included_descendant・test_is_path_included_no_false_prefix、src/local/mod.rs の test_resolve_scan_roots_with_existing_dirs・test_include_scans_only_specified_subdirs、tests/contract/filters.rs の include_restricts_status_and_sync_to_selected_files。include "src" で "src/a.txt" と "src/deep/b.txt" だけが出て、"srcx/a.txt" と "top.txt" は出ない |
| REQ-config-023（入れ子） | nested_include_selects_only_below_the_nested_directory | 同じ元のテスト。include "a/b" で "a/b/c.txt" と "a/b/d/e.txt" だけが出て、親の "a/other.txt"、"a/bx/c.txt"、"top.txt" は出ない |
| REQ-config-024（空の値） | empty_include_value_is_ignored | src/filter.rs の test_normalize_empty_strings_removed。include `["", "src"]` で "src" の下だけが出る。空の値は整えられなければ root_dir 全体を指すため、"src" の外の "top.txt" と "other/c.txt" が出ないことで無視を見分ける |
| REQ-config-024（無効な値の無視） | absolute_traversal_and_glob_include_values_are_ignored | src/filter.rs の test_normalize_rejects_absolute_path・test_normalize_rejects_traversal・test_normalize_glob_warning。include に左の root_dir の中の実在するディレクトリ "absolute" を指す絶対パス、"src/../traversal"、名前に glob 文字を含む実在するディレクトリ "lib[1]"・"lib*"・"lib?"（"["・"*"・"?" のそれぞれ）、"src" を書き、"src/a.txt" だけが出る。無効な値は整えられなければ root_dir の中の実在するディレクトリとして走査されるため、その下のファイルが出ないことで無視を見分ける。警告の文言は実行ファイルのテストで確かめる（下の節） |
| REQ-config-025 | exclude_removes_matching_paths_from_the_include_target | src/local/mod.rs の test_include_with_exclude_combined。include "src" と exclude "*.log" で "src/a.txt" だけが出て、include の中の "src/a.log"・"src/deep/b.log" と、include の外の "top.txt"・"top.log" は出ない |
| REQ-config-026（既定） | default_sensitive_patterns_apply_without_a_sensitive_setting | src/service/status.rs の test_sensitive_env_file・test_sensitive_nested_path・test_sensitive_wildcard。sensitive を書かない設定で、六つのパターンのそれぞれに名前が当たるファイル（".env"・"app/.env"・".env.production"・"certs/server.pem"・"server.key"・"credentials.json"・"config/credentials.yml"・"my-secret-notes.txt"）が "sensitive": true、当たらないファイル（"README.md"・"env.txt"・"certs/server.crt"・"keys.txt"、後の二つの場合で使う "token.confidential"・"data/token.confidential"）が false になる |
| REQ-config-026（グローバル設定に足す） | sensitive_pattern_in_the_global_config_is_added_to_the_defaults | sensitive の合成を確かめる単体テストはなく、合成の組み方は src/config.rs の test_include_merged_from_global_and_project と tests/contract/config_merging.rs の `load` を手本にした。グローバル設定だけに sensitive "*.confidential" を書くと、"token.confidential" と "data/token.confidential" も true になり、既定のパターンに当たるファイルは true のまま、他は false のまま |
| REQ-config-026（プロジェクト設定に足す） | sensitive_pattern_in_the_project_config_is_added_to_the_defaults | 同じ。プロジェクト設定（`load_config_from_paths` の第 2 引数）だけに同じパターンを書いて、同じ結果になる。グローバル設定とプロジェクト設定では合成の経路が別のため、両方で確かめる |

- どちらの設定にも同じ [local] を書く。[local] がないときの挙動は FLAG-config-002 の範囲のため避けた。
- 計画は設定に書く sensitive のパターンの例に "*.secretish" を挙げていたが、"secretish" は "secret" を含み既定の "*secret*" に当たるため、例として使えなかった（最初の実行で "token.secretish" が sensitive を書かなくても true になって落ちた）。計画の条件（既定のパターンにも "*secret*" にも当たらない名前）に合わせて "*.confidential" にした。計画からの逸れとして記録する。
- 絶対パスの include は左の root_dir の中を指す。右（develop）の root_dir は別の一時ディレクトリのため、整えられなかったときも右では root_dir の外として捨てられるが、左で走査されて一覧に出るため見分けられる。

### include の無効な値の警告と先頭の "./"（REQ-config-024）

根拠テストは `tests/contract/config_filters_cli.rs` にある（`test-utils` の feature が要る。SSH の試験サーバを使うため）。
警告は設定の読み込み時に標準エラー（tracing の出力）に出て関数呼び出しでは観測できないため、前の回の `tests/contract/config_values_cli.rs` と同じく実行ファイルを起動して確かめる。

- `tests/common/mod.rs` の `TestDirs::new_2way` で一時ディレクトリと SSH の試験サーバを用意し、左右に "src/a.txt" を置く。
- 設定は `gen_config` の本文の [filter] の `exclude = [".git", "target"]` の行を include の行に置き換えて作り、置き換えが起きたことを `assert_ne!` で確かめる（表を重ねると TOML として読めず、末尾に足すと別の表に入って無視されるため）。
- 置き換えるのは [filter] の行だけで、接続先・認証・root_dir・[agent]・[ssh] は `gen_config` のままのため、既存の隔離の確認 `TestDirs::assert_isolated_config_at` をそのまま通してから起動する。前の回の `Workspace` と `assert_isolated_values_config` は共有せず、起動の補助を新しいモジュールに置いた（既存の確認のほうが厳しく、この設定はそれを通るため）。
- 実行ファイルは `env_clear` したうえで `HOME`・`XDG_CONFIG_HOME`・`XDG_DATA_HOME` を一時ディレクトリの下に向け、`PATH` だけを引き継ぎ、標準入力を `Stdio::null()` にして `status --left local --right develop` を起動する。

| 要件 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| REQ-config-024（警告） | absolute_traversal_and_glob_include_values_each_warn_with_the_value | src/filter.rs の test_normalize_rejects_absolute_path・test_normalize_rejects_traversal・test_normalize_glob_warning と、tests/contract/config_values_cli.rs の起動の組み方。include に左の root_dir の下の "src" を指す絶対パス、"src/../src"、"lib[1]"・"lib*"・"lib?"、"src" を書いて起動し、標準出力と標準エラーをつないだもの（ANSI のエスケープを除く）に "Absolute path is not allowed in include filter: 絶対パス"・"Path traversal is not allowed in include filter: src/../src"・"Glob patterns are not supported in include filter: 値"（"lib[1]"・"lib*"・"lib?" のそれぞれ）が含まれる |
| REQ-config-024（"./"） | include_with_leading_dot_slash_lists_the_same_paths_as_the_plain_value_over_ssh | src/filter.rs の test_normalize_leading_dot_slash。左右に "src/a.txt" と "top.txt" を中身を違えて置き、`status --left local --right develop --format json` の標準出力の "files" の "path" の集合が、include "src" でも include "./src" でも "src/a.txt" だけになる。右の SSH の走査は include の値を find の開始パスにそのまま繋ぎ（src/ssh/tree_parser.rs の build_find_command）、走査の結果から root_dir を取り除いたものをパスにするため、"./" が取り除かれなければ右のパスが "./src/a.txt" になって一覧に出る |

- 無効な値は必ず有効な "src" と一緒に書き（全て無効な include は FLAG-config-013 の範囲）、絶対パスと ".." を含む値は試験用の一時ディレクトリの中を指すものにした。変異の下で整え方が壊れても、走査が一時ディレクトリの外に向かわない。
- 終了コードと接続の結果は確かめない。無効な値が無視されることは上の関数呼び出しのテストで確かめる。
- 先頭の "./" と末尾の "/" の取り除きが観測で見分けられるかを、src/filter.rs の normalize_include_paths の該当する処理を一時的に外して `cargo nextest run --all-features --no-fail-fast --test contract -E 'test(/config_filters/)'` で確かめた。"./" を取り除く繰り返しを外すと、SSH の走査のテスト include_with_leading_dot_slash_lists_the_same_paths_as_the_plain_value_over_ssh が一覧 {"./src/a.txt", "src/a.txt"} で落ち、関数呼び出しの include_with_leading_dot_slash_and_trailing_slash_selects_the_same_as_the_plain_value は通った。`trim_end_matches('/')` を外すと、どちらも通った（関数呼び出しのテストに加え、SSH の走査で include "src/" を試す候補のテストも、include `["src/", "src/deep"]` を試す使い捨てのテストも通った。どちらの SSH の走査でも一覧に "src//a.txt" のようなパスや同じパスの重複は出なかったことから、find は末尾に "/" のある開始パスでも "/" を重ねずに出力し、入れ子の include の重なりも一覧では一つにまとまると判断した。find の出力そのものは読んでいない）。どれも確かめた後に `git checkout src/filter.rs` で戻し、`git diff --stat src/` が空に戻ることを確かめた。
- このため、関数呼び出しの include_with_leading_dot_slash_and_trailing_slash_selects_the_same_as_the_plain_value（include "./src/" の一覧が include "src" の一覧と同じになることを確かめていた、この回に足したテスト）は "./" も末尾の "/" も見分けられなかった。利用者の判断で消した（印を外すだけでは kotowari check が test_without_id を error にする）。末尾の "/" の取り除きは status の JSON の "files" の "path" の範囲で見分けられる観測が見つからず、利用者の判断で要件から外した（[決定記録 A10](../decision/records/2026-09-29-adopt-config-filters.md#A10)）。見分けられなかった SSH の走査の候補のテストは足していない。
- glob 文字の値は "[" だけでなく "*" と "?" のそれぞれについて書いた。src/filter.rs の normalize_include_paths の glob 文字の判定から `s.contains('*') ||` を一時的に取り除くと、関数呼び出しの absolute_traversal_and_glob_include_values_are_ignored（一覧に "lib*/a.txt" が出る）とこの節の警告のテストが落ち、`s.contains('?') ||` を取り除くと同じ二つが落ちる（"lib?/a.txt" が出る）ことを `cargo nextest run --all-features --no-fail-fast --test contract -E 'test(/config_filters/)'` で確かめた。どちらも確かめた後に `git checkout src/filter.rs` で戻し、`git diff --stat src/` が空に戻ることを確かめた。

## 整理後の変異テスト

変異テストは決着の対象の関数に絞って一度だけ実行した。並列数は既定の 2 である。

```sh
scripts/mutants.sh --re '(should_exclude|is_path_excluded|normalize_include_paths|is_path_included|is_sensitive|resolve_scan_roots|merge_configs)' src/filter.rs src/service/status.rs src/local/mod.rs src/config.rs
```

コミット 72c32e1（S2 までのテストと記録）で、作業ツリーに変更のない状態で実行し、実行中は作業ツリーに触れていない。
結果は `mutants: caught=46 survived=1 timeout=0 unviable=1 equivalent=0`（48 件、約 13 分）だった。
スクリプトの終了コードは 1（kotowari mutants が 1 件の見逃しを error として報告したため。メモリ上限での停止ではない）。
48 件は全て正規表現に名前の一致する関数の変異で、構造体のフィールドを消す変異は一件も出なかった（他の関数のものも混ざらなかった）。
見逃しを落とすためのテストは足していないため、回し直していない。

関数ごとの内訳（`outcomes.json` から数えた）は次のとおり。

| 関数 | caught | survived | unviable |
|---|---|---|---|
| should_exclude | 2 | 0 | 0 |
| is_path_excluded | 5 | 1 | 0 |
| normalize_include_paths | 15 | 0 | 0 |
| is_path_included | 7 | 0 | 0 |
| is_sensitive | 3 | 0 | 0 |
| resolve_scan_roots | 7 | 0 | 0 |
| merge_configs | 7 | 0 | 1 |

unviable の 1 件は src/config.rs:511:5（`merge_configs` を `Ok(Default::default())` にする変異）で、`AppConfig` が `Default` を実装しないため組み立てられない。

計画が範囲として記録だけすると定めていたもののうち、`is_path_excluded` の "../" を含むパターンの扱い（FLAG-config-015）、`normalize_include_paths` の全て無効なときの結果（FLAG-config-013）、`resolve_scan_roots` の存在しない include と root_dir の外を指す include の扱い（scan の話題）には、見逃しが出なかった（"../" の判定と存在しない include の分岐には変異が作られず、root_dir の外の判定 src/local/mod.rs:167:20 は既存の単体テストで caught）。
`should_exclude`（エージェントの走査だけで使う。scan の話題）と `is_path_included`（TUI のツリーの経路だけで使う。TUI の範囲）の変異は全て既存の単体テストで caught だった。

### 検知したテスト

検知した 46 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗から少し進んで止まるため、集めた名前は検知したテストの全てではなく、先に失敗したものである。
負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）だけで検知された変異はなかった。

- 足したテストが先に失敗したもの:
  - src/config.rs:561:20（delete ! in merge_configs、グローバル設定の sensitive を足すときの重複除き）: sensitive_pattern_in_the_global_config_is_added_to_the_defaults
  - src/config.rs:585:24（delete ! in merge_configs、プロジェクト設定の sensitive を足すときの重複除き）: sensitive_pattern_in_the_project_config_is_added_to_the_defaults
  - src/config.rs:607:8（delete ! in merge_configs、include が空でないときだけ整える分岐）: empty_include_value_is_ignored・absolute_traversal_and_glob_include_values_are_ignored
- 他の変異は src/filter.rs・src/config.rs・src/local/mod.rs・src/service/status.rs・src/app/report.rs・src/agent/ の既存の単体テストか、既存の契約テスト（config_precedence の filter_patterns_from_both_levels_are_combined・status_excludes_files_matching_either_configuration_level）が先に失敗した。足したテストが同じ変異で落ちるかは、nextest が先に止まったため、このログからは分からない。

### 前の回から引き継いだ二件

前の回（[記録](./config-loading-test-cleanup.md)）でフィルターの回に回した `merge_configs` の二件は、どちらもこの回で caught になった。

- src/config.rs:585:24（delete ! in merge_configs）: 変異でプロジェクト設定だけに書いた新しいパターンが足されなくなり、REQ-config-026 に反する。上のとおり、REQ-config-026 のプロジェクト設定の場合のテスト sensitive_pattern_in_the_project_config_is_added_to_the_defaults で落ちた。
- src/config.rs:607:8（delete ! in merge_configs）: 変異で include が空でないときに整えなくなる。変異のログでは関数呼び出しの empty_include_value_is_ignored と absolute_traversal_and_glob_include_values_are_ignored が落ちた。S2 の警告のテストがこの変異で落ちることはログからは分からなかったため、変異を一時的に書き入れて `cargo nextest run --all-features --no-fail-fast --test contract -E 'test(/config_filters/)'` を回し、上の二つに加えて absolute_traversal_and_glob_include_values_each_warn_with_the_value も落ちる（14 件中 3 件が失敗）ことを確かめた。その後 `git checkout src/config.rs` で戻し、`git diff --stat src/` が空に戻ることを確かめた。

### merge_configs のフィルター以外の部分（前の回との比較）

前の回（d962aaf）では `merge_configs` の変異は 8 件（caught 6・survived 1・unviable 1）で、survived と数えられたのは 607:8 の一件だった。585:24 はその回では tui_merge のテストの負荷による失敗で caught と数えられたが見逃しとして扱ったため、見逃しとして扱ったのは 585:24 と 607:8 の二件である。
この回も `merge_configs` の変異は 8 件で、caught 7・survived 0・unviable 1 になった。前の回の記録にある 585:24 と 607:8 は同じ位置に出た。他の 6 件の位置は前の回の記録になく、一致は確かめていない。
フィルターの合成（552 行から 613 行付近）の外にある変異は 511:5 の unviable の一件だけで、前の回と同じである。前の回に記録だけした max_scan_entries と badge_scan_max_files の変異（FLAG-config-003）と [local] がないときのエラーの変異（FLAG-config-002）は、この回も見逃しとして出なかった。

### 見逃しと決着

計画の区別により、FLAG-config-013・015 の挙動、TUI のツリーの表示の範囲、scan の話題は決着の対象から外して記録だけする。それ以外の見逃しは全て決着の対象にした。

| 位置 | 変異 | 行の中身 | 決着の対象 | 決着 |
|---|---|---|---|---|
| src/filter.rs:62:35 | replace \|\| with && in is_path_excluded | "dir/**" の形のパターンでディレクトリそのものを外す枝刈り（`if path == prefix \|\| glob_match::glob_match(prefix, path)`） | 対象でない（TUI のツリーの表示の範囲） | 記録だけする。変異で変わるのは、prefix に glob 文字を含み（例 "vendor/*/**"）path が prefix と文字として違うディレクトリ（例 "vendor/x"）を外すかどうかだけである。その下のファイル（"vendor/x/a.txt"）はパス全体の glob（`glob_match(pattern, path)`）で外れるため、status のファイルの一覧は変わらず、ディレクトリの行を出す TUI のツリーでだけ見える（実装を読んだ判断で、TUI で実行しての確認はしていない）。既存の単体テスト（src/filter.rs の test_path_pattern_vendor_legacy など）でも落ちていない。prefix に glob 文字を含まない "vendor/legacy/**" では、`path == prefix` が真なら `glob_match(prefix, path)` も真のため、変異の前後で結果が変わらない |

決着の対象の見逃しは残っていない。見逃しや新しいテストが不具合の疑いを示したものはない。同等変異の登録はしていない。

## 要件の verification の見直し

REQ-config-021 から 026 の verification は全て unit で、いずれも具体的なパターン・include の値・ファイルのパスの組み合わせで、一覧に出るかと sensitive かが決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-config-021 | unit | "/" を含まないパターンと、要素（ファイル名・途中のディレクトリ名）の具体的なパスの組で、外れるかが決まる |
| REQ-config-022 | unit | "/" を含むパターンと具体的な相対パスの組で、外れるかが決まる |
| REQ-config-023 | unit | include の値と、区切りの単位で続くか途中で続くかの具体的なパスの組で、対象になるかが決まる |
| REQ-config-024 | unit | 有限の書き方（"./"・空）と三つの無効な値の種類で、整えた値か警告の文言が決まる |
| REQ-config-025 | unit | include と exclude のそれぞれに当たるかの組み合わせで、対象になるかが決まる |
| REQ-config-026 | unit | 六つの既定のパターンと設定に書いたパターン（グローバル・プロジェクト）で、sensitive かが決まる |

## 利用者の判断

新しい FLAG の候補、verification の見直しの候補、決着の対象でない見逃しで計画の区分に当てはまらないものはない。
決着の対象でない見逃しは src/filter.rs:62:35 の一件で、計画が TUI の範囲として記録だけすると定めていた `is_path_excluded` の "dir/**" のディレクトリの枝刈りに当たる。

利用者の判断（2026-09-29）:

1. include の値の末尾の "/" を取り除くことは要件から外した（[決定記録 A10](../decision/records/2026-09-29-adopt-config-filters.md#A10)）。ローカルの走査は canonicalize で、SSH の走査は find の出力で末尾の "/" を吸収し、status の JSON の "files" の "path" では整え方が壊れても違いが出なかったため（上の「include の無効な値の警告と先頭の "./"」の節の注）。末尾の "/" の根拠テストは置かない。
2. "./" も末尾の "/" も見分けられない関数呼び出しのテスト include_with_leading_dot_slash_and_trailing_slash_selects_the_same_as_the_plain_value は、この回に足したテストのため消した。
3. path_pattern_excludes_only_paths_whose_whole_relative_path_matches から、"config/sub/a.toml" が exclude "config/*.toml" で外れないこと（"*" が "/" をまたがないこと）を確かめる部分を、ファイルの構成と期待の両方から外した。REQ-config-022 は相対パス全体に glob で当たるときに除外するとだけ書き、"*" と "/" の関係を契約にしていないため。
