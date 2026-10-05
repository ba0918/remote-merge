# バイナリファイルの比較と転送

テキスト差分を表示できないファイルの一致判定とマージ結果。

## Requirements

### REQ-cli-009: バイナリのハッシュを差分に示す
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A24, docs/decision/records/2026-10-05-quality-cli.md#A4
- verification: unit

バイナリファイルの diff はテキスト差分を出さず、中身の違うバイナリには SHA-256 のハッシュと不一致を示し、中身の同じバイナリはパスの指定の仕方と走査方式によらず項目を出さない。

### REQ-cli-010: バイナリを元のバイト列で転送する
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A24
- verification: unit

バイナリファイルをマージした後の書き込み先は、読み込み元と同じバイト列になる。

### REQ-cli-061: バイナリの判定と差分の出し方
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A9, docs/decision/records/2026-10-05-quality-cli.md#A8
- verification: unit

先頭 8,192 バイトに NUL を含むか、ファイル全体が正しい UTF-8 でないファイルをバイナリとし（先頭 8,192 バイトの境目で切れた多バイトの文字は不正としない）、diff のテキストでは "Binary files differ (left: sha256=ハッシュ, right: sha256=ハッシュ)" を出してない側や読めない側を "missing" とし、JSON に binary と left_hash・right_hash を出す。

## Examples

```gherkin
@id=EX-cli-017 @about=REQ-cli-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A24
Scenario: バイナリの内容が異なる
Given 左右にバイト列の異なるバイナリファイルがある
When diff を実行する
Then ハッシュと不一致が報告されテキストの変更行は出ない

@id=EX-cli-018 @about=REQ-cli-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A24,docs/decision/records/2026-10-05-quality-cli.md#A4
Scenario: バイナリの内容が等しい
Given 左右に同じバイト列のバイナリファイルがある
When そのファイルのパスを指定した diff と、パスを指定しない diff を実行する
Then どちらもそのファイルの項目を出さず差分なしと報告する

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
@id=EX-cli-079 @about=REQ-cli-061 @source=docs/decision/records/2026-10-05-quality-cli.md#A8,docs/decision/records/2026-09-29-adopt-diff-output.md#A9
Scenario: 8,192 バイトより後ろにだけ不正なバイトがある
Given 先頭 8,192 バイトは正しい UTF-8 で、それより後ろにだけ不正な UTF-8 のバイトがあるファイルがある
When diff を実行する
Then そのファイルはバイナリとして SHA-256 のハッシュで比べられる

@id=EX-cli-080 @about=REQ-cli-061 @source=docs/decision/records/2026-10-05-quality-cli.md#A8
Scenario: 境目で多バイトの文字が切れる
Given ファイル全体は正しい UTF-8 で、先頭 8,192 バイトの境目で多バイトの文字が切れている
When diff を実行する
Then そのファイルはテキストとして行の差分で比べられる
```
