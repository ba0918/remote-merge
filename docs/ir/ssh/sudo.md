# 権限が必要なサーバへの接続

サーバごとに権限昇格を指定したときの接続条件と失敗時の扱い。

## Requirements

### REQ-ssh-008: 明示しない接続は権限を上げない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A49
- verification: unit

サーバに sudo=true を明示しない場合、リモート操作は sudo による権限昇格を行わない。

### REQ-ssh-009: 権限昇格できない接続を切り替えない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A49
- verification: unit

sudo=true を指定したサーバで非対話の sudo が使えないときは接続・書き込みを止め、通常権限の SSH 経路には切り替えない。

## Examples

```gherkin
@id=EX-ssh-015 @about=REQ-ssh-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A49
Scenario: 権限昇格を指定していない
Given サーバに sudo=true を指定していない
When そのサーバへ接続する
Then sudo による権限昇格は行われない

@id=EX-ssh-016 @about=REQ-ssh-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A49
Scenario: 権限昇格を指定している
Given サーバに sudo=true を指定している
When 非対話の sudo を利用できるサーバへ接続する
Then 権限が必要なファイルを扱える

@id=EX-ssh-017 @about=REQ-ssh-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A49
Scenario: 権限昇格が失敗する
Given サーバに sudo=true を指定し非対話の sudo は利用できない
When 接続してマージを試みる
Then 書き込みは止まり通常権限の SSH 経路にも切り替わらない

@id=EX-ssh-018 @about=REQ-ssh-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A49
Scenario: 非対話の権限昇格が利用できる
Given サーバに sudo=true を指定し非対話の sudo が使える
When そのサーバへマージする
Then 指定された権限で書き込みが行われる
```
