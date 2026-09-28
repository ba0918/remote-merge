# merge で書き込み先の変更を黙って失う二つの問題を直した記録

merge の二つの問題（--ref を使うファイル全体の merge が書き込み先だけの変更を上書きしていたこと、--hunks の番号が diff --format json の hunk と一致しなかったこと）を直した過程の記録。
判断の出典は [決定記録](../decision/records/2026-09-28-merge-ref-hunks-fix.md)（A1〜A11）。
各要件の根拠にしたテスト、修正の前に落ちたことの記録、変異テストの結果と見逃しの決着を残す。
この修正ではテストを消していない。

根拠テストは `tests/contract/merge_ref_hunks_fix.rs` にある。
準備は `tests/contract/merge_support.rs` の `Fixture` を使い、書き込み先 develop と参照先 staging を `RuntimeTargets::with_local` で一時ディレクトリに差し替えて `execute_merge` を関数呼び出しで呼ぶ。
残っている FLAG の挙動（FLAG-cli-023・024・027、FLAG-merge-016・017・019）は構成に含めない。

## 参照先に対して書き込み先が変わったファイル（REQ-cli-051）

判定は `src/service/merge.rs` の純粋関数 `reference_check_failure(base, source, destination)` に置き、`execute_merge` の --ref があり --force も --dry-run もないときの絞り込みがその結果に従う。
読み込み元か書き込み先のツリーのノードが symlink のファイルは、今回の対象外（A9）のため従来の `has_three_way_conflict` の判定のままにした。
三つのどれかで中身を読めないファイルの扱い（"three-way comparison incomplete"）は変えていない。

構成は、先頭と末尾の行の間に変わらない 8 行を挟んだテキストのファイル file.txt 一つ。
「左右が別々の行を変えた」は、読み込み元が先頭の行、書き込み先が末尾の行を変える。
二つの変更は離れているため、従来の行単位の競合の検出では競合にならない。

| 例 | 根拠テスト | 確かめること |
|---|---|---|
| EX-cli-063 | a_file_whose_sides_changed_different_lines_fails_as_a_three_way_conflict | failed がちょうど [{path: file.txt, error: "three-way conflict"}] で、develop が変わらない |
| EX-cli-064 | a_file_changed_only_on_the_destination_fails_and_is_not_written | 読み込み元が参照先と同じで、failed がちょうど [{path: file.txt, error: "destination changed since reference"}] で、develop が変わらない |
| EX-cli-065 | a_file_changed_only_on_the_source_is_written | 書き込み先が参照先と同じで、failed が空で、develop が読み込み元の中身になる |
| EX-cli-066 | force_writes_a_file_whose_sides_changed_different_lines | EX-cli-063 の構成に --force を付け、failed が空で、develop が読み込み元の中身になる |
| EX-cli-021 | a_merge_with_a_reference_updates_only_the_destination | EX-cli-065 の構成（A7）で、merged に file.txt が出て、develop が読み込み元の中身になり、staging と local が変わらない |

判定の純粋関数の四つの場合は `src/service/merge.rs` の単体テストが確かめる。

| 場合 | 単体テスト |
|---|---|
| 書き込み先が参照先と同じ | reference_check_writes_when_the_destination_equals_the_reference |
| 読み込み元だけが参照先と同じ | reference_check_fails_when_only_the_destination_changed |
| 両方が参照先と違う | reference_check_fails_as_a_conflict_when_both_sides_changed |
| 読み込み元と書き込み先が同じ | reference_check_writes_when_the_source_equals_the_destination |

### 修正の前の実行

`cargo nextest run --all-features --test contract merge_ref_hunks_fix` を `execute_merge` を直す前に実行した。

- 落ちた: a_file_whose_sides_changed_different_lines_fails_as_a_three_way_conflict（failed が []）、a_file_changed_only_on_the_destination_fails_and_is_not_written（failed が []）
- 通った: a_file_changed_only_on_the_source_is_written、force_writes_a_file_whose_sides_changed_different_lines、a_merge_with_a_reference_updates_only_the_destination（修正の前から満たしている挙動）

純粋関数の単体テストは、本体を `todo!()` にした関数の形だけを足して四件とも落ちることを確かめてから本体を書いた。

### 期待が変わった既存のテスト

- `tests/contract/cli_results.rs` の reference_side_is_not_modified_by_a_three_way_merge は --force で三つとも違うファイルを書くテストで、A1 の確認を通らない。本体は変えず、印から EX-cli-021 を外して EX-cli-034 の根拠として残した。
- `tests/contract/merge_results.rs` の a_binary_file_changed_on_only_one_side_is_not_a_conflict_and_is_written を a_binary_changed_only_on_the_source_is_written_and_one_changed_only_on_the_destination_fails に改め、merged がちょうど [a.bin]、failed がちょうど [{path: b.bin, error: "destination changed since reference"}]、終了コード 2、develop の a.bin が読み込み元のバイト列、b.bin が変わらないことを確かめるようにした。

## --hunks の番号の数え方（REQ-merge-032）

