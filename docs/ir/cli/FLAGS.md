# Flags

## Flags

### FLAG-cli-002: 参照先との違いの印の値
- kind: contradiction
- related: REQ-cli-035
- source: docs/decision/records/2026-09-27-adopt-status.md#A16

旧総合仕様は ref_badge の値を "differs"・"exists_only_in_ref"・"missing_in_ref" とするが、実装は三つすべてで中身が同じファイルと、片方だけにあって参照先にもないファイルに "all_equal" を付ける（テキストでは表示しない）。一覧が左右の和集合のため "exists_only_in_ref" は status では出ず、ref_only は常に 0 になる。

### FLAG-cli-007: 確認を断ったときの JSON
- kind: ambiguity
- related: REQ-cli-018, REQ-cli-040
- source: docs/decision/records/2026-09-27-adopt-sync.md#A10

--format json の sync で確認を断ると、実装は標準出力に何も出さずに終了コード 0 で終わる。JSON 指定なら成功とエラーのどちらでも JSON を返すという REQ-cli-018 に取り消しが含まれるかの読みが分かれる。

### FLAG-cli-008: 接続に失敗した書き込み先の並び
- kind: gap
- related: REQ-merge-015, REQ-cli-039
- source: docs/decision/records/2026-09-27-adopt-sync.md#A11

接続またはツリーの取得に失敗した書き込み先を、実装は --right の指定順によらず結果の末尾に並べる。旧資料に記述がなくテストもない。

### FLAG-cli-010: 種類の違いによるスキップと書き込み先の状態
- kind: contradiction
- related: REQ-cli-041, REQ-cli-044, REQ-merge-001
- source: docs/decision/records/2026-09-27-adopt-sync.md#A13

旧個別仕様 symlink-merge の 3.3 は種類の違い・symlink の削除・パスを辿れないことによるスキップが一件でもある書き込み先の status を "partial" にし全体の終了コードを 2 にするとするが、実装は status の判定に skipped を数えないため、そのスキップだけの書き込み先は "success" になり終了コードは 0 になる。

### FLAG-cli-011: 削除の成否と書き込み先の状態
- kind: gap
- related: REQ-cli-041
- source: docs/decision/records/2026-09-27-adopt-sync.md#A14

実装は書き込み先の status の判定に削除の成否を数えないため、書き込んだファイルがなく削除だけがある書き込み先で一部の削除に失敗すると、削除できたファイルがあっても "partial" ではなく "failed" になる。旧資料に記述がなくテストもない。

### FLAG-cli-013: dry-run の削除予定の表示
- kind: gap
- related: REQ-cli-045, REQ-cli-004
- source: docs/decision/records/2026-09-27-adopt-sync.md#A16

--dry-run の sync で書き込む予定がなく削除予定だけがある書き込み先を、実装はテキストで "(deleted)" と削除済みの形で出し、JSON でも deleted の status を実際に削除したときと同じ "ok" にするため、予定と実行済みを区別できない。旧資料に記述がなくテストもない。

### FLAG-cli-015: 確認の行で件数が 0 の部分を省くか
- kind: ambiguity
- related: REQ-cli-040
- source: docs/decision/records/2026-09-27-sync-mutant-flags.md#A1

sync の書き込む前の確認で、実装は書き込むファイルだけがある書き込み先を "[先] N files to merge"、削除予定だけがある書き込み先を "[先] M files to delete" と件数が 0 の部分を省いて出す。REQ-cli-040 は行の形を "[先] N files to merge, M files to delete" と書いており、件数が 0 の部分を省くかは定めていない。旧総合仕様の出力例 "[server1] 3 files to merge (2 modified, 1 added)" は削除の部分を出していない。

### FLAG-cli-016: 種類の違いによるスキップと merge の終了コード
- kind: contradiction
- related: REQ-cli-047, FLAG-cli-010, REQ-merge-001
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A7

旧個別仕様 symlink-merge の 3.3 は種類の違い・symlink の削除・パスを辿れないことによるスキップが一件でもある merge の終了コードを 2 にするとするが、実装は merge の終了コードの判定に skipped を数えないため、そのスキップだけの merge は終了コード 0 になる。

