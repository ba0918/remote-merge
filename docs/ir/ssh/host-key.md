# 未知の SSH ホスト鍵

接続先の鍵を初めて見た場合に、承認を求めるか接続を拒否する条件。

## Requirements

### REQ-ssh-001: 無確認では未知の鍵を信頼しない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A20
- verification: unit

未知の SSH ホスト鍵について利用者に確認できず、明示的な自動承認もないとき、接続を止める。

### REQ-ssh-002: 明示した自動承認は利用できる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A23
- verification: unit

利用者が --yes または strict_host_key_checking=no を明示したとき、未知の SSH ホスト鍵を自動承認して接続を続けられる。

## Examples

```gherkin
@id=EX-ssh-001 @about=REQ-ssh-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A20
Scenario: 確認できない TUI に未知のホスト鍵が届く
Given 未知の SSH ホスト鍵が提示され利用者への確認手段がない
When 明示的な自動承認なしに接続する
Then 接続は停止し未知鍵は受け入れられない

@id=EX-ssh-002 @about=REQ-ssh-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A20
Scenario: CLI で利用者が鍵を確認する
Given 未知の SSH ホスト鍵が提示され利用者が確認できる
When 利用者が鍵の承認を拒否する
Then 接続は停止する

@id=EX-ssh-003 @about=REQ-ssh-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A23
Scenario: 自動承認を明示する
Given 未知の SSH ホスト鍵が提示される
When --yes を明示して接続する
Then 接続は自動承認で続けられる

@id=EX-ssh-004 @about=REQ-ssh-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A23
Scenario: 自動承認を指定しない
Given 未知の SSH ホスト鍵が提示される
When 利用者に確認できないまま接続する
Then 接続は停止する
```
