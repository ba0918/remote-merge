# 書き込み直前の更新確認

差分表示後に別の操作が書き込み先を変えたときのマージ結果。

## Requirements

### REQ-merge-011: 他者の更新を上書きしない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A35, docs/decision/records/2026-09-25-spec-migration.md#A38
- verification: unit

差分確認からマージまでに書き込み先の中身が変わったとき、サイズと更新時刻が元と同じでも、古い状態を前提としたマージは書き込みを止め、変更を報告する。

## Examples

```gherkin
@id=EX-merge-021 @about=REQ-merge-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A35
Scenario: 差分確認後に書き込み先が変わる
Given 差分を確認した後で別の操作が書き込み先を更新した
When 元の差分を使ってマージする
Then 後から行われた更新は上書きされず変更が報告される

@id=EX-merge-022 @about=REQ-merge-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A35
Scenario: 書き込み先は変わらない
Given 差分確認後も書き込み先は同じである
When 元の差分を使ってマージする
Then 書き込みが行われる

@id=EX-merge-023 @about=REQ-merge-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A38
Scenario: 更新時刻を戻した外部変更
Given 差分確認後に書き込み先の中身が変わりサイズと更新時刻は以前と同じである
When 元の差分を使ってマージする
Then 書き込み先は上書きされず内容の変更が報告される
```