### FLAG-cli-024: dry-run で参照先に対する確認をしない
- kind: gap
- related: REQ-cli-004, REQ-cli-051
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A15, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A10

--ref がある --dry-run の merge で、実装は REQ-cli-051 の参照先に対する確認をしないため、実際に実行すると REQ-cli-051 で書き込まれないファイルも merged に status "would merge" で出す。旧資料に記述がなくテストもない。

### FLAG-cli-026: 書き込むファイルのない merge と集約先の場所
- kind: ambiguity
- related: REQ-backup-018, REQ-cli-051
- source: docs/decision/records/2026-09-28-merge-cli-mutant-flags.md#A2, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A10

バックアップが有効で集約先の場所が決まらない構成で、書き込むファイルが一つもない merge のとき、実装は全てのファイルが REQ-cli-051 の参照先に対する確認で外れた場合はエラーで止まらず failed にその確認の error（"three-way conflict" か "destination changed since reference"）を出し、全てのファイルがスキップで外れ REQ-cli-051 の確認で外れたファイルもない場合は "backup store location could not be determined" のエラーで止まる。REQ-backup-018 の「書き込む前にエラーで止まる」が書き込むファイルのない merge に及ぶかを IR は決めていない。

### FLAG-cli-027: 参照先を使う merge での symlink
- kind: ambiguity
- related: REQ-cli-051, REQ-merge-003
- source: docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A9

--ref を使う merge で読み込み元か書き込み先が symlink のとき、REQ-cli-051 の確認が symlink を何で比べるか（リンク先の文字列か、辿った先の中身か）を実装から確かめておらず、REQ-cli-051 の対象から外している。

### FLAG-cli-032: 0 始まりの行番号
- kind: gap
- related: REQ-cli-058, REQ-cli-053
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A15

diff のテキストの "@@" の行と JSON の left_start・right_start は 0 始まりの行番号で、先頭の行の変更が "@@ -0,1 +0,1 @@" になる。旧資料に記述がなく、行番号を確かめるテストもない。

### FLAG-cli-034: files_with_changes が数えるもの
- kind: contradiction
- related: REQ-cli-053
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A17

利用者向けの手引きは summary の files_with_changes を hunk を一つ以上持つファイルの数とするが、実装はハッシュの違うバイナリとリンク先の違う symlink も数える。

### FLAG-cli-048: 種類の違いの note の文言
- kind: contradiction
- related: REQ-cli-020
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A6

REQ-cli-020 は解決後の種類の差を示すとするが、note は相手側に項目がない片側だけの symlink でも "type mismatch: symlink vs file"、相手側が通常ファイルのディレクトリ symlink でも "type mismatch: symlink vs directory" と出し、相手側の種類を正しく示さない場合がある（実装を読んで分かったことで未実行）。

### FLAG-cli-049: ディレクトリの子の一覧の件数の打ち切り
- kind: contradiction
- related: REQ-cli-021
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A7

REQ-cli-021 は件数超過を不完全な比較としてエラーで報告するとするが、diff がディレクトリの子を一覧するときは一つのディレクトリにつき 10,000 件で黙って打ち切り、不完全な比較として報告しない（実装を読んで分かったことで未実行）。

### FLAG-cli-050: 通常のディレクトリの展開の理由のテキスト
- kind: contradiction
- related: REQ-cli-022
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A8

REQ-cli-022 は比較できなかったパスと理由を示すとするが、通常のディレクトリの展開で循環や件数超過が起きたとき、テキスト出力には理由が出ない（JSON では errors に出る。実装を読んで分かったことで未実行）。

### FLAG-cli-058: conflict_regions の要素の形
- kind: contradiction
- related: REQ-cli-065
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A6

旧総合仕様の diff の JSON の例は conflict_regions の要素を "ref_range": [10, 12]、"left_lines"、"right_lines" の三つとするが、実装は "ref_range" を "start" と "end" を持つオブジェクト（0 始まりで終端を含まない）で出し、TUI 用の "left_diff_range"・"right_diff_range"・"left_file_lines"・"right_file_lines" も出す。

### FLAG-cli-059: バイナリのファイルの競合
- kind: contradiction
- related: REQ-cli-016
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A7

