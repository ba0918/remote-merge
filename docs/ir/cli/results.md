# 非対話コマンドの結果

自動操作が成功・失敗・部分成功を読み分けるための出力。

## Requirements

### REQ-cli-018: JSON 指定は失敗時も JSON を返す
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A43, docs/decision/records/2026-10-15-flag-resolution.md#A2, docs/decision/records/2026-10-24-json-one-line.md#A7, docs/decision/records/2026-10-24-json-one-line.md#A11
- verification: unit

--format json に対応する CLI コマンドは、成功とエラーのどちらの場合も JSON として解釈できる結果を返す。
確認を断った取り消しは成功にもエラーにも当たらず、JSON を出さない。
logs は、--level・--since・--tail で絞った後に残った記録を一件ずつ JSON として返し、残った記録がなければ標準出力に何も出さずに終了コード 0 で終わる。
引数の解析で止まったとき（必須の引数がない、値の形が合わない）は、JSON を出さずに終了コード 2 で終わる。

### REQ-cli-102: JSON 指定の出力は一つの JSON を一行で書く
- kind: invariant
- source: docs/decision/records/2026-10-24-json-one-line.md#A1, docs/decision/records/2026-10-24-json-one-line.md#A2, docs/decision/records/2026-10-24-json-one-line.md#A3, docs/decision/records/2026-10-24-json-one-line.md#A4, docs/decision/records/2026-10-24-json-one-line.md#A5, docs/decision/records/2026-10-24-json-one-line.md#A6, docs/decision/records/2026-10-24-json-one-line.md#A7
- verification: unit

--format json のコマンドが結果かエラーの JSON を出すとき、標準出力には JSON を一つずつ一行に書き、各行を改行（LF）一つで終える。
JSON の中には LF を入れず、文字列の外には空白・タブ・CR・LF のどれも入れない。
行の区切りに数えるのは LF だけで、文字列の中の U+2028・U+2029・U+0085 は書き換えずにそのまま出す。
status・diff・merge・sync・rollback が JSON を出すときは、終了コードによらず、結果の JSON かエラーの JSON のどちらか一つを一行だけ標準出力に出す。
logs は、絞った後に残った記録一件ごとに一行を出し、エラーの JSON を出すときはその一行だけを標準出力に出す。

### REQ-cli-019: 部分成功は完全成功と区別する
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A43, docs/decision/records/2026-09-25-spec-migration.md#A42
- verification: unit

非対話 CLI で一部のファイルまたは同期先だけが失敗したとき、完全成功の終了コードとは異なる非ゼロ値を返す。sync は書き込み先ごとの結果も返す。

## Examples

