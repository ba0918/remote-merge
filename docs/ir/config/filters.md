# ファイルの対象フィルター

ツリー表示と比較・書き込みの対象からパスを選ぶ規則。

## Requirements

### REQ-config-003: 除外は表示と操作の両方に効く
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A28
- verification: unit

除外パターンに一致するファイルは、ツリー・status・merge・sync の走査対象から外す。

### REQ-config-004: include は対象を限定する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A28
- verification: unit

include 指定があるとき、ツリー・status・merge・sync は一致するパスだけを対象にする。

## Examples

```gherkin
@id=EX-config-005 @about=REQ-config-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A28
Scenario: 除外したファイルがある
Given 除外パターンに一致するファイルがある
When status と merge を実行する
Then そのファイルは一覧にも書き込み対象にも含まれない

@id=EX-config-006 @about=REQ-config-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A28
Scenario: 除外しないファイルがある
Given 除外パターンに一致しない変更済みファイルがあり include は指定されていない
When status を実行する
Then そのファイルは対象になる

@id=EX-config-007 @about=REQ-config-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A28
Scenario: include で対象を絞る
Given include に一致するファイルと一致しないファイルがある
When status と sync を実行する
Then 一致するファイルだけが一覧と同期の対象になる

@id=EX-config-008 @about=REQ-config-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A28
Scenario: include を指定しない
Given include に指定がない
When ファイル一覧を取得する
Then include による絞り込みは行われない
```
