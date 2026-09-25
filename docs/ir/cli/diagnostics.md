# 診断ログとイベントの閲覧

障害調査のために記録を取得し、機密内容が記録に混入しないようにする規則。

## Requirements

### REQ-cli-014: 記録を閲覧できる
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A34
- verification: unit

CLI の logs と events で、保存された診断ログと操作イベントを閲覧できる。

### REQ-cli-015: 記録に内容や認証情報を含めない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A34
- verification: unit

診断ログと操作イベントに対象ファイルの内容や認証情報を記録しない。

## Examples

```gherkin
@id=EX-cli-027 @about=REQ-cli-014 @source=docs/decision/records/2026-09-25-spec-migration.md#A34
Scenario: 障害のログを確認する
Given 操作によって診断ログが記録されている
When CLI logs を実行する
Then 記録された診断ログを閲覧できる

@id=EX-cli-028 @about=REQ-cli-014 @source=docs/decision/records/2026-09-25-spec-migration.md#A34
Scenario: 操作イベントを確認する
Given TUI 操作によってイベントが記録されている
When CLI events を実行する
Then 記録された操作イベントを閲覧できる

@id=EX-cli-029 @about=REQ-cli-015 @source=docs/decision/records/2026-09-25-spec-migration.md#A34
Scenario: ファイルの内容を扱う操作を記録する
Given マージ対象のファイルに機密の内容がある
When 操作の診断ログを確認する
Then 診断ログにそのファイルの内容は含まれない

@id=EX-cli-030 @about=REQ-cli-015 @source=docs/decision/records/2026-09-25-spec-migration.md#A34
Scenario: SSH 認証を行う
Given 接続に認証情報を用いる
When 操作イベントと診断ログを確認する
Then 認証情報の値は含まれない
```
