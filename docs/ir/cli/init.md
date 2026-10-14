# プロジェクト設定の初期化

プロジェクトの接続設定を作る操作と、既存の設定の保護。

## Requirements

### REQ-cli-012: 初期化は設定のひな形を生成する
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A33
- verification: unit

init はプロジェクト設定が存在しない場合に設定のひな形を生成する。

### REQ-cli-013: 既存の設定を無断で上書きしない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A33
- verification: unit

プロジェクト設定が存在する場合、init はその内容を黙って置き換えない。

## Examples

```gherkin
@id=EX-cli-023 @about=REQ-cli-012 @source=docs/decision/records/2026-09-25-spec-migration.md#A33
Scenario: 初めて初期化する
Given プロジェクト設定がない
When init を実行する
Then 編集できる設定のひな形が作成される

@id=EX-cli-024 @about=REQ-cli-012 @source=docs/decision/records/2026-09-25-spec-migration.md#A33
Scenario: 既に設定がある
Given プロジェクト設定がある
When init を再実行する
Then 既存設定を黙って置き換えない

@id=EX-cli-025 @about=REQ-cli-013 @source=docs/decision/records/2026-09-25-spec-migration.md#A33
Scenario: 編集した設定を保つ
Given プロジェクト設定に利用者が入力した値がある
When init を再実行する
Then 確認なしに入力済みの値は変わらない

@id=EX-cli-026 @about=REQ-cli-013 @source=docs/decision/records/2026-09-25-spec-migration.md#A33
Scenario: 設定がまだない
Given プロジェクト設定がない
When init を実行する
Then 既存設定との衝突なくひな形が作成される
```
