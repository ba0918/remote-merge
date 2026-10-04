# 診断ログの閲覧

障害調査のために記録を取得し、ファイルの中身が記録に混入しないようにする規則。

## Requirements

### REQ-cli-014: 記録を閲覧できる
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A34, docs/decision/records/2026-10-04-tui-disposition.md#A4
- verification: unit

CLI の logs で、保存された診断ログを閲覧できる。

### REQ-cli-015: 記録に内容や認証情報を含めない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A34, docs/decision/records/2026-10-04-tui-disposition.md#A4
- verification: unit

診断ログに対象ファイルの内容や認証情報を記録しない。

## Examples

```gherkin
@id=EX-cli-027 @about=REQ-cli-014 @source=docs/decision/records/2026-09-25-spec-migration.md#A34
Scenario: 障害のログを確認する
Given 操作によって診断ログが記録されている
When CLI logs を実行する
Then 記録された診断ログを閲覧できる

@id=EX-cli-029 @about=REQ-cli-015 @source=docs/decision/records/2026-09-25-spec-migration.md#A34,docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A13
Scenario: ファイルの内容を扱う操作を記録する
Given マージ対象のファイルに中身がある
When 操作の診断ログを確認する
Then 診断ログにそのファイルの内容は含まれない

@id=EX-cli-030 @about=REQ-cli-015 @source=docs/decision/records/2026-09-25-spec-migration.md#A34,docs/decision/records/2026-10-04-tui-disposition.md#A4
Scenario: SSH 認証を行う
Given 接続に認証情報を用いる
When 診断ログを確認する
Then 認証情報の値は含まれない
```
