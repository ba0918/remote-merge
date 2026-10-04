# 差分のコピーとレポート出力

対話画面で差分を外へ持ち出す操作。

## Requirements

### REQ-tui-007: 選択中の差分をコピーできる
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A50
- verification: unit

TUI で選択中のファイルの差分をクリップボードへコピーできる。

### REQ-tui-008: 調査結果をレポートに出せる
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A50
- verification: unit

TUI の差分と集計をレポートとして出力できる。

## Examples

```gherkin
@id=EX-tui-013 @about=REQ-tui-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A50
Scenario: 通常ファイルの差分をコピーする
Given 通常ファイルの差分を選んでいる
When 差分をクリップボードへコピーする
Then 選択したファイルの差分がコピーされる

@id=EX-tui-015 @about=REQ-tui-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A50
Scenario: 調査結果を出力する
Given 複数ファイルの差分を確認している
When TUI からレポートを出力する
Then 差分と集計がレポートに含まれる

@id=EX-tui-016 @about=REQ-tui-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A50
Scenario: 差分がない
Given 出力対象の差分がない
When TUI からレポートを出力する
Then 差分を含むレポートは作られない
```
