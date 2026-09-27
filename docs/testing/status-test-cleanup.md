# status のテスト整理の記録

status の取り込みで加えた要件（REQ-cli-027 から REQ-cli-037）と例 EX-cli-062 の根拠テストを整理した過程の記録。
変異テストの結果、各要件の根拠にしたテスト、削除候補と利用者の判断、整理後の見逃しの決着を残す。

## 整理前の変異テスト

整理を始める前のコミット d55de4f（作業ツリーに変更のない状態）で、次のコマンドを一度だけ実行した。
五つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/service/status.rs src/cli/status.rs src/cli/ref_guard.rs src/service/source_pair.rs src/tree.rs
```

全体の集計は `mutants: caught=178 survived=20 timeout=0 unviable=34 equivalent=0`（232 件、実行時間は約 52 分）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/service/status.rs | 99 | 6 | 0 | 9 |
| src/cli/status.rs | 13 | 7 | 0 | 2 |
| src/cli/ref_guard.rs | 3 | 0 | 0 | 1 |
| src/service/source_pair.rs | 11 | 0 | 0 | 7 |
| src/tree.rs | 52 | 7 | 0 | 15 |

### 見逃し

位置は変異が入る行、変異は kotowari mutants の出力の文言のまま。
「決着の対象」は計画の区別による。status の判定の規則を持たない関数（src/service/status.rs の `verified_content_pairs`・`needs_explicit_file_compare`・`needs_merge_content_compare`・`status_from_read_results`、src/service/source_pair.rs の `resolve_source_pairs`、src/tree.rs の `compare_metadata` 以外の関数）の見逃しは記録だけして決着の対象にしない。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/cli/status.rs:106 | delete ! in execute_status | 対象 |
| src/cli/status.rs:106 | replace && with \|\| in execute_status | 対象 |
| src/cli/status.rs:146 | replace && with \|\| in execute_status | 対象 |
| src/cli/status.rs:170 | delete ! in execute_status | 対象 |
| src/cli/status.rs:198 | replace > with < in execute_status | 対象 |
| src/cli/status.rs:198 | replace > with == in execute_status | 対象 |
| src/cli/status.rs:198 | replace > with >= in execute_status | 対象 |
| src/service/status.rs:96 | replace && with \|\| in TreeIndex<'a>::record_node | 対象 |
| src/service/status.rs:137 | replace path_is_within_unloaded_dir -> bool with true | 対象 |
| src/service/status.rs:249 | replace \|\| with && in needs_content_compare | 対象 |
| src/service/status.rs:320 | replace && with \|\| in needs_merge_content_compare | 対象外（merge の比較対象） |
| src/service/status.rs:323 | replace && with \|\| in needs_merge_content_compare | 対象外（merge の比較対象） |
| src/service/status.rs:560 | replace > with < in status_exit_code | 対象 |
| src/tree.rs:202 | replace FileTree::sort with () | 対象外（ツリーの構造の操作） |
| src/tree.rs:284 | replace += with *= in ensure_path_in_nodes | 対象外（ツリーの構造の操作） |
| src/tree.rs:284 | replace += with -= in ensure_path_in_nodes | 対象外（ツリーの構造の操作） |
| src/tree.rs:285 | replace - with + in ensure_path_in_nodes | 対象外（ツリーの構造の操作） |
| src/tree.rs:285 | replace - with / in ensure_path_in_nodes | 対象外（ツリーの構造の操作） |
| src/tree.rs:302 | replace > with >= in ensure_path_in_nodes | 対象外（ツリーの構造の操作） |
| src/tree.rs:324 | replace > with >= in ensure_path_in_btree_node | 対象外（ツリーの構造の操作） |

### 変異と関係のないテストだけによる検知

検知した 178 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
変異した関数と関係がなく、負荷の下で落ちることのあるテストだけで検知された変異は次の三件。

| 位置 | 変異 | 失敗したテスト |
|---|---|---|
| src/cli/status.rs:125 | delete ! in execute_status | agent_ssh_deploy の agent_ssh_exec_handshake、agent_ssh_list_tree_roundtrip |
| src/service/status.rs:143 | replace + with * in collect_all_file_paths | tui_merge の test_hunk_merge_left_to_right_with_l |
| src/tree.rs:264 | replace sort_nodes with () | tui_merge の test_hunk_merge_right_to_left_with_h_key、test_merge_cancel_with_n、test_sensitive_file_merge_requires_confirmation |

前の二件は結果が変わらない変異（ハッシュ比較を試す条件の反転で、試さなくても中身を読む経路で同じ判定になる。後者は容量の見積もりだけ）で、見かけの検知と判断した。
`sort_nodes` はツリーの並び順を変えるため TUI の操作に影響しうるが、関係があるかは確かめていない。
整理後の比較では、この三件は検知と見逃しのどちらにもなりうるものとして扱う。

ほかに、公開された入口を通すテストでだけ検知された変異があり、整理で消えるテストに頼っているため整理後の比較で注意する。

- `run_status` の戻り値を `Ok(0)`・`Ok(1)`・`Ok(-1)` にする三件: tests/cli_error_handling.rs の test_invalid_server_name_rejected（`Ok(1)` は test_ref_with_left_equal_fails_on_ssh と test_ref_with_right_equal_fails_on_ssh でも）
- src/cli/status.rs:134・139・146 の `delete !` 四件（中身を取得する条件）: tests/cli_exit_codes.rs の test_status_exit_0_when_no_diff
- `print_status_result` を何もしない変異: tests/cli_status.rs の test_status_all_includes_equal、test_status_exclude_filter_works、test_status_excludes_equal_by_default

## 要件ごとの根拠テスト

根拠テストは `tests/contract/` の下に IR の文書ごとのモジュールとして置いた。
関数呼び出しの準備（サーバ "develop" と "staging" を一時ディレクトリに差し替えた構成）は `tests/contract/status_support.rs` にまとめた。
一つの要件に複数の経路や場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。

src/ の中の単体テスト（公開された入口を通さない純粋関数のテスト）は、その振る舞いを公開された入口から確かめるテストを新しく書き、元のテストは移し元に残して削除候補の節に挙げる。
消すかどうかを利用者の一括の判断に任せ、整理後の変異テストの比較を一度で済ませるため（バックアップの整理と同じ扱い）。
tests/ の直下の実行ファイルを起動するテストのうち、同じ振る舞いを入口を通すテストに移したものは移し元から消した。

### 判定（docs/ir/cli/status.md）

根拠テストは全て `tests/contract/status_judgement.rs` にある。
右がサーバで --ref がないときはハッシュの経路、右が local のときは中身を読む経路で比べるため、中身で判定する行は `--left local --right develop` と `--left develop --right local` の両方で確かめる。
サイズと更新時刻が同じ行と、サイズが違う行と、symlink の二件も同じく両方で確かめる。

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-cli-027（左だけ・右だけ・ファイルとディレクトリ） | one_sided_files_are_left_or_right_only_and_a_file_against_a_directory_is_modified | src/service/status.rs の test_status_left_only、test_status_right_only、test_status_nested_files、test_status_file_vs_directory_path_conflict_is_modified。入れ子のパスを含めた |
| REQ-cli-027（サイズが違う） | files_of_different_sizes_are_modified | src/service/status.rs の test_status_modified_when_different_size |
| REQ-cli-027（サイズと更新時刻が同じ） | same_size_and_timestamp_are_equal_without_reading_the_content | src/service/status.rs の test_status_equal_when_same_size_and_mtime。中身が違うファイルが "equal" になることで中身を読まないことを確かめる |
| REQ-cli-027（サイズが同じで更新時刻が違う） | same_size_with_different_timestamps_is_decided_by_the_content | src/service/status.rs の test_status_modified_when_same_size_different_mtime と、refine_status_with_content の test_refine_status_equal_when_content_matches、test_refine_status_stays_modified_when_content_differs、test_refine_status_binary_identical_is_equal、test_refine_status_binary_different_is_modified、test_refine_status_text_still_works。テキストとバイナリの両方で、同じなら "equal"・違えば "modified" |
| REQ-cli-027（サイズか更新時刻が分からない） | unknown_size_or_timestamp_is_decided_by_the_content | src/service/status.rs の test_status_modified_when_no_metadata。ローカルのディレクトリからはこのノードを作れないため、公開の `compute_status_from_trees`・`needs_content_compare`・`refine_status_with_content` を status と同じ順に組み合わせ、サイズも更新時刻もないノードと、サイズだけあるノードで確かめる |
| REQ-cli-028 | two_symlinks_are_compared_by_their_target_text_without_reading_the_content | src/service/status.rs の test_status_symlink_same_target_is_equal、test_status_symlink_different_target_is_modified。リンク先の文字列が同じで左右のリンク先の中身が違う組とリンク先がない組が "equal"、文字列が違いリンク先の中身が同じ組が "modified" |
| EX-cli-062（REQ-cli-028 の片方だけ symlink） | a_regular_file_against_a_symlink_to_the_same_content_is_modified | src/service/status.rs の test_status_symlink_vs_file_is_modified |

- 二つの構成が本当に別の経路を通ることは、コミットに含めない一時的な書き換えで確かめた。`refine_status_with_hashes` を何もしないようにすると same_size_with_different_timestamps_is_decided_by_the_content が落ち、`refine_status_with_content` を何もしないようにすると同じテストと unknown_size_or_timestamp_is_decided_by_the_content が落ちた。
- refine_status_with_content の七件のうち、test_refine_status_skips_non_modified（片側だけのファイルを中身で変えない）は、片側だけのファイルが中身の比較の対象に入らないため入口から観測できず、test_refine_status_equal_to_modified_when_content_differs（--checksum で "equal" を "modified" に直す）は REQ-cli-008 の既存の根拠テスト（tests/contract/status_results.rs の checksum_finds_different_bytes_despite_equal_size_and_timestamp）と重なるため、どちらも根拠にせず削除候補に挙げる。
- tests/cli_status.rs の test_status_text_shows_modified_files、test_status_text_shows_left_only、test_status_text_shows_right_only は判定よりテキストの記号を確かめるテストのため、出力の要件（REQ-cli-031）の根拠テストへ移す。

### 出力と終了コード（docs/ir/cli/status-output.md）

根拠テストは全て `tests/contract/status_output.rs` にある。
集計は関数呼び出しで、終了コードと標準出力は隔離された SSH fixture に対して実行ファイルを起動して確かめる。
このモジュールは SSH fixture を使うため、ほかの SSH のテストと同じく `test-utils` の feature があるときだけ組み込む。
テキストの確認では空行を除いた行を見る。区切りの空行の数とファイルの行の順序は IR が定めていないため固定しない。

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-cli-029 | summary_counts_every_file_including_equal_ones_with_or_without_all_and_summary | src/cli/status.rs の test_summary_equal_count_preserved_after_filter と、tests/cli_status.rs の test_status_excludes_equal_by_default（移して消した）。--all と --summary の四通りの組み合わせで四種の数が同じ |
| REQ-cli-030（0） | exit_code_is_zero_when_every_file_is_equal | tests/cli_exit_codes.rs の test_status_exit_0_when_no_diff（移して消した） |
| REQ-cli-030（1） | exit_code_is_one_when_any_file_is_modified_left_only_or_right_only | tests/cli_exit_codes.rs の test_status_exit_1_when_diff_found（移して消した）と src/service/status.rs の test_exit_code_has_diff、test_exit_code_left_only。"modified"・"left_only"・"right_only" のそれぞれ一件だけの場合 |
| REQ-cli-030（2） | exit_code_is_two_for_an_unknown_server_or_identical_sides | 新しく書いた。設定にないサーバ名と、左右に同じサーバを指定した場合。tests/cli_error_handling.rs の二件は比較対象の要件と合わせて移す（下の節） |
| REQ-cli-031（見出し・記号・機密ファイル・Summary 行） | text_lists_a_header_one_symbol_line_per_file_and_a_final_summary | tests/cli_status.rs の test_status_text_shows_modified_files、test_status_text_shows_left_only、test_status_text_shows_right_only、test_status_sensitive_files_included（移して消した）と src/service/output.rs の test_format_status_text、test_status_header_without_ref、test_status_text_left_only_symbol、test_status_text_right_only_symbol。"=" と " [SENSITIVE]" と Summary 行が最後の行であることを書き足した |
| REQ-cli-031（形式の選択） | format_accepts_text_json_and_diff_as_text_and_rejects_other_values | src/service/output.rs の test_output_format_parse。"diff" と "text" の出力が既定と同じバイト列であることと、未知の値が終了コード 2 になることを実行ファイルで確かめる |
| REQ-cli-032 | json_has_both_sides_every_file_and_the_summary | tests/cli_status.rs の test_status_json_format（移して消した）と src/service/types.rs の test_status_output_serialize、test_file_status_kind_serializes_snake_case。"left" と "right" の "label" と "root"、全ファイルの "path"・"status"・"sensitive"、"status" の四つの値、"summary" の四種の数を確かめる。右の "root" の形（"ホスト:パス"）は IR が定めていないため、右のディレクトリを含むことだけを見る |
| REQ-cli-033 | summary_prints_only_the_header_and_counts_and_json_omits_files | tests/cli_status.rs の test_status_summary_shows_counts（移して消した）と src/service/status.rs の test_build_status_output_summary_only、src/service/output.rs の test_format_status_text_summary_only、test_status_header_appears_in_summary_only_mode、src/service/types.rs の test_status_summary_serialize。テキストは空行を除くと見出しと Summary 行の二行だけ、JSON は "files" がない |

- tests/cli_status.rs の test_status_json_special_chars_in_path（空白を含むパスの JSON）は、パスの文字の扱いを IR が定めていないため根拠にせず、削除候補に挙げる。
- Summary 行が最後の行であることは -v を指定しない場合だけを見る。-v での Agent 行は FLAG-cli-001 の範囲のため確かめない。

### 比較対象と三者比較（docs/ir/cli/status-targets.md）

根拠テストは `tests/contract/status_targets.rs` と、終了コード 2 のテスト（`tests/contract/status_output.rs`）にある。
左右の決め方は関数呼び出しで、参照先の表示と警告は隔離された SSH fixture に対して実行ファイルを起動して確かめる。
三者比較の参照先は、SSH fixture の三サーバ構成（左 develop・右 staging・参照先 local）を使う。
`tests/contract/status_targets.rs` も SSH fixture を使うため `test-utils` の feature があるときだけ組み込む。

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-cli-034（TBL-cli-004 の四行） | sides_come_from_left_and_right_with_local_and_the_default_server_filling_the_gaps | src/service/source_pair.rs の test_left_and_right_explicit、test_no_args_uses_first_config_server、test_right_only、source_pair_left_local_right_remote、source_pair_left_remote_right_local、test_left_only_local_falls_back_to_default、test_left_only_nondefault_succeeds。結果の "left" と "right" の "label" で確かめる |
| REQ-cli-034（左右が同じ・設定にないサーバ名） | identical_sides_and_unknown_servers_stop_with_an_error、exit_code_is_two_for_an_unknown_server_or_identical_sides | src/service/source_pair.rs の test_unknown_server_returns_error、source_pair_both_local_errors、test_same_left_right_explicit_error、test_same_left_right_local_error と、tests/cli_error_handling.rs の test_invalid_server_name_rejected、test_self_compare_rejected（移して消した。後者は終了コード 2 のテストに REQ-cli-034 の印を足した）。エラーの文言は IR が定めていないため、エラーで止まることだけを見る |
| REQ-cli-034（既定サーバで補って同じになった） | sides_made_identical_by_the_default_server_say_so_in_the_error | src/service/source_pair.rs の test_left_only_falls_back_to_default_server、test_implicit_right_error_message_contains_context。エラーに既定サーバの名前と "default server" が含まれることだけを見る |
| REQ-cli-034（サーバが一つもない） | a_needed_default_server_without_any_server_configured_is_an_error | src/service/source_pair.rs の test_no_servers_in_config。何も指定しない場合と --left local だけの場合 |
| REQ-cli-035（TBL-cli-005 の三行と JSON） | json_marks_each_file_against_the_ref_and_counts_the_marks | src/service/status.rs の test_compute_ref_badges_differs、test_compute_ref_badges_missing_in_ref、test_compute_ref_badges_empty_ref_tree と src/service/types.rs の test_status_output_with_ref_serialize。左右の両方にあって参照先にない組、左だけ・右だけにあって参照先にある組、三つにあって中身の違う組（左右が同じで参照先だけが違う組を含む）を確かめる。"ref_only" の値は FLAG-cli-002 の範囲のため数であることだけを見る |
| REQ-cli-035（テキスト） | text_names_the_ref_in_the_header_marks_files_and_adds_a_ref_line_after_the_summary | tests/cli_status.rs の test_status_with_ref_shows_badges（移して消した）と src/service/output.rs の test_status_header_with_ref。見出しの "(ref: local)"、" [ref-]" と " [ref≠]"、Summary 行の後の Ref 行を書き足した。Ref 行の ref-only の数は上と同じ理由で固定しない |
| REQ-cli-036 | json_marks_each_file_against_the_ref_and_counts_the_marks | src/service/status.rs の test_compute_ref_badges_sensitive_missing_in_ref、test_compute_ref_badges_sensitive_exists_in_ref、test_compute_ref_badges_sensitive_skipped、test_compute_ref_badges_sensitive_no_content_leak。参照先にない機密ファイルが "missing_in_ref"、三つにあって中身の違う機密ファイルに印がない |
| REQ-cli-037 | a_ref_equal_to_either_side_warns_and_compares_without_the_ref | src/cli/ref_guard.rs の ref_same_as_left_returns_none、ref_same_as_right_returns_none。警告の文言が標準エラーに出て、見出しに "(ref:" も Ref 行もなく、比較の結果（"M file.txt" と終了コード 1）が出ることを確かめる |

- src/cli/ref_guard.rs の ref_different_returns_some と ref_none_returns_none は、参照先が左右と違うときに三者比較をすること・--ref なしでは三者比較をしないことを見るもので、上の REQ-cli-035 の根拠テストと --ref なしの全ての根拠テストが同じ振る舞いを確かめるため、削除候補に挙げる。
- tests/cli_error_handling.rs の test_ref_with_left_equal_fails_on_ssh と test_ref_with_right_equal_fails_on_ssh は、SSH の接続が警告より先に失敗することを確かめるもので、REQ-cli-037 の根拠にならないため削除候補に挙げる。
- FLAG-cli-002 の範囲の test_compute_ref_badges_all_equal と test_compute_ref_summary、src/service/output.rs の test_format_status_text_with_ref_badges は移さず、削除候補にもしない。

## 削除候補と利用者の判断

整理の計画で削除を利用者が一括で判断する段の入力。
利用者の返答（消すものの一覧）を下の「判断の結果」に書き足してから削除する。まだ何も消していない。

一覧は、計画が名指しした候補（A）と、上の審査で公開された入口を通す根拠テストに置き換えた純粋関数の単体テスト（B）と、tests/ の直下の重複テスト（C）を合わせた 99 件。
「代わりの根拠」は、そのテストが確かめていた振る舞いを今確かめている根拠テスト（モジュール名は tests/contract/ の下のファイル名）。
src/service/output.rs と src/service/types.rs は変異テストの対象外のため、この二ファイルの候補は消しても整理後の変異テストで見逃しの増減として現れない。二ファイルの候補の「代わりの根拠」は同じ整形の分岐を確かめて残るテストで、その行に「裏付けなし」と書いた。

候補にしていないもの:

- FLAG-cli-001 から 005 の挙動を確かめるテスト（src/cli/status.rs の test_determine_agent_status_remote_no_agent と test_determine_agent_status_local、src/service/output.rs の agent を扱う五件と test_format_status_text_with_ref_badges、src/service/types.rs の test_agent_status_serialize と test_agent_status_deserialize、src/service/status.rs の test_compute_ref_badges_all_equal と test_compute_ref_summary）。
- src/service/status.rs の test_refine_with_hashes_symlink_comparison。symlink のハッシュの比較は --checksum のときだけ起こり、FLAG-cli-003 の範囲のため。
- src/service/status.rs の is_sensitive の三件（test_sensitive_env_file、test_sensitive_nested_path、test_sensitive_wildcard）と status_from_read_results の十件。前者は用語「機密ファイル」の判定、後者は diff・merge の処理で、この計画の要件の根拠でないため。
- src/tree.rs の compare_metadata の八件。計画で削除できるファイルに src/tree.rs が入っていないため。compare_metadata は merge と TUI も使う。
- tests/cli_status.rs の test_status_exclude_filter_works（除外フィルターの話題）と、tests/cli_error_handling.rs の設定の誤りとヘルプのテスト。

### A. 計画が名指しした候補（9 件）

| ファイル | テスト | 理由と代わりの根拠 |
|---|---|---|
| src/service/status.rs | test_hash_comparison_equal | status_from_hash_comparison の文字列比較を見るだけ。ハッシュの経路の判定は status_judgement の same_size_with_different_timestamps_is_decided_by_the_content |
| src/service/status.rs | test_hash_comparison_modified | 同上 |
| src/service/status.rs | test_hash_comparison_symlink_same_target | 同上。symlink のリンク先の文字列が渡るのは --checksum のときだけ（FLAG-cli-003） |
| src/service/status.rs | test_hash_comparison_symlink_different_target | 同上 |
| src/service/status.rs | test_hash_comparison_empty_strings | 同上 |
| tests/cli_error_handling.rs | test_ref_with_left_equal_fails_on_ssh | SSH の接続が警告より先に失敗することを見るだけ。警告は status_targets の a_ref_equal_to_either_side_warns_and_compares_without_the_ref |
| tests/cli_error_handling.rs | test_ref_with_right_equal_fails_on_ssh | 同上 |
| src/service/output.rs | test_format_status_text_with_hunks | 作らないと決めた hunks（決定記録 A20）の表示。裏付けなし。同じ行の組み立てを確かめて残るテストは status_output の text_lists_a_header_one_symbol_line_per_file_and_a_final_summary |
| src/service/types.rs | test_status_output_with_hunks | 同上の JSON。裏付けなし。同じ形を確かめて残るテストは status_output の json_has_both_sides_every_file_and_the_summary |

### B. 入口を通す根拠テストに置き換えた純粋関数の単体テスト（87 件）

| ファイル | テスト | 代わりの根拠 |
|---|---|---|
| src/service/status.rs | test_status_left_only | status_judgement の one_sided_files_are_left_or_right_only_and_a_file_against_a_directory_is_modified |
| src/service/status.rs | test_status_right_only | 同上 |
| src/service/status.rs | test_status_nested_files | 同上 |
| src/service/status.rs | test_status_file_vs_directory_path_conflict_is_modified | 同上 |
| src/service/status.rs | test_status_both_exist | status_judgement の unknown_size_or_timestamp_is_decided_by_the_content（メタデータのないノードは中身を読むまで "modified"） |
| src/service/status.rs | test_status_modified_when_no_metadata | 同上 |
| src/service/status.rs | test_status_unloaded_dir_vs_file_stays_modified | なし。未読み込みのディレクトリは status の再帰の走査からは作れず、IR にない場合 |
| src/service/status.rs | test_status_sensitive_flag | status_output の json_has_both_sides_every_file_and_the_summary（".env" の "sensitive"） |
| src/service/status.rs | test_status_equal_when_same_size_and_mtime | status_judgement の same_size_and_timestamp_are_equal_without_reading_the_content |
| src/service/status.rs | test_status_modified_when_different_size | status_judgement の files_of_different_sizes_are_modified |
| src/service/status.rs | test_status_modified_when_same_size_different_mtime | status_judgement の same_size_with_different_timestamps_is_decided_by_the_content |
| src/service/status.rs | test_needs_content_compare_filters_different_size | 同上と files_of_different_sizes_are_modified（どのファイルの中身を読むかは内部の選び方） |
| src/service/status.rs | test_needs_content_compare_handles_file_vs_directory_conflict | status_judgement の one_sided_files_are_left_or_right_only_and_a_file_against_a_directory_is_modified |
| src/service/status.rs | test_needs_content_compare_all_includes_equal | status_results の checksum_finds_different_bytes_despite_equal_size_and_timestamp（EX-cli-015） |
| src/service/status.rs | test_needs_content_compare_all_excludes_left_right_only | なし。片側だけのファイルを比較の対象に入れないことは入口から観測できない |
| src/service/status.rs | test_refine_status_equal_to_modified_when_content_differs | status_results の checksum_finds_different_bytes_despite_equal_size_and_timestamp（EX-cli-015） |
| src/service/status.rs | test_refine_status_equal_when_content_matches | status_judgement の same_size_with_different_timestamps_is_decided_by_the_content |
| src/service/status.rs | test_refine_status_stays_modified_when_content_differs | 同上 |
| src/service/status.rs | test_refine_status_skips_non_modified | なし。片側だけのファイルは中身の比較の対象に入らないため入口から観測できない |
| src/service/status.rs | test_refine_status_binary_identical_is_equal | status_judgement の same_size_with_different_timestamps_is_decided_by_the_content（バイナリ） |
| src/service/status.rs | test_refine_status_binary_different_is_modified | 同上 |
| src/service/status.rs | test_refine_status_text_still_works | 同上 |
| src/service/status.rs | test_refine_with_hashes_equal | 同上（ハッシュの経路） |
| src/service/status.rs | test_refine_with_hashes_modified | 同上 |
| src/service/status.rs | test_refine_with_hashes_skips_left_only | なし。片側だけのファイルはハッシュの比較の対象に入らないため入口から観測できない |
| src/service/status.rs | test_refine_with_hashes_missing_hash_keeps_status | なし。片側のハッシュだけが得られない状況は入口から作れない |
| src/service/status.rs | test_refine_with_hashes_metadata_equal_to_modified | status_results の checksum_finds_different_bytes_despite_equal_size_and_timestamp（EX-cli-015。右がサーバのためハッシュの経路） |
| src/service/status.rs | test_status_symlink_same_target_is_equal | status_judgement の two_symlinks_are_compared_by_their_target_text_without_reading_the_content |
| src/service/status.rs | test_status_symlink_different_target_is_modified | 同上 |
| src/service/status.rs | test_status_symlink_vs_file_is_modified | status_judgement の a_regular_file_against_a_symlink_to_the_same_content_is_modified（EX-cli-062） |
| src/service/status.rs | test_summary | status_output の summary_counts_every_file_including_equal_ones_with_or_without_all_and_summary |
| src/service/status.rs | test_build_status_output_with_files | status_output の json_has_both_sides_every_file_and_the_summary |
| src/service/status.rs | test_build_status_output_summary_only | status_output の summary_prints_only_the_header_and_counts_and_json_omits_files |
| src/service/status.rs | test_build_status_output_no_ref_backward_compat | なし。--ref なしで "ref" と ref の集計が出ないことは IR にない |
| src/service/status.rs | test_exit_code_no_diff | status_output の exit_code_is_zero_when_every_file_is_equal |
| src/service/status.rs | test_exit_code_has_diff | status_output の exit_code_is_one_when_any_file_is_modified_left_only_or_right_only |
| src/service/status.rs | test_exit_code_left_only | 同上 |
| src/service/status.rs | test_compute_ref_badges_differs | status_targets の json_marks_each_file_against_the_ref_and_counts_the_marks |
| src/service/status.rs | test_compute_ref_badges_missing_in_ref | 同上 |
| src/service/status.rs | test_compute_ref_badges_empty_ref_tree | 同上 |
| src/service/status.rs | test_compute_ref_badges_sensitive_skipped | 同上（REQ-cli-036） |
| src/service/status.rs | test_compute_ref_badges_sensitive_missing_in_ref | 同上 |
| src/service/status.rs | test_compute_ref_badges_sensitive_exists_in_ref | 同上 |
| src/service/status.rs | test_compute_ref_badges_sensitive_no_content_leak | 同上 |
| src/cli/status.rs | test_summary_equal_count_preserved_after_filter | status_output の summary_counts_every_file_including_equal_ones_with_or_without_all_and_summary |
| src/cli/ref_guard.rs | ref_same_as_left_returns_none | status_targets の a_ref_equal_to_either_side_warns_and_compares_without_the_ref |
| src/cli/ref_guard.rs | ref_same_as_right_returns_none | 同上 |
| src/cli/ref_guard.rs | ref_different_returns_some | status_targets の json_marks_each_file_against_the_ref_and_counts_the_marks |
| src/cli/ref_guard.rs | ref_none_returns_none | --ref なしの全ての根拠テスト |
| src/service/source_pair.rs | test_left_and_right_explicit | status_targets の sides_come_from_left_and_right_with_local_and_the_default_server_filling_the_gaps |
| src/service/source_pair.rs | test_no_args_uses_first_config_server | 同上 |
| src/service/source_pair.rs | test_right_only | 同上 |
| src/service/source_pair.rs | source_pair_left_local_right_remote | 同上 |
| src/service/source_pair.rs | source_pair_left_remote_right_local | 同上 |
| src/service/source_pair.rs | test_left_only_local_falls_back_to_default | 同上 |
| src/service/source_pair.rs | test_left_only_nondefault_succeeds | 同上 |
| src/service/source_pair.rs | test_unknown_server_returns_error | status_targets の identical_sides_and_unknown_servers_stop_with_an_error |
| src/service/source_pair.rs | source_pair_both_local_errors | 同上 |
| src/service/source_pair.rs | test_same_left_right_explicit_error | 同上 |
| src/service/source_pair.rs | test_same_left_right_local_error | 同上 |
| src/service/source_pair.rs | test_left_only_falls_back_to_default_server | status_targets の sides_made_identical_by_the_default_server_say_so_in_the_error |
| src/service/source_pair.rs | test_implicit_right_error_message_contains_context | 同上 |
| src/service/source_pair.rs | test_no_servers_in_config | status_targets の a_needed_default_server_without_any_server_configured_is_an_error |
| src/service/source_pair.rs | test_resolve_ref_source_remote | なし。参照先にサーバを指定した三者比較は根拠テストにない（参照先は local で確かめている） |
| src/service/source_pair.rs | test_resolve_ref_source_local | status_targets の json_marks_each_file_against_the_ref_and_counts_the_marks |
| src/service/source_pair.rs | test_resolve_ref_source_nonexistent | なし。--ref に設定にないサーバ名を指定したときのエラーは IR にない |
| src/service/source_pair.rs | test_resolve_ref_source_none | --ref なしの全ての根拠テスト |
| src/service/source_pair.rs | test_resolve_ref_source_same_as_left | status_targets の a_ref_equal_to_either_side_warns_and_compares_without_the_ref（左右と同じ参照先もエラーにならない） |
| src/service/output.rs | test_format_status_text | status_output の text_lists_a_header_one_symbol_line_per_file_and_a_final_summary。裏付けなし |
| src/service/output.rs | test_status_text_left_only_symbol | 同上。裏付けなし |
| src/service/output.rs | test_status_text_right_only_symbol | 同上。裏付けなし |
| src/service/output.rs | test_status_header_without_ref | 同上。裏付けなし |
| src/service/output.rs | test_format_status_text_summary_only | status_output の summary_prints_only_the_header_and_counts_and_json_omits_files。裏付けなし |
| src/service/output.rs | test_status_header_appears_in_summary_only_mode | 同上。裏付けなし |
| src/service/output.rs | test_output_format_parse | status_output の format_accepts_text_json_and_diff_as_text_and_rejects_other_values。裏付けなし |
| src/service/output.rs | test_status_header_with_ref | status_targets の text_names_the_ref_in_the_header_marks_files_and_adds_a_ref_line_after_the_summary。裏付けなし |
| src/service/output.rs | test_format_status_text_no_ref_backward_compat | --ref なしのテキストの根拠テスト（status_output の text_lists_a_header_one_symbol_line_per_file_and_a_final_summary は Summary 行が最後であることを見る）。裏付けなし |
| src/service/output.rs | test_format_json | status_output の json_has_both_sides_every_file_and_the_summary。裏付けなし |
| src/service/output.rs | test_status_json_not_affected_by_header | 同上（JSON として読めることを見る）。裏付けなし |
| src/service/types.rs | test_status_output_serialize | status_output の json_has_both_sides_every_file_and_the_summary。裏付けなし |
| src/service/types.rs | test_file_status_kind_serializes_snake_case | 同上。裏付けなし |
| src/service/types.rs | test_status_summary_serialize | status_output の summary_prints_only_the_header_and_counts_and_json_omits_files。裏付けなし |
| src/service/types.rs | test_status_output_with_ref_serialize | status_targets の json_marks_each_file_against_the_ref_and_counts_the_marks。裏付けなし |
| src/service/types.rs | test_status_output_without_ref_serialize | なし。--ref なしで "ref" が出ないことは IR にない。裏付けなし |
| src/service/types.rs | test_file_status_ref_badge_serialize | status_targets の json_marks_each_file_against_the_ref_and_counts_the_marks。裏付けなし |
| src/service/types.rs | test_file_status_ref_badge_none_omitted | なし。--ref なしで "ref_badge" が出ないことは IR にない。裏付けなし |
| src/service/types.rs | test_status_summary_backward_compat_deserialize | なし。status の JSON を読み込む処理は製品コードにない（集計の読み込みの後方互換）。裏付けなし |

### C. 根拠にしなかった tests/ の直下の重複テスト（3 件）

| ファイル | テスト | 理由 |
|---|---|---|
| tests/cli_status.rs | test_status_all_includes_equal | status_results の all_status_includes_equal_files（EX-cli-013）と同じ振る舞い。整理前の変異テストでは print_status_result を何もしない変異をこのテストでも検知していた（今は status_output のテストも検知しうる） |
| tests/cli_status.rs | test_status_empty_tree_both_sides | status_output の exit_code_is_zero_when_every_file_is_equal と重なる。両側が空のときにファイルの行が出ないことは IR にない |
| tests/cli_status.rs | test_status_json_special_chars_in_path | 空白を含むパスの JSON。パスの文字の扱いは IR にない |

### 判断の結果

利用者は A の 9 件と C の 3 件（計 12 件）を消し、B の 87 件は全て残すと決めた。

- A と C: 実装詳細をなぞるだけのテスト、作らないと決めた hunks の表示、入口を通す根拠テストと重なる tests/ 直下のテストで、消しても確かめる振る舞いは減らない。
- B: 公開された入口を通す根拠テストと振る舞いは重なるが、失敗の場所がすぐ分かる速い単体テストで、入口から作れない場合（未読み込みのディレクトリ、片側だけのファイルを比較の対象に入れないこと）も持つため残す。バックアップと rollback の整理で純粋関数の単体テストを残した判断と同じ。

決まった 12 件だけを消し、消した後に `cargo nextest run --all-features` が通ることを確かめた（2,847 件）。

## 整理後の変異テスト

削除を終えたコミット c4c429c で、整理前と同じコマンドを一回実行した。実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/service/status.rs src/cli/status.rs src/cli/ref_guard.rs src/service/source_pair.rs src/tree.rs
```

