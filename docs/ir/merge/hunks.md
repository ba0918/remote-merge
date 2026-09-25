# 変更のまとまりを選ぶマージ

テキストファイル全体ではなく、利用者が選んだ変更だけを書き込む操作。

## Requirements

### REQ-merge-009: 選択した変更だけを適用する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A29
- verification: unit

テキスト差分から選んだ hunk だけをマージしたとき、書き込み先にはその変更だけを適用し、選ばなかった変更は保持する。

### REQ-merge-010: 部分マージ前に衝突と書き込みを確認する
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A29
- verification: unit

hunk マージは衝突の有無と書き込みの確認を経ずに書き込み先を変更しない。

## Examples

```gherkin
@id=EX-merge-017 @about=REQ-merge-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A29
Scenario: 二つの変更のうち一つを選ぶ
Given テキスト差分に独立した二つの hunk がある
When 一方だけを選んでマージする
Then 選んだ変更だけが書き込み先に適用される

@id=EX-merge-018 @about=REQ-merge-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A29
Scenario: 全ての変更を選ぶ
Given テキスト差分に二つの hunk がある
When 両方を選んでマージする
Then 両方の変更が書き込み先に適用される

@id=EX-merge-019 @about=REQ-merge-010 @source=docs/decision/records/2026-09-25-spec-migration.md#A29
Scenario: 確認を拒否する
Given 一つの hunk が選ばれている
When 利用者が書き込み前の確認を拒否する
Then 書き込み先は変更されない

@id=EX-merge-020 @about=REQ-merge-010 @source=docs/decision/records/2026-09-25-spec-migration.md#A29
Scenario: 衝突を検出する
Given 適用先で対象行が変更されている
When 選択した hunk を適用する
Then 衝突が報告され確認なしに書き込み先は変更されない
```