```gherkin
@id=EX-cli-035 @about=REQ-cli-018 @source=docs/decision/records/2026-09-25-spec-migration.md#A43
Scenario: JSON 指定で成功する
Given 対象の差分を取得できる
When JSON 形式で CLI diff を実行する
Then 成功結果は JSON として解釈できる

@id=EX-cli-036 @about=REQ-cli-018 @source=docs/decision/records/2026-09-25-spec-migration.md#A43
Scenario: JSON 指定で失敗する
Given 対象の設定が無効である
When JSON 形式で CLI diff を実行する
Then 失敗結果も JSON として解釈できる

@id=EX-cli-116 @about=REQ-cli-102 @source=docs/decision/records/2026-10-24-json-one-line.md#A1,docs/decision/records/2026-10-24-json-one-line.md#A2,docs/decision/records/2026-10-24-json-one-line.md#A3,docs/decision/records/2026-10-24-json-one-line.md#A4
Scenario: 中身が複数行のファイルの差分を JSON で出す
Given 左右で中身の違うファイルがあり、その中身は LF で区切った複数の行からなる
When JSON 形式で CLI diff を実行する
Then 標準出力は LF で終わる一行だけで、その行の途中に LF はない
And その行は JSON として解釈でき、文字列の外に空白・タブ・CR を含まない

@id=EX-cli-117 @about=REQ-cli-102 @source=docs/decision/records/2026-10-24-json-one-line.md#A1,docs/decision/records/2026-10-24-json-one-line.md#A2,docs/decision/records/2026-10-24-json-one-line.md#A3
Scenario: JSON 指定の失敗も一行で出す
Given 対象の設定が無効である
When JSON 形式で CLI diff を実行する
Then 標準出力はエラーの JSON 一つを書いた、LF で終わる一行だけである
And その行は文字列の外に空白・タブ・CR を含まない

@id=EX-cli-118 @about=REQ-cli-102 @source=docs/decision/records/2026-10-24-json-one-line.md#A1,docs/decision/records/2026-10-24-json-one-line.md#A4
Scenario: 行の区切りに数えない文字は書き換えない
Given 左右で中身の違うファイルがあり、その中身に U+2028・U+2029・U+0085 を含む
When JSON 形式で CLI diff を実行する
Then 標準出力は LF で終わる一行だけで、その三つの文字は書き換えられずにその行の中に出る

@id=EX-cli-121 @about=REQ-cli-102 @source=docs/decision/records/2026-10-24-json-one-line.md#A1,docs/decision/records/2026-10-24-json-one-line.md#A2,docs/decision/records/2026-10-24-json-one-line.md#A3
Scenario: ほかのコマンドの結果も一行で出す
Given 左右で中身の違うファイルがある
When JSON 形式で CLI の status・merge・sync・rollback をそれぞれ実行する
Then どの実行でも、標準出力は LF で終わる一行だけで、その行は JSON として解釈でき、文字列の外に空白・タブ・CR を含まない

@id=EX-cli-122 @about=REQ-cli-102 @source=docs/decision/records/2026-10-24-json-one-line.md#A1,docs/decision/records/2026-10-24-json-one-line.md#A3,docs/decision/records/2026-09-25-spec-migration.md#A43
Scenario: 部分成功の結果も一行で出す
Given 二つの書き込み先のうち一つだけ失敗する
When JSON 形式で CLI sync を実行する
Then 終了コードは完全成功と異なり、標準出力は結果の JSON 一つを書いた、LF で終わる一行だけである

@id=EX-cli-119 @about=REQ-cli-102 @source=docs/decision/records/2026-10-24-json-one-line.md#A5,docs/decision/records/2026-10-24-json-one-line.md#A4
Scenario: logs は記録一件ごとに一行を出す
Given 診断ログに三件の記録がある
When JSON 形式で CLI logs を実行する
Then 標準出力は三行で、どの行も途中に LF のない JSON 一つとして解釈でき、LF で終わる

@id=EX-cli-120 @about=REQ-cli-018 @source=docs/decision/records/2026-10-24-json-one-line.md#A7
Scenario: 記録のない logs は何も出さない
Given 診断ログに記録がない
When JSON 形式で CLI logs を実行する
Then 標準出力は空で、終了コードは 0 になる

@id=EX-cli-123 @about=REQ-cli-018 @source=docs/decision/records/2026-10-24-json-one-line.md#A7
Scenario: 絞った結果が残らない logs も何も出さない
Given 診断ログには INFO の記録だけがある
When JSON 形式で CLI logs を --level error で実行する
Then 標準出力は空で、終了コードは 0 になる

@id=EX-cli-124 @about=REQ-cli-018 @source=docs/decision/records/2026-10-24-json-one-line.md#A11
Scenario: 引数の解析で止まると JSON を出さない
Given 設定は有効である
When JSON 形式で CLI merge をパスを付けずに実行する
Then 標準出力は空で、終了コードは 2 になる

@id=EX-cli-037 @about=REQ-cli-019 @source=docs/decision/records/2026-09-25-spec-migration.md#A43,docs/decision/records/2026-09-25-spec-migration.md#A42
Scenario: 同期先の一部が失敗する
Given 二つの書き込み先のうち一つだけ失敗する
When CLI sync を実行する
Then 失敗した先が個別に報告され終了コードは完全成功と異なる非ゼロになる

@id=EX-cli-038 @about=REQ-cli-019 @source=docs/decision/records/2026-09-25-spec-migration.md#A43,docs/decision/records/2026-09-25-spec-migration.md#A42
Scenario: すべて成功する
Given どのファイルも更新できる
When CLI sync を実行する
Then すべての先が成功と報告され完全成功の終了コードになる
```
