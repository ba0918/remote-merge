# 非対話コマンドの結果

自動操作が成功・失敗・部分成功を読み分けるための出力。

## Requirements

### REQ-cli-018: JSON 指定は失敗時も JSON を返す
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A43
- verification: unit

--format json に対応する CLI コマンドは、成功とエラーのどちらの場合も JSON として解釈できる結果を返す。

### REQ-cli-019: 部分成功は完全成功と区別する
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A43, docs/decision/records/2026-09-25-spec-migration.md#A42
- verification: unit

非対話 CLI で一部のファイルまたは同期先だけが失敗したとき、完全成功の終了コードとは異なる非ゼロ値を返す。sync は書き込み先ごとの結果も返す。

## Examples

```gherkin
@id=EX-cli-035 @about=REQ-cli-018 @source=docs/decision/records/2026-09-25-spec-migration.md#A43
Scenario: JSON 指定で成功する
Given 対象の差分を取得できる
When JSON 形式で CLI diff を実行する
Then 成功結果は JSON として解釈できる

@id=EX-cli-036 @about=REQ-cli-018 @source=docs/decision/records/2026-09-25-spec-migration.md#A43
Scenario: JSON 指定で失敗する
Given 対象の設定が無効である
When JSON 形式で CLI diff を実行する
Then 失敗結果も JSON として解釈できる

@id=EX-cli-037 @about=REQ-cli-019 @source=docs/decision/records/2026-09-25-spec-migration.md#A43,docs/decision/records/2026-09-25-spec-migration.md#A42
Scenario: 同期先の一部が失敗する
Given 二つの書き込み先のうち一つだけ失敗する
When CLI sync を実行する
Then 失敗した先が個別に報告され終了コードは完全成功と異なる非ゼロになる

@id=EX-cli-038 @about=REQ-cli-019 @source=docs/decision/records/2026-09-25-spec-migration.md#A43,docs/decision/records/2026-09-25-spec-migration.md#A42
Scenario: すべて成功する
Given どのファイルも更新できる
When CLI sync を実行する
Then すべての先が成功と報告され完全成功の終了コードになる
```
