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
