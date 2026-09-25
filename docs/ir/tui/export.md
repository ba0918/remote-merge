# 差分のコピーとレポート出力

対話画面で差分を外へ持ち出す操作と、機密ファイルの内容を含める際の確認。

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

### REQ-tui-009: 機密の持ち出しを事前に確認する
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A50, docs/decision/records/2026-09-25-spec-migration.md#A52
- verification: unit

機密ファイルの本文をコピーまたはレポート出力する前に対象と出力先を利用者に示して確認し、確認されなければ本文を出力しない。

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

@id=EX-tui-017 @about=REQ-tui-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A52
Scenario: 機密ファイルのコピーを拒否する
Given 選択した差分に機密ファイルの内容がある
When コピー先を示す確認を拒否する
Then 機密ファイルの本文はクリップボードにコピーされない

@id=EX-tui-018 @about=REQ-tui-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A52
Scenario: 機密を含むレポートを承認する
Given レポートの対象に機密ファイルの差分がある
When 対象と出力先を確認して本文の出力を承認する
Then 承認した機密ファイルの本文がレポートに含まれる
```
