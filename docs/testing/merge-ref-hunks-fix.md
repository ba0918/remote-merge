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
