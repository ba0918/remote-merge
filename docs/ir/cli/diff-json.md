# 複数ファイルの JSON 差分

ディレクトリを指定した差分表示を機械的に読み取るための出力。

## Requirements

### REQ-cli-001: ディレクトリ指定の差分を JSON で返す
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A15
- verification: unit

diff コマンドにディレクトリと JSON 形式を指定したとき、配下の複数ファイルの差分を構造化して返す。

## Examples

```gherkin
@id=EX-cli-001 @about=REQ-cli-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A15
Scenario: 二つのファイルが異なるディレクトリ
Given 指定ディレクトリの配下に差分のあるファイルが二つある
When JSON 形式で差分を取得する
Then 二つのファイルの差分が構造化された結果に含まれる

@id=EX-cli-002 @about=REQ-cli-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A15
Scenario: 差分がないディレクトリ
Given 指定ディレクトリの配下に差分がない
When JSON 形式で差分を取得する
Then 差分のないことが構造化された結果から分かる
```
