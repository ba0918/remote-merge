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
