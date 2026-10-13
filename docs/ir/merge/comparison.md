# マージ対象の内容比較

merge と sync が、compare/judgement.md の TBL-compare-003 のどちらの比べ方で対象を比べるか。判定の規則そのものは compare/judgement.md の TBL-compare-001 と TBL-compare-002 に置く。

## Requirements

### REQ-merge-005: ファイル指定では内容を比較する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A5, docs/decision/records/2026-10-12-foundation-4.md#E4
- verification: unit

ファイルを明示指定した merge と sync は、TBL-compare-003 の中身の比べ方で比べ、サイズと更新時刻が同じでも内容が異なれば書き込み先を更新し、内容が同じなら更新しない。

### REQ-merge-006: ディレクトリ指定では内容比較を選べる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A5, docs/decision/records/2026-10-12-foundation-4.md#E4
- verification: unit

ディレクトリ指定の merge と sync に --checksum が与えられたとき、TBL-compare-003 の中身の比べ方で比べ、サイズと更新時刻が同じファイルも内容を比較し、異なるものを更新する。

### REQ-merge-020: ディレクトリ指定の既定はサイズと更新時刻で判定する
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A2, docs/decision/records/2026-10-05-quality-merge.md#A2, docs/decision/records/2026-10-12-foundation-4.md#E4
- verification: unit

--checksum のないディレクトリ指定の merge と sync は、TBL-compare-003 のメタデータの比べ方で比べ、TBL-compare-001 と TBL-compare-002 で違うと判定した組を書き、同じと判定した組は書かない。
そのため、サイズが違う組は中身を読まずに書き、サイズと更新時刻が TBL-compare-001 の意味で同じ組は中身が違っても書かず、片側でも秒の精度しかなく秒が同じ組は中身で決めて、中身が違えば書く。

## Examples

```gherkin
@id=EX-merge-009 @about=REQ-merge-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A5
Scenario: ファイルのサイズと更新時刻が同じでも中身が違う
Given 読み込み元と書き込み先のファイルのサイズと更新時刻が同じで中身が違う
When ファイルを指定してマージする
Then 書き込み先の中身は読み込み元に揃う

@id=EX-merge-010 @about=REQ-merge-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A5
Scenario: 内容が同じファイルは書き換えない
Given 読み込み元と書き込み先のファイルの中身が同じである
When ファイルを指定して同期する
Then 書き込み先のファイルは書き換えられない

@id=EX-merge-011 @about=REQ-merge-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A5
Scenario: 内容比較を指定したディレクトリ同期
Given ディレクトリの中にサイズと更新時刻が同じで中身が違うファイルがある
When --checksum を付けてディレクトリを同期する
Then 中身が違うファイルは更新される

@id=EX-merge-041 @about=REQ-merge-020 @source=docs/decision/records/2026-10-05-quality-merge.md#A2
Scenario: 片側の更新時刻が秒の精度しかない
Given ディレクトリの中に、サイズが同じで中身が違い、ローカルではナノ秒、リモートでは秒の精度しかない更新時刻が同じ秒を指すファイルがある
When --checksum なしでディレクトリを同期する
Then 中身を読み比べて、そのファイルを書き込む
```