全体の集計は `mutants: caught=182 survived=16 timeout=0 unviable=34 equivalent=0`（232 件、実行時間は約 47 分）。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/service/status.rs | 100 | 5 | 0 | 9 |
| src/cli/status.rs | 15 | 5 | 0 | 2 |
| src/cli/ref_guard.rs | 3 | 0 | 0 | 1 |
| src/service/source_pair.rs | 11 | 0 | 0 | 7 |
| src/tree.rs | 53 | 6 | 0 | 15 |

### 整理前との比較

整理後の見逃し 16 件のうち 15 件は整理前の見逃しに含まれる。
残る一件の src/cli/status.rs:125 の `delete ! in execute_status` は、整理前は agent_ssh_deploy のテストだけで検知されていた変異（上の「変異と関係のないテストだけによる検知」の一件目）で、削除したテストによる検知ではない。
この変異は機密ファイルでないファイルの中身を取得する条件の反転で、三者比較のときに効く。整理前の見かけの検知が今回はなかったため見逃しに出た。削除で増えた見逃しはないため、戻した削除はない。

整理前の見逃しのうち次の五件は整理後に検知された。

- src/cli/status.rs:170 の `delete ! in execute_status`。ただし tui_merge のテストだけによる検知のため、下で決着させる
- src/cli/status.rs:198 の `replace > with ==` と `replace > with >=`（-v の判定。FLAG-cli-001 の範囲）
- src/service/status.rs:560 の `replace > with < in status_exit_code`（右だけのファイルだけのときの終了コード 1 を新しい根拠テストが確かめる）
- src/tree.rs:284 の `replace += with *= in ensure_path_in_nodes`（tui_merge のテストだけによる検知。決着の対象外）

