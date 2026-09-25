# 差分一覧の表示

ファイル一覧の既定表示と、全件表示・内容比較を要求する操作。

## Requirements

### REQ-cli-006: 既定では差分だけを列挙する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A21
- verification: unit

status は左右の片方にだけあるファイルと変更されたファイルを既定で列挙し、等しいファイルを省く。

### REQ-cli-007: 全件表示を選べる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A21
- verification: unit

status に --all を指定したときは、等しいファイルも一覧に含める。

### REQ-cli-008: 内容で再比較できる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A21
- verification: unit

status に --checksum を指定したときは、左右にあるファイルの内容を読み比べ、メタデータが等しくても中身が違えば変更として報告する。

## Examples

```gherkin
@id=EX-cli-011 @about=REQ-cli-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: 同じファイルを既定表示から省く
Given 左右に等しいファイルと変更されたファイルがある
When status を既定の指定で実行する
Then 差分のあるファイルだけが一覧に出る

@id=EX-cli-012 @about=REQ-cli-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: 片側にだけあるファイルを見せる
Given 左側にだけ存在するファイルがある
When status を実行する
Then そのファイルは左側のみとして一覧に出る

@id=EX-cli-013 @about=REQ-cli-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: 等しいファイルも一覧に含める
Given 左右に等しいファイルがある
When --all を指定して status を実行する
Then そのファイルも一覧に含まれる

@id=EX-cli-014 @about=REQ-cli-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: 全件指定をしない
Given 左右に等しいファイルがある
When --all なしで status を実行する
Then そのファイルは一覧に含まれない

@id=EX-cli-015 @about=REQ-cli-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: メタデータは同じで中身が違う
Given 左右のファイルのサイズと更新時刻が同じで中身が違う
When --checksum で status を実行する
Then そのファイルは変更として報告される

@id=EX-cli-016 @about=REQ-cli-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: 内容まで等しい
Given 左右のファイルの中身が等しい
When --checksum と --all で status を実行する
Then そのファイルは等しいと報告される
```
