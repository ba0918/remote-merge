# 三者の状態の表示

左右の差分に加えて参照先との差と全体像を対話画面で確認する方法。

## Requirements

### REQ-tui-005: 参照先との違いを見分けられる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A44
- verification: unit

参照先を指定した TUI では、左右間の差分と参照先との差を区別して表示する。

### REQ-tui-006: 三者の状態を概観できる
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A44
- verification: unit

参照先を指定した TUI では、三者のファイル状態をまとめて確認できるサマリーを表示できる。

## Examples

```gherkin
@id=EX-tui-009 @about=REQ-tui-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A44
Scenario: 左右と参照先が異なる
Given 三者で内容が異なるファイルがある
When TUI でそのファイルを見る
Then 左右の差と参照先との差を見分けられる

@id=EX-tui-010 @about=REQ-tui-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A44
Scenario: 参照先と片側が同じ
Given 参照先と左側が同じ内容のファイルがある
When TUI でそのファイルを見る
Then どちらが参照先と異なるか分かる

@id=EX-tui-011 @about=REQ-tui-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A44
Scenario: 複数のファイルで三者が食い違う
Given 異なる状態のファイルが複数ある
When TUI で三者サマリーを開く
Then ファイルごとの三者の状態を概観できる

@id=EX-tui-012 @about=REQ-tui-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A44
Scenario: 三者が同じ状態である
Given 参照先と左右が同じ状態のファイルがある
When TUI で三者サマリーを開く
Then 食い違いがないことが分かる
```