cargo-mutants は nextest の最初の失敗で止めるため、変異ごとのログに残る失敗したテストは最初に落ちたものだけである。「変異と関係のないテストだけによる検知」は、その変異を関係のあるテストが検知できるかどうかまでは示さない。

### 見逃しの決着

決着の対象の見逃し（status の入口から呼ばれる関数のもの）と、整理後に tui_merge のテストだけで検知された :170 を一件ずつ決着させた。
テストを足したものは、コミットに含めない一時的な書き換えでその変異を入れ、足したテストが落ちることを確かめた。

| 位置 | 変異 | 決着 |
|---|---|---|
| src/service/status.rs:249 | replace \|\| with && in needs_content_compare | テストを足した。status_judgement の a_regular_file_against_a_symlink_to_the_same_content_is_modified（EX-cli-062）の symlink を、例の文どおり通常ファイルと同じサイズ（リンク先の文字列を 10 バイト）にした。変異では片側だけの symlink の組の中身を読み比べて "equal" になり、このテストが落ちる |
| src/service/status.rs:96 | replace && with \|\| in TreeIndex<'a>::record_node | テストを足した。status_judgement の one_sided_files_are_left_or_right_only_and_a_file_against_a_directory_is_modified に、左がファイルへの symlink・右が同じ名前のディレクトリの組を加え、ディレクトリの中のファイルが "right_only" であることを確かめる。変異では symlink が未読み込みのディレクトリとして扱われ、中のファイルが "modified" になって落ちる |
| src/cli/status.rs:125 | delete ! in execute_status | テストを足した。status_targets の三者比較の構成に三つとも同じ中身の all_same.txt を加えた。変異では機密ファイルでないファイルの中身を読まず、all_same.txt が参照先と違うと数えられて "ref_differs" が 5 になり、json_marks_each_file_against_the_ref_and_counts_the_marks と text_names_the_ref_in_the_header_marks_files_and_adds_a_ref_line_after_the_summary が落ちる |
| src/cli/status.rs:170 | delete ! in execute_status | 同上。変異では参照先の機密ファイルでないファイルの中身を読まず、all_same.txt が違うと数えられて同じ二件が落ちる |
| src/cli/status.rs:106 | replace && with \|\| in execute_status | FLAG-cli-005 の範囲として記録する。変異は --ref があってもハッシュの経路で比べるようにし、違いが出るのは --ref を指定したときの機密ファイルの判定だけだった。コミットに含めない一時的なテストで、--ref 付きで、サイズが同じで更新時刻が違い中身が同じ機密ファイルが、元のコードでは "modified"（FLAG-cli-005 の挙動）、変異では "equal" になることを確かめた。機密でないファイルと symlink の判定は変わらなかった |
| src/cli/status.rs:106 | delete ! in execute_status | FLAG-cli-003 の範囲として記録する。変異は右がサーバで --ref がないときもハッシュの経路を使わず中身を読む経路で比べるようにし、違いが出るのは --checksum での symlink の組の判定だけだった。同じ一時的なテストで、リンク先の文字列が同じでリンク先の中身が違う symlink の組が、--checksum 付きで元のコードでは "equal"、変異では "modified" になることを確かめた。--checksum なしと --ref 付きの判定は変わらなかった |
| src/cli/status.rs:198 | replace > with < in execute_status | FLAG-cli-001 の範囲として記録する。変異は -v を指定しても agent の状態を出さないようにするもので、agent の出し方と出す条件は FLAG-cli-001 で未決のため、根拠テストで確かめない |
| src/cli/status.rs:146 | replace && with \|\| in execute_status | 同等変異として .kotowari/mutants-equivalents.yaml に登録した。ハッシュで判定が済むのは --ref がないときだけで、そのとき中身は一つも取得されないため、変異で中身の比較を呼んでも空の組で何も変わらない。別の文脈のエージェントに、status の公開された入口（execute_status と SSH fixture の実行ファイル、agent の有無、--checksum・--ref・フィルター・読めないファイルの組み合わせ）からこの変異で落ちるテストを書かせたが、出力が全て元のコードと一致し書けなかった |
| src/service/status.rs:137 | replace path_is_within_unloaded_dir -> bool with true | 同等変異として登録した。この関数は未読み込みのディレクトリがあるときだけ呼ばれるが、status のツリーは再帰の走査からしか作られず、ローカル・SSH・agent のどの経路でもディレクトリの子の一覧は読み込まれる。同じエージェントに、空ディレクトリ・ディレクトリへのリンク・切れたリンク・除外や include・走査件数の上限・読めないディレクトリを含む構成で落ちるテストを書かせたが書けなかった |

