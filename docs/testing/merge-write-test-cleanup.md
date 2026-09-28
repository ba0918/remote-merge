# merge の書き込みの中身のテスト整理の記録

merge の書き込みの中身の取り込みで加えた要件（REQ-merge-019 から REQ-merge-022）の根拠テストを整えた過程の記録。
変異テストの結果、各要件の根拠にしたテスト、整理後の見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テストは全て既存要件の根拠だったため、この整理ではテストを消していない。

## 整理前の変異テスト

整理を始める前のコミット bb37889（作業ツリーに変更のない状態）で、次のコマンドを一度だけ実行した。
三つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/service/merge_flow.rs src/cli/tolerant_io.rs src/service/status.rs
```

全体の集計は `mutants: caught=133 survived=13 timeout=0 unviable=17 equivalent=0`（163 件、実行時間は約 37 分）。
スクリプトの終了コードは 1 だった。これは kotowari mutants が見逃しを error として報告したためで、メモリ上限による停止ではない（cargo-mutants は完了し、結果が読まれている）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/service/merge_flow.rs | 23 | 10 | 0 | 8 |
| src/cli/tolerant_io.rs | 8 | 0 | 0 | 0 |
| src/service/status.rs | 102 | 3 | 0 | 9 |

### 見逃し

位置は変異が入る行と列、変異は kotowari mutants の出力の文言のまま。
「決着の対象」は計画の区別による。src/service/merge_flow.rs の `execute_single_merge` の通常ファイルの経路と `copy_permissions`、src/cli/tolerant_io.rs の `fetch_contents_required`、src/service/status.rs の `verified_content_pairs`・`needs_explicit_file_compare`・`needs_merge_content_compare`・`refine_status_with_content` の見逃しを対象にする。
src/service/status.rs の他の関数、`fetch_contents_tolerant`、symlink の分岐・`check_source_exists`・`execute_deletions`・`validate_hunk_merge_target`・`execute_hunk_merge` の見逃しは記録だけする。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/service/merge_flow.rs:256:14 | replace > with >= in copy_permissions | 対象（読み込み元の権限が 0 のときに権限を付けるか） |
| src/service/merge_flow.rs:256:18 | replace && with \|\| in copy_permissions | 対象（同上） |
| src/service/merge_flow.rs:413:33 | replace \|\| with && in execute_hunk_merge | 記録だけ（変更のまとまりを選ぶマージの回で扱う） |
| src/service/merge_flow.rs:427:13 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:428:13 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:479:21 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:480:21 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:481:21 | delete field hunk_info from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:502:17 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:503:17 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/status.rs:143:37 | replace + with * in collect_all_file_paths | 記録だけ（status の規則。容量の見積もりだけに使う式） |
| src/service/status.rs:320:15 | replace && with \|\| in needs_merge_content_compare | 対象（--checksum のときに中身を読み比べる組の条件） |
| src/service/status.rs:323:61 | replace && with \|\| in needs_merge_content_compare | 対象（同上） |

`copy_permissions` の二件は、読み込み元がリモートの場合（FLAG-merge-002）、新規ファイルの権限（FLAG-merge-003）、権限の変更の失敗（FLAG-merge-004）のどれにも当たらない（読み込み元の権限の値が 0 のときだけ違いが出る）。
`fetch_contents_required` と `verified_content_pairs`・`needs_explicit_file_compare`・`refine_status_with_content` には見逃しがなかった。

### 変異と関係のないテストだけによる検知

検知した 133 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗から少し進んで止まるため、集めた名前は検知したテストの全てではなく、先に失敗したものである。

負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）だけで検知された変異は次の一件だった。

| 位置 | 変異 | 失敗したテスト | 扱い |
|---|---|---|---|
| src/service/status.rs:137:5 | replace path_is_within_unloaded_dir -> bool with true | tui_merge の test_hunk_merge_left_to_right_with_l、test_hunk_merge_right_to_left_with_h_key、test_merge_cancel_with_n、test_sensitive_file_merge_requires_confirmation | status の規則で決着の対象ではない。整理後の比較では、この変異を見逃しの候補に含めて比べる |

この一件は決着の対象の関数ではないため、見かけの検知かどうかは確かめていない。
