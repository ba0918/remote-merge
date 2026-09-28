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
| src/service/merge.rs:77:9 | delete match arm (Ok(base), Ok(left), Ok(right)) in has_three_way_conflict | 記録だけ（参照先との三者の衝突の判定。merge の指定・確認・出力の回で FLAG-cli-025 の範囲として決着済み。[決定記録 A1](../decision/records/2026-09-28-merge-cli-mutant-flags.md#A1)） |
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

## 要件ごとの根拠テスト

根拠テストは `tests/contract/merge_links.rs` にある。手本にした元のテストは消さず、書き換えていない。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。

準備は `tests/contract/merge_support.rs` の `Fixture` を使い、書き込み先 develop を `RuntimeTargets::with_local` で一時ディレクトリに差し替えて `execute_merge` と `execute_sync` を関数呼び出しで呼ぶ。
結果は --format json と同じ `format_json` で JSON にして確かめる。
symlink を作る補助 `Fixture::symlink`（`std::os::unix::fs::symlink` を使う）を `Fixture` に足した。
どのテストも終了コードと標準エラーは確かめない。
FLAG-merge-008 から 014 と FLAG-cli-016 から 026 の挙動（スキップ理由の文言のうち要件にないもの、dry-run、パス脱出、エージェントの経路、削除の直前の調査と失敗、スキップだけのときの終了コード、機密ファイルの通知）は確かめない。

### symlink の作成と削除しないファイルのスキップ（REQ-merge-023、REQ-merge-024、REQ-merge-025）

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-merge-023（merge） | merge_creates_a_missing_destination_symlink_without_following_it | src/service/merge.rs の test_determine_merge_action_source_symlink_target_not_exists（ツリーの上で CreateSymlink の判定になることだけを見る）。local に target.txt とそれを指す link.txt を置き、引数に link.txt だけを渡した merge で、merged に link.txt が出て、develop/link.txt が symlink になり `fs::read_link` が "target.txt" で、develop/target.txt が作られない（`fs::symlink_metadata` が失敗する）ことを確かめる |
| REQ-merge-023（sync） | sync_creates_a_missing_destination_symlink_without_following_it | 同じ元のテスト。同じ構成を --force の sync で確かめる |
| REQ-merge-024（merge） | merge_without_delete_skips_a_destination_only_file | src/service/path_resolver.rs の filter_merge_candidates_excludes_equal_and_right_only と tests/contract/merge_paths.rs の sync_without_delete_keeps_destination_only_regular_files（sync で deleted が空でファイルが残ることだけを見る）。develop にだけ only-here.txt を置き、引数 "." の --delete のない merge で、ファイルの中身が変わらず、skipped の only-here.txt の reason がちょうど "right-only file (use --delete to remove)" の一件であることを確かめる |
| REQ-merge-024（sync） | sync_without_delete_skips_a_destination_only_file | 同じ元のテスト。同じ構成を --force の sync で確かめる |
| REQ-merge-025（merge） | merge_delete_without_force_keeps_a_destination_only_sensitive_file | src/service/sync.rs の plan_deletions_sensitive_skipped_without_force。develop にだけ .env を置き、引数 "." の --delete（--force なし）の merge で、.env の中身が変わらず、skipped の .env の reason がちょうど "sensitive file (use --force to include)" の一件で、deleted が空であることを確かめる |
| REQ-merge-025（sync） | sync_delete_without_force_keeps_a_destination_only_sensitive_file | 同じ元のテスト。同じ構成を --delete・--force なし・--dry-run なしの sync で確かめる |

- REQ-merge-023 は引数にリンクのパスだけを渡す（"." を渡すと local の target.txt も読み込み元にだけあるファイルとして正当に書き込まれ、リンクの先へ辿らないことを確かめられなくなるため）。
- REQ-merge-025 の sync は、書き込み先にだけある .env の他に書き込むものも削除するものもない構成にした。書き込む予定がないため `execute_sync` は確認のプロンプトの前に戻り、標準入力を読まない（読んでいれば取り消しの結果になり、`Fixture::sync_json` が落ちる）。
- .env が機密ファイルであることは設定の既定の機密ファイルのパターンによる（fixture の設定は filter を書かない）。

### 削除の結果の JSON とテキスト（REQ-merge-026、REQ-merge-027）

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-merge-026（バックアップが有効） | merge_reports_a_deleted_file_with_its_backup_when_backup_is_enabled | tests/contract/merge_paths.rs の a_deleted_regular_file_is_backed_up_and_recreated_by_rollback（deleted が一件で backup があることだけを見る）と sync_delete_removes_a_destination_only_regular_file。develop にだけ old/obsolete.txt を置き、引数 "." の --delete の merge で、ファイルが消え、deleted がちょうど一件で path が old/obsolete.txt、status が "ok"、backup を最初の "/" で分けたパスの部分が old/obsolete.txt で、セッションIDの部分が fixture の集約先の sessions にちょうど一つあるセッションの名前と一致することを確かめる。現在時刻とは比べない |
| REQ-merge-026（バックアップが無効） | merge_reports_a_deleted_file_without_backup_when_backup_is_disabled | 同じ元のテスト。バックアップを無効にした構成で、deleted の一件に backup の項目がないことを確かめる |
| REQ-merge-027（バックアップが有効） | merge_text_shows_a_deleted_file_with_its_backup_when_backup_is_enabled | src/service/output.rs の test_format_merge_text_with_deleted（手で組んだ結果を整形する）。--dry-run のない --delete の merge の結果を `format_merge_text` に渡し、"Deleted: " で始まる行がちょうど "Deleted: old/obsolete.txt (backup: セッションID/old/obsolete.txt)" の一行で、セッションIDが集約先のセッションの名前と一致することを確かめる |
| REQ-merge-027（バックアップが無効） | merge_text_shows_a_deleted_file_without_backup_when_backup_is_disabled | src/service/output.rs の test_format_merge_text_deleted_no_backup。同じ手順で、"Deleted: " で始まる行がちょうど "Deleted: old/obsolete.txt" の一行であることを確かめる |

- テキストは `Fixture::merge_text` で得る。`execute_merge` の結果（`MergeCommandOutput::Files`）を公開された `remote_merge::service::output::format_merge_text` に渡す。テキストと JSON の振り分けと実行ファイルの出力は REQ-cli-049 の実行ファイルのテストが確かめているため、ここでは SSH の fixture を使わない。
- 集約先のセッションの名前は `Fixture::backup_sessions`（集約先の sessions の下のディレクトリ名）で得る。集約先は `RuntimeTargets::with_backup_store` で fixture の一時ディレクトリに差し替えてあり、セッションがその中にちょうど一つできることを確かめるため、集約先がテストの外に書かれていないことも同時に分かる。
- 削除したファイルを配下のディレクトリ old/ に置き、backup の "セッションID/パス" のパスの部分が "/" を含むパスのまま続くことを確かめる。

## 整理後の変異テスト

整理の最後のコミット e8f8dce（テストの最後のコミットは 112226b。その後のコミットはスクリプトと計画だけで、src/ とテストは変えていない）で、決着の対象の関数に絞って次のコマンドを一回実行した。実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh --re '(find_symlink_target|determine_merge_action|plan_deletions|skip_symlink_deletions|check_path_traversal|filter_merge_candidates|execute_deletions|execute_single_merge)' src/service/merge.rs src/service/sync.rs src/service/path_resolver.rs src/service/merge_flow.rs
```

全体の集計は `mutants: caught=32 survived=7 timeout=0 unviable=18 equivalent=0`（57 件、実行時間は約 14 分）。
スクリプトの終了コードは 1（見逃しの error による。メモリ上限での停止ではない）。
57 件には、正規表現に名前の一致しない src/service/merge_flow.rs の `execute_hunk_merge` の変異 9 件（:427:13、:428:13、:479:21、:480:21、:481:21、:502:17、:503:17、:504:17、:505:17）が含まれていた。含まれた理由は確かめていない。この 9 件は決着の対象の関数ではないため、比較から外して下に記録だけする。
ファイルごとの内訳（`outcomes.json` から数え、`execute_hunk_merge` の 9 件を除いたもの）は次のとおり。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/service/merge.rs | 6 | 0 | 0 | 1 |
| src/service/sync.rs | 10 | 0 | 0 | 6 |
| src/service/path_resolver.rs | 8 | 0 | 0 | 3 |
| src/service/merge_flow.rs | 6 | 0 | 0 | 8 |

### 整理前との比較

整理前の実行のうち同じ関数の変異は 48 件で、caught 30、survived 0、unviable 18 だった（ファイルごとの内訳も上の表と同じ）。
整理後の同じ関数の 48 件も caught 30、survived 0、unviable 18 で、見逃しはない。見逃しの集合は空で、整理前の見逃しの部分集合になっている。

整理後の見逃しの判定には、変異と関係のないテストだけによる検知を見逃しに数え戻す（検知した 32 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めて確かめた）。
負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）だけで検知された変異はなかった。
この回で足したテスト（tests/contract/merge_links.rs）が先に失敗したテストに名前の出た変異はなかった。決着の対象の関数の変異は、整理前から単体テストと既存の契約テストに検知されている。
この計画はテストを消していないため、見逃しが増えないことは予想どおりだった。

### 見逃しの決着

決着の対象の見逃しは整理前も整理後もなく、決着させるものはない。新しい FLAG の候補も同等変異の登録もない。

決着の対象でない見逃しは次の 11 件で、記録だけする。整理後の実行で回した `execute_hunk_merge` の 7 件は整理後も見逃しで、:428:13 は整理後は cargo-mutants の見逃しだった（整理前は tui_merge だけによる見かけの検知）。他の 4 件は整理後の実行の対象外で、整理前の記録のまま扱う。

| 位置 | 変異 | 扱う回 |
|---|---|---|
| src/service/merge.rs:77:9 | delete match arm (Ok(base), Ok(left), Ok(right)) in has_three_way_conflict | merge の指定・確認・出力の回で FLAG-cli-025 の範囲として決着済み（参照先との三者の衝突の判定。[決定記録 A1](../decision/records/2026-09-28-merge-cli-mutant-flags.md#A1)） |
| src/service/merge_flow.rs:256:14 | replace > with >= in copy_permissions | 書き込みの中身の回で FLAG-merge-007 の範囲として決着済み |
| src/service/merge_flow.rs:256:18 | replace && with \|\| in copy_permissions | 同上 |
| src/service/merge_flow.rs:413:33 | replace \|\| with && in execute_hunk_merge | 変更のまとまりを選ぶマージの回（整理前は見かけの検知） |
| src/service/merge_flow.rs:427:13 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:428:13 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:479:21 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:480:21 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:481:21 | delete field hunk_info from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:502:17 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:503:17 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 同上 |

## 要件の verification の見直し

REQ-merge-023 から 027 の verification は全て unit で、いずれも具体的な場面の入力で結果が決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-merge-023 | unit | 読み込み元が symlink で書き込み先にないという場面で、作るものと辿らないことが決まる |
| REQ-merge-024 | unit | --delete のない実行と書き込み先だけのファイルという場面で、変えないことと skipped の理由が決まる |
| REQ-merge-025 | unit | --delete ありで --force なしと機密ファイルという場面で、削除しないことと skipped の理由が決まる |
| REQ-merge-026 | unit | バックアップの有効・無効の場面ごとに deleted の項目が決まる |
| REQ-merge-027 | unit | バックアップの有無の場面ごとに削除の行の形が決まる |
