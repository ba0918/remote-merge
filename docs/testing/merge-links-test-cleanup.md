# merge の symlink と削除のテスト整理の記録

merge の symlink と削除の取り込みで加えた要件（REQ-merge-023 から REQ-merge-027）の根拠テストを整えた過程の記録。
変異テストの結果、各要件の根拠にしたテスト、整理後の見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テストに実装詳細をなぞるだけのものはなかったため、この整理ではテストを消していない。

## 整理前の変異テスト

整理を始める前のコミット 42aecc7（作業ツリーに変更のない状態）で、次のコマンドを一度だけ実行した。
四つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/service/merge.rs src/service/sync.rs src/service/path_resolver.rs src/service/merge_flow.rs
```

全体の集計は `mutants: caught=112 survived=9 timeout=0 unviable=23 equivalent=0`（144 件、実行時間は約 36 分）。
スクリプトの終了コードは 1 だった。これは kotowari mutants が見逃しを error として報告したためで、メモリ上限による停止ではない（cargo-mutants は完了し、結果が読まれている）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/service/merge.rs | 24 | 1 | 0 | 4 |
| src/service/sync.rs | 17 | 0 | 0 | 8 |
| src/service/path_resolver.rs | 46 | 0 | 0 | 3 |
| src/service/merge_flow.rs | 25 | 8 | 0 | 8 |

### 見逃し

位置は変異が入る行と列、変異は kotowari mutants の出力の文言のまま。
「決着の対象」は計画の区別による。src/service/merge.rs の `find_symlink_target`・`determine_merge_action`、src/service/sync.rs の `plan_deletions`・`skip_symlink_deletions`、src/service/path_resolver.rs の `check_path_traversal`・`filter_merge_candidates`、src/service/merge_flow.rs の `execute_single_merge` の symlink の分岐（`SkipDifferentKind`・`CreateSymlink`・`ReplaceSymlinkWithFile`）と `execute_deletions` の見逃しを対象にする。
他の関数の見逃しは記録だけする。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/service/merge.rs:77:9 | delete match arm (Ok(base), Ok(left), Ok(right)) in has_three_way_conflict | 記録だけ（参照先との三者の衝突の判定。merge の指定・確認・出力の規則） |
| src/service/merge_flow.rs:256:14 | replace > with >= in copy_permissions | 記録だけ（書き込みの中身の回で FLAG-merge-007 の範囲として決着済み） |
| src/service/merge_flow.rs:256:18 | replace && with \|\| in copy_permissions | 記録だけ（同上） |
| src/service/merge_flow.rs:427:13 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（変更のまとまりを選ぶマージの回で扱う） |
| src/service/merge_flow.rs:479:21 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:480:21 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:481:21 | delete field hunk_info from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:502:17 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:503:17 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |

決着の対象の関数には見逃しがなかった。

### 変異と関係のないテストだけによる検知

検知した 112 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗から少し進んで止まるため、集めた名前は検知したテストの全てではなく、先に失敗したものである。

負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）だけで検知された変異は次の二件だった。

| 位置 | 変異 | 失敗したテスト | 扱い |
|---|---|---|---|
| src/service/merge_flow.rs:413:33 | replace \|\| with && in execute_hunk_merge | tui_merge の test_hunk_merge_left_to_right_with_l、test_hunk_merge_right_to_left_with_h_key、test_sensitive_file_merge_requires_confirmation | TUI の変更のまとまりのマージは src/handler/merge_exec.rs の別の execute_hunk_merge を呼び、この関数を通らない（書き込みの中身の回の記録による）。見かけの検知とみなし、見逃しに数える。決着の対象ではない（変更のまとまりを選ぶマージの回で扱う） |
| src/service/merge_flow.rs:428:13 | delete field status from struct MergeFileResult expression in execute_hunk_merge | tui_merge の test_hunk_merge_left_to_right_with_l | 同上。書き込みの中身の回では cargo-mutants の見逃しだった |

これを数え戻した整理前の見逃しは 11 件で、決着の対象の関数のものはない。
比較の控えとして、この実行の `target/mutants-run/mutants.out` を target/ の外の一時ディレクトリに写した（コミットしない）。