同等変異を登録した後に整理後の結果を読み直すと `mutants: caught=182 survived=14 timeout=0 unviable=34 equivalent=2` になる（`kotowari mutants --tool cargo-mutants --format text` を同じ outcomes.json に対して実行）。
テストを足した四件は、足した後に変異テストを回し直してはいない。一件ずつ一時的な書き換えで検知を確かめた。

決着の対象でない見逃し（整理前から変わらない）:

- src/service/status.rs:320・323 の `replace && with || in needs_merge_content_compare`（merge の比較対象）
- src/tree.rs:202 の `replace FileTree::sort with ()`、:284 の `replace += with -= in ensure_path_in_nodes`、:285 の `replace - with + in ensure_path_in_nodes` と `replace - with / in ensure_path_in_nodes`、:302 の `replace > with >= in ensure_path_in_nodes`、:324 の `replace > with >= in ensure_path_in_btree_node`（ツリーの構造の操作）

新しい FLAG の候補は見つからなかった。

## 要件の verification の見直し

status の要件の verification は全て unit で、いずれも具体的な場面の入力で結果が決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-cli-006 | unit | 等しいファイルと差分のあるファイルを置いた場面で一覧の中身が決まる |
| REQ-cli-007 | unit | --all の有無の場面で等しいファイルが一覧に入るかが決まる |
| REQ-cli-008 | unit | メタデータが同じで中身が違う場面で --checksum の判定が決まる |
| REQ-cli-027 | unit | TBL-cli-001 の各行が具体的な左右のファイルの組で決まる |
| REQ-cli-028 | unit | symlink の組のリンク先の文字列で判定が決まる |
| REQ-cli-029 | unit | 置いたファイルの組に対して集計の数が決まる |
| REQ-cli-030 | unit | 差分の有無とエラーの場面で終了コードが決まる |
| REQ-cli-031 | unit | 形式の指定と置いたファイルに対して出力の行が決まる |
| REQ-cli-032 | unit | 置いたファイルに対して JSON の項目と値が決まる |
| REQ-cli-033 | unit | --summary の場面で出力に含まれるものが決まる |
| REQ-cli-034 | unit | --left・--right と設定のサーバの組で左右かエラーが決まる |
| REQ-cli-035 | unit | 左右と参照先のファイルの組で印と集計が決まる |
| REQ-cli-036 | unit | 機密ファイルと参照先の有無の組で印が決まる |
| REQ-cli-037 | unit | 参照先が左右と同じ場面で警告と三者比較の有無が決まる |
