# 診断ログの閲覧

障害調査のために記録を取得し水準で絞り、ファイルの中身が記録に混入しないようにする規則。

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

### REQ-cli-075: CLI の実行も診断ログを残す
- kind: ubiquitous
- source: docs/decision/records/2026-10-04-tui-disposition.md#A14, docs/decision/records/2026-10-04-tui-disposition.md#A16, docs/decision/records/2026-10-04-tui-disposition.md#A17, docs/decision/records/2026-10-04-tui-disposition.md#A22, docs/decision/records/2026-10-04-tui-disposition.md#A23
- verification: unit

agent を除く CLI のサブコマンドの実行は、ログの細かさを指定しなくても情報レベル以上の診断ログを保存し、その記録は CLI の logs で閲覧できる。
診断ログには remote-merge 自身の記録だけを残し、外部の crate の記録は残さない。
利用者のキャッシュの場所が決まらないときや診断ログを保存できないときは、保存をあきらめてコマンドを続ける。

### REQ-cli-093: logs は指定した水準以上を出す
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A11, docs/decision/records/2026-10-05-quality-cli.md#A16
- verification: unit

logs の --level は、指定した水準とそれより重い水準の記録を出し（重い順に error・warn・info・debug・trace で、"warn" なら WARN と ERROR）、この五つの名前以外の値はエラーで止める。
五つの名前以外の値のエラーの文言と、名前の大文字と小文字の扱いは IR で決めず、実装者が決める。大文字と小文字を区別せずに名前を受け付けてもよい。テストはその文言と大文字と小文字の扱いを固定しない。

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
@id=EX-cli-092 @about=REQ-cli-093 @source=docs/decision/records/2026-10-05-quality-cli.md#A11
Scenario: warn 以上の記録を見る
Given 診断ログに INFO・WARN・ERROR の記録がある
When logs --level warn を実行する
Then WARN と ERROR の記録が出て INFO の記録は出ない
```
