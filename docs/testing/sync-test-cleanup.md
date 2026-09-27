# sync のテスト整理の記録

sync の取り込みで加えた要件（REQ-cli-038 から REQ-cli-045）の根拠テストを整理した過程の記録。
変異テストの結果、各要件の根拠にしたテスト、削除候補と利用者の判断、整理後の見逃しの決着を残す。

## 整理前の変異テスト

整理を始める前のコミット 3b92547（作業ツリーに変更のない状態）で、次のコマンドを一度だけ実行した。
三つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/service/sync.rs src/cli/sync.rs src/service/source_pair.rs
```

全体の集計は `mutants: caught=43 survived=20 timeout=0 unviable=17 equivalent=0`（80 件、実行時間は約 22 分）。
スクリプトの終了コードは 1 だった。これは kotowari mutants が見逃しを error として報告したためで、メモリ上限による停止ではない（cargo-mutants は完了し、結果が読まれている）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/service/sync.rs | 16 | 1 | 0 | 8 |
| src/cli/sync.rs | 16 | 19 | 0 | 2 |
| src/service/source_pair.rs | 11 | 0 | 0 | 7 |

### 見逃し

位置は変異が入る行、変異は kotowari mutants の出力の文言のまま。同じ行に同じ文言が二つあるものは、行の中の別の `>`（列 24 と列 44）への変異である。
「決着の対象」は計画の区別による。sync の入口の規則を持つ関数（src/service/sync.rs の `compute_target_status`・`compute_sync_summary`・`sync_exit_code`、src/service/source_pair.rs の `resolve_source_pairs`、src/cli/sync.rs の `run_sync`・`print_sync_result`・`validate_sync_args`・`build_dry_run_targets`・`print_sync_plan`・`execute_sync`）の見逃しを対象にする。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/cli/sync.rs:87 | replace run_sync -> anyhow::Result<i32> with Ok(0) | 対象 |
| src/cli/sync.rs:87 | replace run_sync -> anyhow::Result<i32> with Ok(-1) | 対象 |
| src/cli/sync.rs:298 | delete ! in execute_sync | 対象（確認の答えの判定） |
| src/cli/sync.rs:416 | replace print_sync_result -> anyhow::Result<()> with Ok(()) | 対象（テキストの分岐は FLAG-cli-012 の範囲） |
| src/cli/sync.rs:536 | replace fetch_partial_tree -> anyhow::Result<FileTree> with Ok(Default::default()) | 対象外（走査の取り方。計画の対象の関数にない） |
| src/cli/sync.rs:548 | replace print_sync_plan with () | 対象 |
| src/cli/sync.rs:557 | replace \|\| with && in print_sync_plan | 対象 |
| src/cli/sync.rs:557 | replace > with < in print_sync_plan（列 24） | 対象 |
| src/cli/sync.rs:557 | replace > with == in print_sync_plan（列 24） | 対象 |
| src/cli/sync.rs:557 | replace > with >= in print_sync_plan（列 24） | 対象 |
| src/cli/sync.rs:557 | replace > with < in print_sync_plan（列 44） | 対象 |
| src/cli/sync.rs:557 | replace > with == in print_sync_plan（列 44） | 対象 |
| src/cli/sync.rs:557 | replace > with >= in print_sync_plan（列 44） | 対象 |
| src/cli/sync.rs:559 | replace > with < in print_sync_plan | 対象 |
| src/cli/sync.rs:559 | replace > with == in print_sync_plan | 対象 |
| src/cli/sync.rs:559 | replace > with >= in print_sync_plan | 対象 |
| src/cli/sync.rs:562 | replace > with < in print_sync_plan | 対象 |
| src/cli/sync.rs:562 | replace > with == in print_sync_plan | 対象 |
| src/cli/sync.rs:562 | replace > with >= in print_sync_plan | 対象 |
| src/service/sync.rs:57 | replace == with != in compute_sync_summary | 対象 |

### 変異と関係のないテストだけによる検知

検知した 43 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
変異した関数と関係がなく、負荷の下で落ちることのあるテストだけで検知された変異は次の三件。

| 位置 | 変異 | 失敗したテスト |
|---|---|---|
| src/cli/sync.rs:87 | replace run_sync -> anyhow::Result<i32> with Ok(1) | tui_merge の test_merge_cancel_with_n |
| src/cli/sync.rs:219 | delete ! in execute_sync（列 28） | agent_ssh_deploy の agent_ssh_read_files_roundtrip |
| src/cli/sync.rs:219 | replace == with != in execute_sync（列 80） | tui_merge の test_hunk_merge_left_to_right_with_l、test_hunk_merge_right_to_left_with_h_key |

一件目は `run_sync`（実行ファイルの sync だけが呼ぶ）の戻り値の変異で、TUI のテストとは関係がない。見かけの検知と判断した。
二件目と三件目は :219 の、中身の読み比べに失敗したファイルを書き込み予定から外す条件で、`execute_sync` のうち merge と共通の区間（`compute_status_from_trees` から `plan_deletions` と `skip_symlink_deletions` の呼び出しまで）にあるため、決着の対象ではない。
整理後の比較では、この三件は検知と見逃しのどちらにもなりうるものとして扱う。

ほかに、src/ の中の単体テストだけで検知された変異があり、整理で移すか消すテストに頼っているため整理後の比較で注意する。

- `validate_sync_args` を `Ok(())` にする変異: src/cli/sync.rs の validate_missing_left、validate_empty_right
- `build_dry_run_targets` を `vec![]` にする変異: src/cli/sync.rs の build_dry_run_targets_includes_would_merge、build_dry_run_targets_includes_connection_failures
- `compute_target_status` の `delete !` 二件と `sync_exit_code` の四件: src/service/sync.rs の compute_target_status_* と sync_exit_code_*
- src/service/source_pair.rs の検知 11 件: 全て source_pair の単体テスト（resolve_source_pairs_* と status 用の関数のテスト）
