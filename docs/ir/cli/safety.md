# 書き込みと差分表示の安全確認

書き込み先の指定とプレビューで利用者に求める操作。

## Requirements

### REQ-cli-002: 書き込み元と先を明示する
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A17
- verification: unit

merge と sync は書き込み元と書き込み先の明示的な指定がないとき、書き込みを開始しない。

### REQ-cli-003: 確認なしで危険な書き込みをしない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A17, docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A2
- verification: unit

リモート間で merge・sync するとき、追加確認または利用者による明示的な強制指定がなければ書き込まない。

### REQ-cli-004: プレビューでは書き込まない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A17
- verification: unit

merge と sync の dry-run は変更予定を報告し、書き込み先を変更しない。

### REQ-cli-076: サブコマンドなしの起動は使い方を示して止まる
- kind: event_driven
- source: docs/decision/records/2026-10-04-tui-disposition.md#A2, docs/decision/records/2026-10-04-tui-disposition.md#A19, docs/decision/records/2026-10-04-tui-disposition.md#A24
- verification: unit

サブコマンドを指定せずに起動したときは、グローバルなフラグだけを付けた場合も含め、比較も診断ログを含む書き込みもせずに標準エラーへ使い方を出し、終了コード 2 で終わる。
--help と --version はこの扱いから除き、使い方と版を標準出力に出して終了コード 0 で終わる。

## Examples

```gherkin
@id=EX-cli-003 @about=REQ-cli-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A17
Scenario: 書き込み先を省略する
Given マージ対象のファイルがある
When 書き込み先を指定せず CLI merge を実行する
Then ファイルは変更されない

@id=EX-cli-004 @about=REQ-cli-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A17,docs/decision/records/2026-09-25-spec-migration.md#A53,docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A8
Scenario: 書き込み元と先を明示する
Given 異なる内容の通常ファイルがローカルとリモートにあり、読み取り・確認・バックアップが成功し競合や外部更新はない
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

@id=EX-cli-008 @about=REQ-cli-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A17,docs/decision/records/2026-09-25-spec-migration.md#A53,docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A8
Scenario: 試算せずに実行する
Given ローカルとリモートの通常ファイルの内容が異なり読み取り・確認・バックアップが成功し競合や外部更新はない
When 書き込み元と先を明示し dry-run なしで CLI merge を実行する
Then 書き込み先の内容は更新される
```