`execute_hunk_merge`（`src/service/merge_flow.rs`）は、番号・hunks_total・範囲外の判定を `DiffResult::Modified` の `hunks`（文脈 3 行の区切り。diff --format json と同じもの）の数で数えるようにした。
選んだ区切りに入る `merge_hunks`（文脈 0 行の区切り）は `src/diff/engine.rs` の純粋関数 `merge_hunks_in_display_hunks` が行の範囲の包含で選び、選ばれたものを全て適用する。
全ての `merge_hunks` が選ばれたときは、従来どおり読み込み元の中身をそのまま使う。
hunks_applied は利用者が渡した番号のまま出す。

構成は、先頭と末尾の行を変える二つの変更の間に変わらない行を挟んだテキストのファイル file.txt 一つ。
テストは `merge_hunks.rs` と同じく --force を付けて参照先を付けない（--hunks が確認を出さないこと（FLAG-merge-016）に頼らない）。
diff の hunk の数は、`merge_support.rs` に足した `Fixture::diff_json`（`execute_diff` を関数として呼ぶ）で確かめる。

| 例 | 根拠テスト | 確かめること |
|---|---|---|
| EX-merge-038 | two_changes_two_lines_apart_are_one_hunk_in_diff_and_in_merge | 変わらない行を 2 行挟んだ構成で、diff の JSON の hunks がちょうど一つ、--hunks 0 の merge の hunks_total が 1、develop に二つの変更の両方が入る |
| EX-merge-038 | hunk_index_one_is_out_of_range_when_two_changes_are_one_hunk | 同じ構成で --hunks 1 のエラーの文言がちょうど "Hunk index 1 is out of range (total hunks: 1)" で、develop が変わらない |
| EX-merge-039 | hunk_index_one_applies_only_the_second_of_two_changes_seven_lines_apart | 変わらない行を 7 行挟んだ構成で、--hunks 1 の merge の hunks_total が 2、develop には二つ目の変更だけが入る |

純粋関数の単体テスト（`src/diff/engine.rs`）は、一つの表示用の区切りに二つの変更が入る場合（番号 0 で両方が選ばれる）、二つの区切りに分かれる場合（番号 1 で二つ目だけが選ばれる）、何も選ばない場合を確かめる。

### 修正の前の実行

`cargo nextest run --all-features --test contract merge_ref_hunks_fix` を `execute_hunk_merge` を直す前に実行した。

- 落ちた: two_changes_two_lines_apart_are_one_hunk_in_diff_and_in_merge（hunks_total が 2。diff の hunks が一つであることの確かめは通った）、hunk_index_one_is_out_of_range_when_two_changes_are_one_hunk（merge がエラーにならず成功した）
- 通った: hunk_index_one_applies_only_the_second_of_two_changes_seven_lines_apart（離れた変更では従来も番号が一致していた）

純粋関数の単体テストは、本体を `todo!()` にした関数の形だけを足して三件とも落ちることを確かめてから本体を書いた。
修正の後、既存の `merge_hunks.rs` のテストと `merge_paths.rs` の selected_hunk_changes_only_the_selected_region・selecting_all_hunks_applies_both_regions を含む `cargo nextest run --all-features` の全 2932 件が通った。

## 変異テスト

テストを削除しない修正のため、最初の実行は省き（[決定記録](../decision/records/2026-09-28-mutation-scope.md)）、全てのテストを足した後のコミット 6a2d327 で一回だけ実行した。実行中は作業ツリーに触れていない。
直した二つの純粋関数と `execute_hunk_merge`、`execute_merge` に絞った。並列数は既定の 2 である。

```sh
scripts/mutants.sh --re '(reference_check_failure|merge_hunks_in_display_hunks|execute_hunk_merge|execute_merge)' src/service/merge.rs src/cli/merge.rs src/service/merge_flow.rs src/diff/engine.rs
```

全体の集計は `mutants: caught=43 survived=4 timeout=0 unviable=1 equivalent=0`（48 件、実行時間は約 20 分）。
スクリプトの終了コードは 1（見逃しの error による。メモリ上限での停止ではない）。

| 関数 | caught | survived | unviable |
|---|---|---|---|
| reference_check_failure（src/service/merge.rs） | 7 | 0 | 0 |
| merge_hunks_in_display_hunks（src/diff/engine.rs） | 6 | 0 | 0 |
| execute_hunk_merge（src/service/merge_flow.rs） | 13 | 2 | 0 |
| execute_merge（src/cli/merge.rs） | 17 | 2 | 1 |

`execute_merge` の unviable は関数全体を `Ok(Default::default())` にする変異（105:5）である。
二つの純粋関数の変異は全て、その関数の単体テストで検知された。
`execute_hunk_merge` の番号の数え方を変えた箇所の変異（453:24 の範囲外の判定、465:61 の全ての区切りを選んだときの近道）は `merge_hunks.rs` のテストで検知された。

### 変異と関係のないテストだけによる検知

