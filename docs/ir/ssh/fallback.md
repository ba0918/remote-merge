# Agent が使えないときのリモート操作

リモート側の専用 Agent を利用できない場合の比較・マージの結果。

## Requirements

### REQ-ssh-005: Agent の不在で操作を失わない
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A31
- verification: unit

リモート側の Agent が利用できないときも、SSH 経路でファイルの比較とマージを行える。

## Examples

```gherkin
@id=EX-ssh-009 @about=REQ-ssh-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A31
Scenario: リモートで Agent を使えない
Given SSH 接続は可能だがリモート側の Agent は利用できない
When 差分を確認してファイルをマージする
Then SSH 経路で差分が確認でき書き込み先が更新される

@id=EX-ssh-010 @about=REQ-ssh-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A31
Scenario: Agent を利用できる
Given SSH と Agent の両方が利用可能である
When 差分を確認してファイルをマージする
Then 差分が確認でき書き込み先が更新される
```
