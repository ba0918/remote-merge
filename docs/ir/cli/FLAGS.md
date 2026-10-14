# Flags

## Flags

### FLAG-cli-007: 確認を断ったときの JSON
- kind: ambiguity
- related: REQ-cli-018, REQ-cli-040
- source: docs/decision/records/2026-09-27-adopt-sync.md#A10

--format json の sync で確認を断ると、実装は標準出力に何も出さずに終了コード 0 で終わる。JSON 指定なら成功とエラーのどちらでも JSON を返すという REQ-cli-018 に取り消しが含まれるかの読みが分かれる。

### FLAG-cli-015: 確認の行で件数が 0 の部分を省くか
- kind: ambiguity
- related: REQ-cli-040
- source: docs/decision/records/2026-09-27-sync-mutant-flags.md#A1

sync の書き込む前の確認で、実装は書き込むファイルだけがある書き込み先を "[先] N files to merge"、削除予定だけがある書き込み先を "[先] M files to delete" と件数が 0 の部分を省いて出す。REQ-cli-040 は行の形を "[先] N files to merge, M files to delete" と書いており、件数が 0 の部分を省くかは定めていない。旧総合仕様の出力例 "[server1] 3 files to merge (2 modified, 1 added)" は削除の部分を出していない。

### FLAG-cli-032: 0 始まりの行番号
- kind: gap
- related: REQ-cli-058, REQ-cli-053
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A15

diff のテキストの "@@" の行と JSON の left_start・right_start は 0 始まりの行番号で、先頭の行の変更が "@@ -0,1 +0,1 @@" になる。旧資料に記述がなく、行番号を確かめるテストもない。

### FLAG-cli-058: conflict_regions の要素の形
- kind: contradiction
- related: REQ-cli-065
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A6

旧総合仕様の diff の JSON の例は conflict_regions の要素を "ref_range": [10, 12]、"left_lines"、"right_lines" の三つとするが、実装は "ref_range" を "start" と "end" を持つオブジェクト（0 始まりで終端を含まない）で出し、TUI 用の "left_diff_range"・"right_diff_range"・"left_file_lines"・"right_file_lines" も出す。