変異ごとのログから、先に失敗したテストの名前を集めた。
負荷の下で落ちることのある tui_merge のテストだけで検知された変異が二件あった。
どちらも、コミットに含めない一時的な書き換えでその変異を入れて `cargo nextest run --all-features --no-fail-fast` を実行すると 2932 件が全て通ったため、見かけの検知と判断し、見逃しとして扱う（書き換えの後に `git diff --stat src/` が空に戻ったことを確かめた）。

| 位置 | 変異 | 失敗したテスト |
|---|---|---|
| src/cli/merge.rs:299:48 | replace \|\| with && in execute_merge | tui_merge の test_merge_cancel_with_n |
| src/cli/merge.rs:246:47 | replace && with \|\| in execute_merge | tui_merge の test_merge_cancel_with_n、test_hunk_merge_right_to_left_with_h_key |

### 見逃しと決着

| 位置 | 変異 | 決着 |
|---|---|---|
| src/cli/merge.rs:299:48 | replace \|\| with && in execute_merge（見かけの検知を数え戻したもの） | FLAG-cli-027 の範囲として記録する。読み込み元か書き込み先のどちらか一方だけが symlink のファイルを従来の判定に回す分岐で、symlink の扱いは今回の対象外（A9）のためテストは足さない |
| src/cli/merge.rs:246:32 | replace && with \|\| in execute_merge | --ref の判定でない部分（機密ファイルの件数の通知の条件）。記録だけする（前の記録では FLAG-cli-022 の範囲） |
| src/cli/merge.rs:246:47 | replace && with \|\| in execute_merge（見かけの検知を数え戻したもの） | 同上 |
| src/cli/merge.rs:324:62 | delete ! in execute_merge | 全てのファイルが参照先に対する確認で外れたときの早期の戻り。FLAG-cli-026 の範囲として記録する（書き込むファイルのない merge と集約先の場所） |
| src/service/merge_flow.rs:430:13 | delete field path from struct MergeFileResult expression in execute_hunk_merge | FLAG-merge-019 の範囲として記録する（差分のないファイルの "skipped (no changes)" の結果） |
| src/service/merge_flow.rs:431:13 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 同上 |

同等変異の登録はない。新しい FLAG の候補はない。

### 前の記録との比較

最初の実行を省いたため、[merge の指定・確認・出力](merge-cli-test-cleanup.md) と [merge の変更のまとまりを選ぶマージ](merge-hunks-test-cleanup.md) の整理後の最後の記録と比べる。修正で行がずれたため、位置は今のコードの行で書く。

| 前の記録の位置 | 今の位置 | 変異 | 前の記録 | この回 |
|---|---|---|---|---|
| src/cli/merge.rs:246:32 | 246:32 | replace && with \|\| in execute_merge | 見逃し（FLAG-cli-022） | 見逃し |
| src/cli/merge.rs:246:47 | 246:47 | replace && with \|\| in execute_merge | 見逃し（FLAG-cli-022） | 見逃し（見かけの検知） |
| src/cli/merge.rs:316:62 | 324:62 | delete ! in execute_merge | 見逃し（FLAG-cli-026） | 見逃し |
| src/service/merge_flow.rs:427:13 | 430:13 | delete field path in execute_hunk_merge | 見逃し（FLAG-merge-019） | 見逃し |
| src/service/merge_flow.rs:428:13 | 431:13 | delete field status in execute_hunk_merge | 見逃し（FLAG-merge-019） | 見逃し |
| なし | src/cli/merge.rs:299:48 | replace \|\| with && in execute_merge | 前はこの分岐がなかった | 見逃し（見かけの検知。FLAG-cli-027） |

前の記録の見逃しは全て同じ変異の見逃しのままで、前に検知されていた変異で見逃しになったものはない。増えた見逃しは、この修正で足した symlink の分岐の一件だけである。
### 対象外の関数への影響

前の記録で `has_three_way_conflict` の `||` を `&&` にする二つの変異（src/service/merge.rs:69:22 と 69:38）を落としていたのは a_binary_file_changed_on_only_one_side_is_not_a_conflict_and_is_written だった。
このテストは期待を改め、ファイル全体の merge の通常ファイルは `has_three_way_conflict` を通らなくなったため、二つの変異を落とさなくなった。
`has_three_way_conflict` は今回の対象の関数でないため変異テストでは回していないが、コミットに含めない一時的な書き換えで二つの変異をそれぞれ入れて `cargo nextest run --all-features --no-fail-fast` を実行すると、どちらも 2932 件が全て通った（書き換えの後に `git diff --stat src/` が空に戻ったことを確かめた）。
この関数を今も使うのは --hunks の三者の競合の判定（REQ-merge-031）と symlink のファイル（FLAG-cli-027）である。
UTF-8 として読めない中身でしか違いが出ず、--hunks はそのファイルをバイナリとしてエラーで止めるため、落とすテストはどちらのエラーが先に出るかという IR が決めていない点に触れる。この計画の範囲外のため、テストは足さず、扱いを利用者の判断に残す。

## 要件の verification

REQ-cli-051 と REQ-merge-032 の verification はどちらも unit で、具体的な三つの中身や指定した番号の構成の入力で結果が決まる挙動のため、要件の性質に合う。見直しの候補はない。
