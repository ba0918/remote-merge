# バイナリファイルの比較と転送

テキスト差分を表示できないファイルの一致判定とマージ結果。

## Requirements

### REQ-cli-009: バイナリのハッシュを差分に示す
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A24
- verification: unit

バイナリファイルの diff はテキスト差分を出さず、SHA-256 のハッシュと一致または不一致を示す。

### REQ-cli-010: バイナリを元のバイト列で転送する
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A24
- verification: unit

バイナリファイルをマージした後の書き込み先は、読み込み元と同じバイト列になる。

## Examples

```gherkin
@id=EX-cli-017 @about=REQ-cli-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A24
Scenario: バイナリの内容が異なる
Given 左右にバイト列の異なるバイナリファイルがある
When diff を実行する
Then ハッシュと不一致が報告されテキストの変更行は出ない

@id=EX-cli-018 @about=REQ-cli-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A24
Scenario: バイナリの内容が等しい
Given 左右に同じバイト列のバイナリファイルがある
When diff を実行する
Then ハッシュによる一致が分かる

@id=EX-cli-019 @about=REQ-cli-010 @source=docs/decision/records/2026-09-25-spec-migration.md#A24
Scenario: NUL を含むファイルをマージする
Given 読み込み元のファイルに NUL を含むバイト列がある
When そのバイナリファイルをマージする
Then 書き込み先のバイト列は読み込み元と一致する

@id=EX-cli-020 @about=REQ-cli-010 @source=docs/decision/records/2026-09-25-spec-migration.md#A24
Scenario: 無効なテキスト符号のバイト列を転送する
Given 読み込み元のバイナリファイルにテキストとして解釈できないバイトがある
When そのファイルをマージする
Then 書き込み先のバイト列は欠落せず読み込み元と一致する
```
