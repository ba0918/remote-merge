# 設定の優先順位とフィルター

グローバル設定とプロジェクト設定の両方がある場合に利用者へ適用される値。

## Requirements

### REQ-config-001: プロジェクト固有の値を優先する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A14
- verification: unit

同名サーバ・ローカル root・SSH 設定がグローバルとプロジェクトの両方にあるとき、プロジェクト側の値を採用する。

### REQ-config-002: 両階層のフィルターを適用する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A14
- verification: unit

除外・機密・対象フィルターをグローバル設定とプロジェクト設定の両方に指定したとき、両方の指定を結合して適用する。

## Examples

```gherkin
@id=EX-config-001 @about=REQ-config-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A14
Scenario: 同名サーバを両方に設定する
Given グローバルとプロジェクトに同名サーバの異なる接続先がある
When 設定を読み込む
Then プロジェクト側の接続先が使われる

@id=EX-config-002 @about=REQ-config-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A14
Scenario: プロジェクトにそのサーバがない
Given グローバルだけに設定されたサーバがある
When 設定を読み込む
Then グローバル側の接続先が使われる

@id=EX-config-003 @about=REQ-config-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A14
Scenario: 異なる除外パターンがある
Given 両方の設定に異なる除外パターンがある
When ファイル一覧を取得する
Then どちらのパターンに一致するファイルも除外される

@id=EX-config-004 @about=REQ-config-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A14,docs/decision/records/2026-09-25-spec-migration.md#A19
Scenario: グローバルだけに機密パターンがある
Given グローバルにだけ機密パターンがある
When プロジェクト設定と組み合わせて差分を表示する
Then グローバルで指定された機密パターンが適用される
```
