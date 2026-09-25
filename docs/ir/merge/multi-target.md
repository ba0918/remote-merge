# 複数の書き込み先への同期

同じ元から複数サーバへ同期したときの処理継続と結果。

## Requirements

### REQ-merge-015: 複数先の結果を区別する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A42
- verification: unit

sync は複数の書き込み先を指定でき、一つの書き込み先が失敗しても他を処理し、書き込み先ごとの成功または失敗と全体の部分失敗を報告する。

## Examples

```gherkin
@id=EX-merge-030 @about=REQ-merge-015 @source=docs/decision/records/2026-09-25-spec-migration.md#A42
Scenario: 一つのサーバだけ接続できない
Given 二つの書き込み先のうち一つに接続できない
When 両方へ同期する
Then 接続できる先は更新され接続できない先の失敗が区別される

@id=EX-merge-031 @about=REQ-merge-015 @source=docs/decision/records/2026-09-25-spec-migration.md#A42
Scenario: 両方のサーバが利用できる
Given 二つの書き込み先に接続できる
When 両方へ同期する
Then 両方の先が更新されそれぞれ成功が報告される
```
