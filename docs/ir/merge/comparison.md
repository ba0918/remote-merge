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

```