用語「競合」はバイナリや UTF-8 として読めないファイルをファイル全体で一つの箇所とし、REQ-cli-016 は競合を比較結果に示すとするが、diff は左右がバイナリ（先頭 8192 バイトに NUL を含むか UTF-8 として不正）のファイルで競合を示さない。先頭 8192 バイトより後ろにだけ不正な UTF-8 があるファイルは、置換文字を含むテキストとして行単位で競合を調べる。

### FLAG-cli-060: 参照先にないかバイナリのファイルの競合
- kind: ambiguity
- related: REQ-cli-016
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A8

diff は参照先にファイルがないとき、または参照先のファイルだけがバイナリのとき、競合を調べず示さない。左右が同じ位置に別の内容を持つこの場合を REQ-cli-016 と用語「競合」の競合に当たるとするかが決まらない。

### FLAG-cli-061: 参照先との差の打ち切り
- kind: ambiguity
- related: REQ-cli-054, REQ-cli-063
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A9

REQ-cli-054 は --max-lines で打ち切ったファイルの truncated を true にするとするが、diff は --max-lines を "ref_hunks" にも別に当てて打ち切り、参照先との差を打ち切ってもそのファイルの "truncated" を変えない。参照先との差の打ち切りに要件が及ぶかが決まらない。

### FLAG-cli-063: 参照先と比べない項目
- kind: gap
- related: REQ-cli-063
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A11

diff は symlink の項目、root_dir の外に解決される項目、--max-files の枠を超えて読まないファイルには、--ref があっても "ref" を付けず参照先と比べない。旧資料に記述がなくテストもない。

### FLAG-cli-064: バイナリのファイルの参照先との差
- kind: gap
- related: REQ-cli-063
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A12

diff は左右がバイナリのファイルに "ref_hunks" を出さず、参照先のファイルだけがバイナリのときは "ref_hunks" を空の配列にするため、参照先が左と同じ場合と見分けられない。旧資料に記述がなくテストもない。

### FLAG-cli-067: 片側にないファイルの競合
- kind: ambiguity
- related: REQ-cli-016
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A15

diff は片側にないファイルを、ない側を空の内容として参照先と比べて競合を調べるため、片側にないことが参照先からの全行の削除として扱われ競合が示されうる。これが REQ-cli-016 の競合に当たるかが決まらない（実装を読んで分かったことで未実行）。

### FLAG-cli-070: 参照先の root_dir の外の判定
- kind: gap
- related: REQ-cli-026, REQ-cli-063
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A18

diff の root_dir の外の判定は左右の項目だけを見るため、参照先の同じパスの項目が root_dir の外を指す symlink のとき、--follow-external-links なしでもその内容が "ref_hunks" に出うる（実装を読んで分かったことで未実行）。

### FLAG-cli-071: 行を足すだけの変更の競合
- kind: ambiguity
- related: REQ-cli-016, REQ-cli-065
- source: docs/decision/records/2026-09-30-diff-ref-mutant-flags.md#A1

用語「競合」は同じ箇所を参照先からの変更の行の範囲が重なることとするが、行を足すだけの変更は参照先の行の範囲が空で、空の範囲が重なるかを定めていない。diff は、左右が同じ位置に行を足したとき、一方が足した位置が他方の変えた範囲の中にあるとき、他方の変えた範囲の先頭の行の前に足したときを競合として示す。

### FLAG-cli-072: 複数の変更と重なる変更の競合の数
- kind: ambiguity
- related: REQ-cli-065
- source: docs/decision/records/2026-09-30-diff-ref-mutant-flags.md#A2, docs/decision/records/2026-09-30-diff-ref-mutant-flags.md#A3

diff は一方の一つの変更が他方の複数の変更と重なるとき（例 参照先 "a\nb\nc" に対して左が三行を全て変え、右が 1 行目と 3 行目を別々に変える）、重なる組を一つにまとめて "conflict_count" を 1 とするが、REQ-cli-065 の競合の数がこの場合に 1 か 2 かは定めていない。接するが別々の二つの競合（例 参照先 "1\n2\n3\n4\n5\n"、左 "A\nB\n3\nC\nD\n"、右 "1\nX\nY\n4\nZ\n" で、参照先の 0..3 と 3..5）は、diff は二つとして "conflict_count" を 2 とするが、一つと数えるか二つと数えるかも定めていない。
