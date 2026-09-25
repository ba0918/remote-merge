# 書き込みと差分表示の安全確認

書き込み先の指定とプレビュー、機密ファイルの差分表示で利用者に求める操作。

## Requirements

### REQ-cli-002: 書き込み元と先を明示する
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A17
- verification: unit

merge と sync は書き込み元と書き込み先の明示的な指定がないとき、書き込みを開始しない。

### REQ-cli-003: 確認なしで危険な書き込みをしない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A17
- verification: unit

リモート間または機密ファイルを対象に merge・sync するとき、追加確認または利用者による明示的な強制指定がなければ書き込まない。

### REQ-cli-004: プレビューでは書き込まない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A17
- verification: unit

merge と sync の dry-run は変更予定を報告し、書き込み先を変更しない。

### REQ-cli-005: 機密ファイルの内容を隠す
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A19
- verification: unit

diff の対象が機密ファイルのとき、強制指定のない出力では内容を隠し、利用者が --force を指定した場合だけ表示する。

## Examples

```gherkin
@id=EX-cli-003 @about=REQ-cli-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A17
Scenario: 書き込み先を省略する
Given マージ対象のファイルがある
When 書き込み先を指定せず CLI merge を実行する
Then ファイルは変更されない

@id=EX-cli-004 @about=REQ-cli-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A17,docs/decision/records/2026-09-25-spec-migration.md#A53
Scenario: 書き込み元と先を明示する
Given 異なる内容の非機密の通常ファイルがローカルとリモートにあり、読み取り・確認・バックアップが成功し競合や外部更新はない
When 書き込み元と先を指定して CLI merge を実行する
Then 指定した先が更新される

@id=EX-cli-005 @about=REQ-cli-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A17
Scenario: 確認なしでサーバ間をマージする
Given 書き込み元と先が異なるリモートサーバである
When 強制指定なしに非対話の CLI merge を実行する
Then 書き込み先は変わらない

@id=EX-cli-006 @about=REQ-cli-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A17,docs/decision/records/2026-09-25-spec-migration.md#A53
Scenario: サーバ間マージを明示的に許可する
Given 書き込み元と先が異なるリモートサーバで、通常ファイルの読み取り・確認・バックアップが成功し競合や外部更新はない
When 書き込み元と先を指定し強制指定して CLI merge を実行する
Then 書き込み先は更新される

@id=EX-cli-007 @about=REQ-cli-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A17
Scenario: 差分を試算する
Given 書き込み先と元のファイルの内容が異なる
When dry-run を指定して CLI merge を実行する
Then 変更予定は報告され書き込み先の内容は変わらない

@id=EX-cli-008 @about=REQ-cli-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A17,docs/decision/records/2026-09-25-spec-migration.md#A53
Scenario: 試算せずに実行する
Given ローカルとリモートの非機密の通常ファイルの内容が異なり読み取り・確認・バックアップが成功し競合や外部更新はない
When 書き込み元と先を明示し dry-run なしで CLI merge を実行する
Then 書き込み先の内容は更新される

@id=EX-cli-009 @about=REQ-cli-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A19
Scenario: 機密ファイルの差分を通常表示する
Given 機密ファイルの内容が左右で異なる
When 強制指定なしに diff を実行する
Then 出力にファイル内容は含まれない

@id=EX-cli-010 @about=REQ-cli-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A19
Scenario: 明示的に機密ファイルの差分を表示する
Given 機密ファイルの内容が左右で異なる
When --force を指定して diff を実行する
Then ファイル内容を含む差分が返る
```
