# 三者比較の参照先

追加の参照サーバを指定した場合に、書き込み対象と参照対象を区別する規則。

## Requirements

### REQ-cli-011: 参照先を書き換えない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A25
- verification: unit

--ref で追加した参照先は比較にだけ使い、merge と sync は明示した書き込み先以外を変更しない。

## Examples

```gherkin
@id=EX-cli-021 @about=REQ-cli-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A25
Scenario: 三者で差分を見ながらマージする
Given 左右と参照先の三つに異なるファイルがある
When --ref を指定して左から右へマージする
Then 右だけが更新され参照先のファイルは変わらない

```
