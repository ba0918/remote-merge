# 三者比較の衝突

参照先を基準に両側が違う変更を持つ場合の表示とマージの判断。

## Requirements

### REQ-cli-016: 三者間の競合を明示する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A39, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A4, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A11, docs/decision/records/2026-10-12-foundation-4.md#E16
- verification: unit

参照先に対して左右が同じ箇所を異なる内容に変えた競合があるとき、比較結果に競合を示す。
バイナリと UTF-8 として読めないファイルは、用語の `競合` のとおりファイル全体を一つの箇所とし、左右がともに参照先から変わり左右の中身が違えば一つの競合として示す。

### REQ-cli-017: 競合を勝手に解消しない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A39
- verification: unit

三者比較で競合がある内容を、利用者の判断なしに一方の変更だけを選んで上書きしない。

### REQ-cli-065: diff に競合の数と場所を出す
- kind: state_driven
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A4
- verification: unit

--ref を指定した diff は、競合があるファイルについて、JSON に "conflict_count"（競合の数）と "conflict_regions" を出し、テキストに "Conflicts: N region(s) where both sides changed the same lines differently" を出す。
複数のファイルのテキストでは、末尾に "N conflict(s) detected across files" を出す。競合がなければどれも出さない。

### REQ-cli-097: 参照先にないファイルと消すと変えるの組はファイル全体で一つの競合にする
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E26
- verification: unit

--ref を指定した diff は、参照先になく左右にあって中身が違うファイルと、左右の片方が参照先にあったファイルを消し他方が参照先から中身を変えたファイルを、どちらもファイル全体で一つの競合として示す。
片方が消し他方が参照先と同じままのファイルと、参照先になく左右の中身が同じファイルは、競合として示さない。
ない側を空の中身として行の範囲で比べることはしない。

### REQ-cli-098: 行を足す変更と重なる変更の競合の数え方
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E27
- verification: unit

--ref を指定した diff は、左右が参照先の同じ位置に行を足したとき、一方が行を足した位置が他方の変えた参照先の行の範囲の中にあるとき、一方が他方の変えた範囲の先頭の行の前に行を足したときを、競合として示す。
一方の一つの変更が他方の複数の変更と重なるときは、重なる組を一つの競合として数え、参照先の範囲が接するが重ならない二つの競合は二つと数える（"conflict_count"）。

### REQ-cli-099: ファイル全体の競合の形
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E28, docs/decision/records/2026-10-12-foundation-4.md#E16
- verification: unit

ファイル全体で一つの競合（バイナリと UTF-8 として読めないファイルの競合、REQ-cli-097 の競合）は、JSON の "conflict_regions" に要素を一つだけ出し、その "ref_range" をファイル全体を表す {"whole_file": true} にして、行の範囲と行の内容の欄を出さず、"conflict_count" を 1 とする。

## Examples

```gherkin
@id=EX-cli-104 @about=REQ-cli-097 @source=docs/decision/records/2026-10-12-foundation-4.md#E26
Scenario: 参照先になく左右で中身が違うファイル
Given "new.txt" は参照先になく、左右にあって中身が違う
When --ref を指定して diff --format json を実行する
Then "new.txt" の "conflict_count" は 1 で、"conflict_regions" の一つの要素の "ref_range" はファイル全体を示す

@id=EX-cli-105 @about=REQ-cli-097 @source=docs/decision/records/2026-10-12-foundation-4.md#E26
Scenario: 片方が消し他方が変えたファイル
Given "a.txt" は参照先にあり、左では消され、右では参照先から中身が変わっている
When --ref を指定して diff --format json を実行する
Then "a.txt" はファイル全体で一つの競合として示される

@id=EX-cli-106 @about=REQ-cli-098 @source=docs/decision/records/2026-10-12-foundation-4.md#E27
Scenario: 左右が同じ位置に行を足す
Given 参照先 "a\nb\n" に対して、左は 1 行目の後に "x" を足し、右は 1 行目の後に "y" を足した
When --ref を指定して diff --format json を実行する
Then そのファイルの "conflict_count" は 1 になる

@id=EX-cli-107 @about=REQ-cli-098 @source=docs/decision/records/2026-10-12-foundation-4.md#E27
Scenario: 一つの変更が他方の二つの変更と重なる
Given 参照先 "a\nb\nc" に対して、左は三行をすべて変え、右は 1 行目と 3 行目を別々に変えた
When --ref を指定して diff --format json を実行する
Then そのファイルの "conflict_count" は 1 になる

@id=EX-cli-108 @about=REQ-cli-099 @source=docs/decision/records/2026-10-12-foundation-4.md#E28,docs/decision/records/2026-10-12-foundation-4.md#E16
Scenario: バイナリの競合の形
Given 左右のバイナリ "img.bin" はどちらも参照先から変わり、左右の中身が違う
When --ref を指定して diff --format json を実行する
Then "img.bin" の "conflict_count" は 1 で、"conflict_regions" の要素は一つで、その "ref_range" はファイル全体を示し、行の内容の欄はない

@id=EX-cli-031 @about=REQ-cli-016 @source=docs/decision/records/2026-09-25-spec-migration.md#A39
Scenario: 左右が同じ箇所を異なる内容に変えた
Given 参照先に対して左右が同じ箇所を別々に編集した
When 三者比較を実行する
Then 競合が表示される

@id=EX-cli-032 @about=REQ-cli-016 @source=docs/decision/records/2026-09-25-spec-migration.md#A39
Scenario: 左右が同じ内容へ変更した
Given 参照先から左右が同じ変更をした
When 三者比較を実行する
Then その変更は競合と表示されない

@id=EX-cli-033 @about=REQ-cli-017 @source=docs/decision/records/2026-09-25-spec-migration.md#A39
Scenario: 競合があるファイルを操作する
Given 三者比較で競合が表示されている
When 利用者が競合の扱いを選ばない
Then 競合の内容は自動上書きされない

@id=EX-cli-034 @about=REQ-cli-017 @source=docs/decision/records/2026-09-25-spec-migration.md#A39,docs/decision/records/2026-09-25-spec-migration.md#A25
Scenario: 利用者が書き込みを選ぶ
Given 三者比較で競合が表示されている
When 利用者が書き込み元と先を確認して選ぶ
Then 選んだ書き込み先以外と参照先は変更されない
```
