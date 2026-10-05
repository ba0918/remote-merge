# マージ対象の内容比較

merge と sync の対象がメタデータ上等しく見える場合に、内容の違いを判定する規則。

## Requirements

### REQ-merge-005: ファイル指定では内容を比較する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A5
- verification: unit

ファイルを明示指定した merge と sync は、サイズと更新時刻が同じでも内容が異なれば書き込み先を更新し、内容が同じなら更新しない。

### REQ-merge-006: ディレクトリ指定では内容比較を選べる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A5
- verification: unit

ディレクトリ指定の merge と sync に --checksum が与えられたとき、サイズと更新時刻が同じファイルも内容を比較し、異なるものを更新する。

### REQ-merge-020: ディレクトリ指定の既定はサイズと更新時刻で判定する
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A2, docs/decision/records/2026-10-05-quality-merge.md#A2
- verification: unit

--checksum のないディレクトリ指定の merge と sync は、サイズと更新時刻で変更ありと判定した組のうちサイズが同じものだけ中身を読み比べて中身が同じ組は書かず、サイズと更新時刻が同じファイルは中身が違っても書かない。
更新時刻が同じとするのは、両側がともに秒より細かい精度の更新時刻を持ち、両側に共通する最も細かい精度で比べて同じときに限り、片側でも秒の精度しかなく秒が同じ組は中身を読み比べて、中身が違えば書く。

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
