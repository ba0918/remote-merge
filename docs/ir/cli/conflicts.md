# 三者比較の衝突

参照先を基準に両側が違う変更を持つ場合の表示とマージの判断。

## Requirements

### REQ-cli-016: 三者間の競合を明示する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A39
- verification: unit

参照先に対して左右に異なる変更があるとき、比較結果に競合を示す。

### REQ-cli-017: 競合を勝手に解消しない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A39
- verification: unit

三者比較で競合がある内容を、利用者の判断なしに一方の変更だけを選んで上書きしない。

## Examples

```gherkin
@id=EX-cli-031 @about=REQ-cli-016 @source=docs/decision/records/2026-09-25-spec-migration.md#A39
Scenario: 左右が同じ箇所を異なる内容に変えた
Given 参照先に対して左右が同じ箇所を別々に編集した
When 三者比較を実行する
Then 競合が表示される

@id=EX-cli-032 @about=REQ-cli-016 @source=docs/decision/records/2026-09-25-spec-migration.md#A39
Scenario: 左右が同じ内容へ変更した
Given 参照先から左右が同じ変更をした
When 三者比較を実行する
Then その変更は競合と表示されない

@id=EX-cli-033 @about=REQ-cli-017 @source=docs/decision/records/2026-09-25-spec-migration.md#A39
Scenario: 競合があるファイルを操作する
Given 三者比較で競合が表示されている
When 利用者が競合の扱いを選ばない
Then 競合の内容は自動上書きされない

@id=EX-cli-034 @about=REQ-cli-017 @source=docs/decision/records/2026-09-25-spec-migration.md#A39,docs/decision/records/2026-09-25-spec-migration.md#A25
Scenario: 利用者が書き込みを選ぶ
Given 三者比較で競合が表示されている
When 利用者が書き込み元と先を確認して選ぶ
Then 選んだ書き込み先以外と参照先は変更されない
```
