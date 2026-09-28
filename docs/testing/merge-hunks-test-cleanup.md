# merge の変更のまとまりを選ぶマージのテスト整理の記録

merge の変更のまとまりを選ぶマージ（--hunks）の取り込みで加えた要件（REQ-merge-028 から REQ-merge-031）の根拠テストを整えた過程の記録。
各要件の根拠にしたテスト、変異テストの結果、見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テスト 40 件に実装詳細をなぞるだけのものはなかったため、この整理ではテストを消していない。
テストを削除しない整理のため、整理前の変異テストは回していない（REQ-testing-012、[決定記録 A2](../decision/records/2026-09-28-mutation-scope.md#A2)）。
整理後の見逃しは、同じファイルを前に回したときの記録（[merge の指定・確認・出力](merge-cli-test-cleanup.md)、[merge の書き込みの中身](merge-write-test-cleanup.md)、[merge の symlink と削除](merge-links-test-cleanup.md) の整理後の節）と比べる。

## 要件ごとの根拠テスト

根拠テストは `tests/contract/merge_hunks.rs` にある。手本にした元のテストは消さず、書き換えていない。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。

準備は `tests/contract/merge_support.rs` の `Fixture` を使う。
接続より前に止まる指定は `Fixture::run_cli`（書き込み先を差し替えない隔離した実行ファイルの起動）で終了コードと標準エラーを確かめる。
それ以外は書き込み先 develop と参照先 staging を `RuntimeTargets::with_local` で一時ディレクトリに差し替えて `execute_merge` を関数呼び出しで呼ぶ。
`execute_merge` のエラーを受け取る補助 `Fixture::merge_error` を `merge_support.rs` に足した（`config()` と `runtime_targets()` が非公開のため同じファイルに置いた）。
書き込むテストと、書き込む前に止まることを確かめるテストは --force を付ける。CLI の --hunks が確認なしに書き込むのは FLAG-merge-016 の挙動のため、それに頼らない。--force を外すのは、--force がないことが要件の条件になっている機密ファイルの行と三者の競合のテストだけである。
FLAG-merge-015 から 020 と FLAG-cli-016 から 026 の挙動（diff の JSON との番号の対応、確認のプロンプトの有無、書き込み直前の確認、--max-entries、差分のないファイルの出力、機密ファイルのスキップ、リモート間の拒否）は確かめない。

### --hunks の指定のエラー（REQ-merge-028、TBL-merge-001）

| TBL-merge-001 の行 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| パスが一つでない | hunks_with_more_than_one_path_stops_with_exit_code_2 | src/cli/merge.rs の test_hunks_with_multiple_paths_returns_error（検証の関数を直接呼び、文言の一部だけを見る）。`run_cli(&["a.txt", "b.txt", "--hunks", "0", "--left", "local", "--right", "develop"])` が終了コード 2 で、標準エラーに "--hunks requires exactly one path (got 2)" を含む |
| --delete と併せる | hunks_with_delete_stops_with_exit_code_2 | src/cli/merge.rs の test_hunks_with_delete_returns_error。一つのパスに --hunks 0 と --delete を付けた `run_cli` が終了コード 2 で、標準エラーに "--hunks and --delete cannot be used together" を含む |
| 番号が hunk の数以上 | hunk_index_out_of_range_stops_without_writing | 手本の単体テストはない（tests/contract/merge_paths.rs の selected_hunk_changes_only_the_selected_region の二つの hunk の構成を写した）。二つの hunk のファイルに番号 2 を渡し、エラーの文言がちょうど "Hunk index 2 is out of range (total hunks: 2)" で、develop が変わらない |
| 読み込み元が symlink | hunks_on_a_source_symlink_stops_without_writing | src/service/merge_flow.rs の test_validate_hunk_merge_target_source_symlink_error（手で組んだツリーで検証の関数を呼ぶ）。local の file.txt を target.txt を指す symlink にし、エラーの文言がちょうど "Hunk merge is not supported for symlink files: 'file.txt'" で、develop が変わらない |
| 書き込み先が symlink | hunks_on_a_destination_symlink_stops_without_writing | src/service/merge_flow.rs の test_validate_hunk_merge_target_target_symlink_error。develop の file.txt を target.txt を指す symlink にし、同じ文言で、develop の file.txt が symlink のままでリンク先の中身が変わらない |
| 読み込み元がバイナリ | hunks_on_a_binary_source_stops_without_writing | 手本の単体テストはない。local の file.txt に NUL を含むバイト列を置き、エラーの文言がちょうど "Hunk merge is not supported for binary files: 'file.txt'" で、develop が変わらない |
| 書き込み先がバイナリ | hunks_on_a_destination_that_is_not_utf8_stops_without_writing | 手本の単体テストはない。develop の file.txt に NUL を含まず UTF-8 として正しくないバイト列（Latin-1 の é）を置き、同じ文言で、develop のバイト列が変わらない |
| 機密ファイルで --force がない | hunks_on_a_configured_sensitive_file_without_force_stops_without_writing | src/service/merge_flow.rs の test_validate_hunk_merge_target_sensitive_without_force_error。設定の `[filter]` の `sensitive` に "*.vault" を書いた構成で、二つの hunk の deploy.vault に --force なしで番号 0 を渡し、エラーの文言がちょうど "Sensitive file 'deploy.vault' requires --force for hunk merge" で、develop が変わらない |

- パスが一つでない行と --delete と併せる行は、引数の検証（`validate_merge_args`）で接続より前に止まるため、書き込み先を差し替えない実行ファイルの起動で観測できた。--format json は付けていない（JSON ではエラーが標準出力に出るため）。
- 接続の後に止まる行は、`execute_merge` がエラーを返すことと、その文言と、書き込み先が変わらないことで確かめる。エラーを終了コード 2 にするのは `src/main.rs` の共通の経路で、REQ-cli-046 の実行ファイルのテストが確かめている。
- symlink とバイナリは、読み込み元だけ・書き込み先だけの二通りを作った。どちらか一方だけが該当する構成にすることで、読み込み元と書き込み先の判定をつなぐ "||" を "&&" に変えた変異を落とせる。バイナリの一方を NUL でなく UTF-8 として正しくないバイト列にしたのは、`is_binary` の UTF-8 の判定を通る場合も確かめるためである。
- deploy.vault が機密ファイルであることは、設定に書いた "*.vault" だけによる（既定の機密ファイルのパターンのどれにも一致しない）。設定にパターンを書かずに同じテストを一時的に実行すると、merge はエラーを返さず終了コード 0 で書き込み、テストが落ちることを確かめた（この一時的な変更はコミットしていない）。

### 三者の競合（REQ-merge-031）

| 場合 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| 書き込む実行 | hunks_with_a_three_way_conflict_stops_without_writing | tests/contract/cli_results.rs の selected_hunk_with_three_way_conflict_does_not_overwrite_the_target（エラーか 0 でない終了コードのどちらかであることと、書き込み先が変わらないことだけを見る）。参照先 staging が "base"、local が "left change"、develop が "right change" の file.txt に、--ref staging で --force なしの --hunks 0 を渡し、エラーの文言がちょうど "three-way conflict: file.txt" で、develop と staging が変わらない |
| --dry-run | hunks_dry_run_with_a_three_way_conflict_stops_the_same_way | 同じ元のテスト。同じ構成に --dry-run を付け、同じ文言のエラーで止まり、develop が変わらない |

### --hunks の JSON とテキスト（REQ-merge-029、REQ-merge-030）

構成は、間に変わらない 12 行を挟んで先頭と末尾の行を変えたテキストのファイル file.txt 一つで（tests/contract/merge_paths.rs の selected_hunk_changes_only_the_selected_region の構成を写した）、--hunks に二つ目の変更の番号 1 だけを渡す。
番号 0 でなく 1 を渡すのは、hunks_applied の値と書き込まれた位置が既定の値や先頭の変更と見分けられるようにするためである。
二つの変更を離しておけば、`--hunks` の番号の区切りと表示用の区切りの数が合い、FLAG-merge-015 の違いに触れない。
差分のないファイルは構成に含めない（FLAG-merge-019）。

| 要件 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| REQ-merge-029（書き込み、バックアップが無効） | hunks_json_reports_the_applied_hunk_without_backup_when_backup_is_disabled | tests/contract/merge_paths.rs の selected_hunk_changes_only_the_selected_region（hunks_total と書き込み先の中身だけを見る）。merged がちょうど `[{"path": "file.txt", "status": "merged", "hunks_applied": [1], "hunks_total": 2, "direction": "left_to_right"}]` で backup の項目がなく、develop には二つ目の変更だけが入る |
| REQ-merge-029（書き込み、バックアップが有効） | hunks_json_reports_the_backup_when_backup_is_enabled | 同じ元のテスト。バックアップを有効にした構成で、merged の一件が上の項目に backup（文字列）を加えたものとちょうど等しく、develop には二つ目の変更だけが入る |
| REQ-merge-029（--dry-run） | hunks_dry_run_json_reports_would_merge_without_writing | 同じ元のテスト。バックアップを無効にした構成に --dry-run を付け、merged がちょうど status "would merge" の同じ一件で、develop が変わらない。--dry-run では backup を確かめない |
| REQ-merge-030（書き込み、バックアップが無効） | hunks_text_shows_the_applied_hunk_when_backup_is_disabled | src/service/output.rs の test_format_merge_text_hunk_merge_info（手で組んだ結果を整形し、文言の一部だけを見る）。"Merged: " か "Would merge: " で始まる行がちょうど "Merged: file.txt (hunks: 1/2)" の一行 |
| REQ-merge-030（書き込み、バックアップが有効） | hunks_text_shows_the_backup_when_backup_is_enabled | 同じ元のテスト。同じ結果の JSON の backup の値を使い、その行がちょうど "Merged: file.txt (hunks: 1/2) (backup: その backup の値)" の一行 |
| REQ-merge-030（--dry-run） | hunks_dry_run_text_shows_would_merge | 同じ元のテスト。バックアップを無効にした構成に --dry-run を付け、その行がちょうど "Would merge: file.txt (hunks: 1/2)" の一行 |

- JSON とテキストは `Fixture::merge_json_and_text` で得る。一度の `execute_merge` の結果を `format_json` と公開された `remote_merge::service::output::format_merge_text` の両方に渡す。テキストと JSON の振り分けと実行ファイルの出力は REQ-cli-049 の実行ファイルのテストが確かめている。
- JSON は merged の一件を項目の全体で比べるため、要件にない項目（ref_badge など）が出ないことも同時に確かめている。
- backup の値の形（セッションID/パス）は REQ-merge-029 と 030 が述べていないため確かめない。

## 整理後の変異テスト

テストの最後のコミット 3654888 の後の、記録だけを足したコミット acb7bda（作業ツリーに変更のない状態）で、決着の対象の関数に絞って次のコマンドを一回実行した。実行中は作業ツリーに触れていない。
この実行の後にテストは足していない。

```sh
scripts/mutants.sh --re '(validate_merge_args|run_hunk_merge|validate_hunk_merge_target|execute_hunk_merge|apply_selected_hunks_single_pass)' src/cli/merge.rs src/service/merge_flow.rs src/diff/engine.rs
```

全体の集計は `mutants: caught=31 survived=3 timeout=0 unviable=1 equivalent=0`（35 件、実行時間は約 11 分）。
スクリプトの終了コードは 1（kotowari mutants が見逃しを error として報告したため。メモリ上限での停止ではない）。
35 件は全て正規表現に名前の一致する関数の変異で、構造体のフィールドを消す変異も `execute_hunk_merge` のものだけだった（他の関数のものは混ざらなかった）。
ファイルごとの内訳（`outcomes.json` から数えた）は次のとおり。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/cli/merge.rs | 4 | 0 | 0 | 1 |
| src/service/merge_flow.rs | 15 | 2 | 0 | 0 |
| src/diff/engine.rs | 12 | 1 | 0 | 0 |

関数ごとには、`validate_merge_args` が 3 件（全て caught）、`run_hunk_merge` が 2 件（caught 1、unviable 1）、`validate_hunk_merge_target` が 4 件（全て caught）、`execute_hunk_merge` が 13 件、`apply_selected_hunks_single_pass` が 13 件だった。
`validate_merge_args` の 3 件は、関数全体を `Ok(())` にする変異（66:5）、--hunks でない分岐の `||` を `&&` にする変異（69:28）、--hunks の分岐の `!=` を `==` にする変異（76:29）で、全て src/cli/merge.rs の単体テストに検知された。

### 変異と関係のないテストだけによる検知

検知した 31 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗から少し進んで止まるため、集めた名前は検知したテストの全てではなく、先に失敗したものである。

負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）だけで検知された変異は次の一件だった。

| 位置 | 変異 | 失敗したテスト | 扱い |
|---|---|---|---|
| src/diff/engine.rs:332:21 | replace < with <= in apply_selected_hunks_single_pass | tui_merge の test_hunk_merge_left_to_right_with_l、test_hunk_merge_right_to_left_with_h_key、test_sensitive_file_merge_requires_confirmation | この関数を製品で呼ぶのは src/service/merge_flow.rs の `execute_hunk_merge` だけで、TUI の変更のまとまりのマージは通らない（`rg apply_selected_hunks_single_pass src` による）。コミットに含めない一時的な書き換えでこの変異を入れて `cargo nextest run --all-features --no-fail-fast` を実行すると、2915 件のうち ssh_integration の test_connect_with_verifier_accept_succeeds の 1 件だけが落ち、同じ変異のままその 1 件と tui_merge を実行し直すと 7 件が全て通った。見かけの検知とみなし、見逃しに数える。書き換えの後に `git diff --stat src/` が空に戻ったことを確かめた |

これを数え戻した見逃しは次の 4 件である。
この回で足したテスト（tests/contract/merge_hunks.rs）は、execute_hunk_merge の 390:5、413:33、446:24、456:55、479:21 から 481:21、502:17 から 505:17 の変異で先に失敗したテストに名前が出た。

### 見逃しと決着

「決着の対象」は計画の区別による。`execute_hunk_merge` の書き込みと --dry-run の経路（DiffResult::Modified）の変異と、`validate_merge_args` の --hunks の分岐、`run_hunk_merge`、`validate_hunk_merge_target`、`apply_selected_hunks_single_pass` の変異を対象にする。
`execute_hunk_merge` の差分のないファイルの経路（DiffResult::Equal の "skipped (no changes)"）の変異は、差分のないファイルでしか落とせないため FLAG-merge-019 の範囲として記録する。

| 位置 | 変異 | 決着の対象 | 決着 |
|---|---|---|---|
| src/diff/engine.rs:352:20 | replace match guard tag == replace_tag with true in apply_selected_hunks_single_pass | 対象 | 同等変異として `.kotowari/mutants-equivalents.yaml` に登録した。下の注 1 |
| src/diff/engine.rs:332:21 | replace < with <= in apply_selected_hunks_single_pass | 対象（見かけの検知を数え戻したもの） | 同等変異として登録した。下の注 2 |
| src/service/merge_flow.rs:427:13 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 対象でない | FLAG-merge-019 の範囲として記録する（差分のないファイルの "skipped (no changes)" の結果） |
| src/service/merge_flow.rs:428:13 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 対象でない | 同上 |

同等変異の判断のため、別の文脈のエージェントに、二つの変異のそれぞれを当てると落ちて今のコードでは通るテストを書かせた（製品コードは変えず、一時的な書き換えとテストは戻させ、作業ツリーに変更が残っていないことを確かめた）。

1. engine.rs:352:20。DiffTag は Equal・Delete・Insert の三つだけで、keep_tag と replace_tag はどちらの方向でも Delete と Insert を一つずつ受け持つ。match の腕は Equal、keep_tag、replace_tag の順のため、三つ目の腕に来る値は必ず replace_tag で、条件を true にしても同じ腕に入る。エージェントは merge の入口（`execute_merge` の --hunks）から、重複した番号、三つの変更からの二つの選択、片側にだけ行のある変更、末尾に改行のない内容、CRLF と空行を含む内容を試したが落とせず、関数を直接呼んで両方向・全ての選び方の出力を比べても一致した。
2. engine.rs:332:21。変わるのは、選んだ番号に hunk の数と等しいものがあるときに黙って無視するか panic するかだけである。製品で唯一の呼び出し元の `execute_hunk_merge` は hunk の数以上の番号を先に "Hunk index N is out of range" のエラーで止めるため（REQ-merge-028）、そのような番号がこの関数に渡ることはない。エージェントは merge の入口から落とせず、落とせたのはこの関数を直接呼んで hunk の数と等しい番号を渡すテストだけだった。その入力は IR のどこにも定められておらず、merge の出力・標準エラー・終了コードに違いが出ないため、根拠にならないテストとして採らなかった（バックアップの回（[backup-test-cleanup.md](backup-test-cleanup.md)）で登録した src/runtime/backup_store.rs の変異と同じ扱い）。

どちらの変異も、製品コードを簡単にすれば変異そのものをなくせる（352 行の条件と到達しない `unreachable!` の腕をまとめる、332 行の範囲の確かめを除く）。この計画は製品コードを変えないため、登録で決着させた。

登録の後に同じ `outcomes.json` を `kotowari mutants --tool cargo-mutants --format text` で読み直すと `mutants: caught=31 survived=2 timeout=0 unviable=1 equivalent=1` になる（332:21 は cargo-mutants の判定では caught のため、この集計の equivalent には入らない）。
残る 2 件は FLAG-merge-019 の範囲で、決着の対象の見逃しは全て決着した。新しい FLAG の候補はない。

### 前の回との比較

`execute_hunk_merge` は前の回の記録（[merge の書き込みの中身](merge-write-test-cleanup.md) と [merge の symlink と削除](merge-links-test-cleanup.md) の整理後の節）で、決着の対象でない見逃しとして次の 8 件が記録されていた。位置は今のコードでも同じ行を指す。

| 位置 | 変異 | 前の回 | この回 |
|---|---|---|---|
| src/service/merge_flow.rs:413:33 | replace \|\| with && in execute_hunk_merge | 見逃し（tui_merge だけによる見かけの検知） | caught（hunks_on_a_destination_that_is_not_utf8_stops_without_writing） |
| src/service/merge_flow.rs:427:13 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 見逃し | 見逃し（FLAG-merge-019 の範囲） |
| src/service/merge_flow.rs:428:13 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 見逃し | 見逃し（同上） |
| src/service/merge_flow.rs:479:21 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 見逃し | caught（hunks_dry_run_json_reports_would_merge_without_writing） |
| src/service/merge_flow.rs:480:21 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 見逃し | caught（同上） |
| src/service/merge_flow.rs:481:21 | delete field hunk_info from struct MergeFileResult expression in execute_hunk_merge | 見逃し | caught（同上） |
| src/service/merge_flow.rs:502:17 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 見逃し | caught（hunks_json_reports_the_applied_hunk_without_backup_when_backup_is_disabled ほか） |
| src/service/merge_flow.rs:503:17 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 見逃し | caught（同上） |

この回の `execute_hunk_merge` の見逃しは 427:13 と 428:13 だけで、前の回の見逃しの部分集合になっている。
`validate_merge_args` は [merge の指定・確認・出力](merge-cli-test-cleanup.md) の回で同じ 3 件が検知されており、この回も全て検知された。`validate_hunk_merge_target` は前の回で見逃しが記録されておらず、この回も見逃しはない。
src/diff/engine.rs と `run_hunk_merge` はこれまでの回で変異テストを回していないため比べる相手がなく、見逃しの 2 件（engine.rs:352:20、332:21）は上のとおり新しい見逃しとして決着させた。`run_hunk_merge` に見逃しはなかった。

## 要件の verification の見直し

REQ-merge-028 から 031 の verification は全て unit で、いずれも具体的な指定と構成の入力で結果が決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-merge-028 | unit | TBL-merge-001 の指定ごとの場面で、止まることとエラーの文言が決まる |
| REQ-merge-029 | unit | 書き込みか --dry-run か、バックアップの有効・無効の場面ごとに merged の一件の項目が決まる |
| REQ-merge-030 | unit | 同じ場面ごとにテキストの行の形が決まる |
| REQ-merge-031 | unit | 参照先に対して左右が異なる変更をした場面で、書き込まずに止まることと文言が決まる |
