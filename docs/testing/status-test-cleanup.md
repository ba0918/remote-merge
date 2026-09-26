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
