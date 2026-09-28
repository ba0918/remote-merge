# merge の指定・確認・出力のテスト整理の記録

merge の指定・確認・出力の取り込みで加えた要件（REQ-cli-046 から REQ-cli-051）と、merge 側の根拠を足す REQ-cli-018 の根拠テストを整理した過程の記録。
変異テストの結果、各要件の根拠にしたテスト、削除候補と利用者の判断、整理後の見逃しの決着を残す。

## 整理前の変異テスト

整理を始める前のコミット 7209529（作業ツリーに変更のない状態）で、次のコマンドを一度だけ実行した。
三つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/cli/merge.rs src/service/merge.rs src/cli/ref_guard.rs
```

全体の集計は `mutants: caught=57 survived=6 timeout=0 unviable=10 equivalent=0`（73 件、実行時間は約 17 分）。
スクリプトの終了コードは 1 だった。これは kotowari mutants が見逃しを error として報告したためで、メモリ上限による停止ではない（cargo-mutants は完了し、結果が読まれている）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/cli/merge.rs | 32 | 3 | 0 | 5 |
| src/service/merge.rs | 22 | 3 | 0 | 4 |
| src/cli/ref_guard.rs | 3 | 0 | 0 | 1 |

この計画の変異テストは TBL-cli-008 の全ての行を裏付けない。
--left と --right が同じ行と設定にないサーバ名の行は `src/service/source_pair.rs`（sync の整理で変異テストの対象にした）が、--format の行は `src/service/output.rs` の `OutputFormat::parse` が決めており、どちらのファイルもこの計画の変異テストの対象外である。
merge のテキストと JSON の整形（`src/service/output.rs`）も対象外で、実行ファイルと `format_json` を通す根拠テストで確かめる。

### 見逃し

位置は変異が入る行と列、変異は kotowari mutants の出力の文言のまま。
「決着の対象」は計画の区別による。merge の入口の規則を持つ関数（src/cli/merge.rs の `validate_merge_args`（--hunks の分岐を除く）・`run_merge`・`execute_merge`・`print_merge_result`、src/service/merge.rs の `plan_merge`・`build_merge_output`・`merge_exit_code`・`has_three_way_conflict`・`check_r2r_guard`、src/cli/ref_guard.rs の `validate_ref_side`）の見逃しを対象にする。
ただし `execute_merge` のうち、左右の比較から削除計画まで（`fetch_trees_and_statuses_for_merge` の呼び出しから `skip_symlink_deletions` の呼び出しまで）と、ファイルごとの書き込みと削除の実行（`execute_single_merge` と `execute_deletions` の呼び出し）は merge の後の回で扱うため記録だけする。
機密ファイルの件数の通知（FLAG-cli-022）、三者の中身がそろわないときの分岐（FLAG-cli-023）、dry-run で競合を確かめない条件（FLAG-cli-024）に入った見逃しは、その FLAG の範囲として記録する。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/cli/merge.rs:246:32 | replace && with \|\| in execute_merge | FLAG-cli-022 の範囲（機密ファイルの件数の通知の条件） |
| src/cli/merge.rs:246:47 | replace && with \|\| in execute_merge | FLAG-cli-022 の範囲（同上） |
| src/cli/merge.rs:316:62 | delete ! in execute_merge | 対象（全てのファイルが競合で外れたときの早期の戻り） |
| src/service/merge.rs:69:22 | replace \|\| with && in has_three_way_conflict | 対象 |
| src/service/merge.rs:69:38 | replace \|\| with && in has_three_way_conflict | 対象 |
| src/service/merge.rs:77:9 | delete match arm (Ok(base), Ok(left), Ok(right)) in has_three_way_conflict | 対象 |

### 変異と関係のないテストだけによる検知

検知した 57 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗で止まる（例: src/cli/merge.rs:120 の変異では 2862 件中 2464 件で止まった）ため、集めた名前は検知したテストの全てではなく、先に失敗したものである。

負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）で検知された変異はなかった。
変異した関数を merge の入口の外から使うテストだけで検知された変異は次の四件で、いずれも変異した関数を実際に通るテストによる検知のため、見かけの検知ではない。

| 位置 | 変異 | 失敗したテスト | 関係 |
|---|---|---|---|
| src/cli/merge.rs:120:8 | delete ! in execute_merge | contract の backup_cleanup::cleanup_removes_expired_history_of_a_configured_server | 期限切れのバックアップの片付けを merge で行う |
| src/service/merge.rs:29:19 | replace && with \|\| in plan_merge | src/cli/sync.rs の build_dry_run_targets_includes_would_merge | sync も `plan_merge` を使う |
| src/service/merge.rs:86:5 | replace find_symlink_target -> Option<String> with Some("xyzzy".into()) | src/handler/merge_exec.rs の single_file_merge_stores_backup_outside_both_targets | TUI の書き込みも `find_symlink_target` を使う |
| src/service/merge.rs:86:5 | replace find_symlink_target -> Option<String> with Some(String::new()) | 同上 | 同上 |

ほかに、src/ の中の単体テストだけで検知された変異があり、整理で消すテストに頼っていないかを整理後の比較で注意する。

- `validate_merge_args` の三件（`Ok(())` にする変異、:69 の `||` を `&&`、:76 の `!=` を `==`）: src/cli/merge.rs の test_empty_paths_returns_error、test_merge_with_only_left_returns_error、test_merge_with_only_right_returns_error と --hunks のテスト
- `run_merge` を `Ok(0)`・`Ok(1)`・`Ok(-1)` にする三件: src/cli/merge.rs の test_run_merge_rejects_invalid_format_early
- `validate_ref_side` の三件: src/cli/ref_guard.rs の四件のうち三件
- `plan_merge` の :29:12 の `delete !`、`merge_exit_code` の三件、`check_r2r_guard` の五件: src/service/merge.rs の test_plan_merge_*、test_merge_exit_code_*、test_check_r2r_guard_*
- `merge_partial_nodes`・`merge_partial_node`・`merge_file_node` と `determine_merge_action`・`find_symlink_target` の検知: 計画で記録だけする関数の単体テスト
