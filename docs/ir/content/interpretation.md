# ファイル内容の文字コードとバイト列

ファイル内容を文字として解釈する規則、BOM の扱い、差分と競合の解釈の一致、および文字列とは分離した元バイト列の保持。
文字コードの選択は config/content.md に、部分マージの適用は merge/hunks.md に置く。

## Requirements

### REQ-content-001: 指定と内容からテキストかバイナリかを決める
- kind: algorithm
- source: docs/decision/records/2026-10-16-content-encoding.md#A1, docs/decision/records/2026-10-16-content-encoding.md#A10, docs/decision/records/2026-10-16-content-encoding.md#A13, docs/decision/records/2026-10-16-content-encoding.md#D2
- verification: unit
- definition: TBL-content-001

### REQ-content-002: 解釈を比較の用途によらず一致させる
- kind: invariant
- source: docs/decision/records/2026-10-16-content-encoding.md#A3, docs/decision/records/2026-10-16-content-encoding.md#A16, docs/decision/records/2026-10-16-content-encoding.md#A17
- verification: unit

同じ文字コード指定と同じ元バイト列には、左右の差分、参照先との差分、行単位の競合判定、部分マージで同じデコード、行分割、hunk の区切りを使う。
左右と参照先をそれぞれの文字コードで解釈できれば、文字コードが異なっても行の差と競合を求める。
行の比較には LF と CRLF の違いと末尾改行の有無を含める。

### REQ-content-003: BOM を本文と分ける
- kind: invariant
- source: docs/decision/records/2026-10-16-content-encoding.md#A11
- verification: unit

UTF-8 の先頭の BOM は本文の行に含めず、BOM だけの違いを行の変更として示さない。
部分マージは既存の書き込み先の BOM の有無を保ち、全体コピーは BOM を含む読み込み元の全バイトを転送する。

### REQ-content-004: 元バイト列による一致判定と保存を変えない
- kind: invariant
- source: docs/decision/records/2026-10-16-content-encoding.md#A2, docs/decision/records/2026-10-16-content-encoding.md#P2, docs/decision/records/2026-10-12-foundation-4.md#E5
- verification: unit

文字コード指定によって、元バイト列の一致判定、SHA-256、書き込み直前の更新検知、全体 merge と sync の転送、バックアップと rollback のバイト列を変えない。
バックアップ後に文字コード設定が変わっても、rollback は保存した元バイト列を復元する。

### REQ-content-005: 表現だけの違いを差分として示す
- kind: state_driven
- source: docs/decision/records/2026-10-16-content-encoding.md#A5, docs/decision/records/2026-10-16-content-encoding.md#A7, docs/decision/records/2026-10-16-content-encoding.md#A11, docs/decision/records/2026-10-16-content-encoding.md#D3
- verification: unit

左右の元バイト列が違い、BOM を除いたデコード後の本文が同じなら、diff は差分ありとして数え、行の hunks は空にし、JSON の "byte_only_difference" を true として元バイト列のハッシュと note を示し、テキストでも表現上の差であることを示す。
この差だけを部分マージしても本文の変更はなく、BOM や未変更の本文のバイト列を書き換えない。

### REQ-content-006: 明示した文字コードのデコード失敗を隠さない
- kind: state_driven
- source: docs/decision/records/2026-10-16-content-encoding.md#A6, docs/decision/records/2026-10-16-content-encoding.md#A13, docs/decision/records/2026-10-16-content-encoding.md#A19
- verification: unit

明示した文字コードでデコードに失敗した場合、diff はパスと側と文字コードと理由を示し、他のファイルの比較を続けて終了コード 2 を返す。
部分マージに必要な内容のデコードに失敗した場合は、--dry-run と実行時のどちらも書き込み前に止まり、--force でも不正なデコードを使用しない。
失敗した内容を空や置換文字に置き換えず、参照先を解釈できないことを競合なしとしない。
JSON ではトップレベルの "errors" にパスを出して理由に側と文字コードを含め、左右の失敗ならファイル項目と差分件数から外す。
参照先だけの失敗なら左右の差分を残し、参照先との差と競合結果を出さない。

### REQ-content-007: 使用文字コードをファイルごとに示す
- kind: ubiquitous
- source: docs/decision/records/2026-10-16-content-encoding.md#A4, docs/decision/records/2026-10-16-content-encoding.md#D3
- verification: unit

diff の JSON のテキストとして解釈したファイル項目は "encodings" に側ごとの "left"、"right"、"ref" の文字コードを示し、存在しない側を省く。
文字コード名は "utf-8"、"euc-jp"、"shift_jis" に正規化する。

### REQ-content-008: 全体コピーが変換しないことを示す
- kind: ubiquitous
- source: docs/decision/records/2026-10-16-content-encoding.md#A15
- verification: unit

merge と sync のヘルプと全体コピーの --dry-run の結果は、全体コピーが読み込み元のバイト列をそのまま転送し、内容の文字コード指定で変換しないことを示す。

## Decision tables

### TBL-content-001: 内容の解釈
- source: docs/decision/records/2026-10-16-content-encoding.md#A1, docs/decision/records/2026-10-16-content-encoding.md#A10, docs/decision/records/2026-10-16-content-encoding.md#A13, docs/decision/records/2026-10-16-content-encoding.md#D2

上から最初に一致する行を使う。

| 指定と内容 | 結果 |
|---|---|
| 指定が "binary" | バイナリ |
| 先頭 8,192 バイトに NUL がある | バイナリ |
| 文字コードが未設定で、全体が正しい UTF-8 | UTF-8 のテキスト |
| 文字コードが未設定で、全体が正しい UTF-8 ではない | バイナリ |
| 文字コードが明示され、全体を指定した文字コードでデコードできる | その文字コードのテキスト |
| 文字コードが明示され、全体を指定した文字コードでデコードできない | デコード失敗 |

## Examples

```gherkin
@id=EX-content-001 @about=REQ-content-001 @source=docs/decision/records/2026-10-16-content-encoding.md#A1,docs/decision/records/2026-10-16-content-encoding.md#A13
Scenario: EUC-JP の指定でテキスト差分を出す
Given 左右に内容の違う EUC-JP のファイルがあり両側に EUC-JP が指定されている
When diff する
Then 日本語の変更行が示されバイナリとはされない

@id=EX-content-002 @about=REQ-content-001 @source=docs/decision/records/2026-10-16-content-encoding.md#A13,docs/decision/records/2026-09-25-spec-migration.md#A24
Scenario: 未設定の非 UTF-8 は従来どおり扱う
Given 文字コードの指定がなく UTF-8 ではない内容がある
When diff する
Then バイナリとして元バイト列のハッシュが示される

@id=EX-content-003 @about=REQ-content-001,REQ-content-006 @source=docs/decision/records/2026-10-16-content-encoding.md#A13
Scenario: 指定文字コードで読めない末尾を置換しない
Given EUC-JP が明示されたファイルの 8,192 バイトより後ろに不正な EUC-JP のバイトがある
When diff する
Then デコード失敗のパスと側と文字コードと理由を示し終了コード 2 を返す

@id=EX-content-004 @about=REQ-content-002 @source=docs/decision/records/2026-10-16-content-encoding.md#A3,docs/decision/records/2026-10-16-content-encoding.md#A16,docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A11
Scenario: 参照先だけが別の文字コードである
Given 左右は EUC-JP で参照先は UTF-8 と指定され左右が参照先の同じ行を異なる内容へ変えている
When diff と参照先を使う部分マージで競合を調べる
Then 両方が同じ行の競合を検出し部分マージは書き込まない

@id=EX-content-005 @about=REQ-content-003,REQ-content-005 @source=docs/decision/records/2026-10-16-content-encoding.md#A7,docs/decision/records/2026-10-16-content-encoding.md#A11,docs/decision/records/2026-10-16-content-encoding.md#D3
Scenario: BOM だけが異なる
Given 左右は UTF-8 で本文が同じだが片側だけに BOM がある
When diff する
Then 行の hunks は空で表現上の差が示され差分ありになる

@id=EX-content-006 @about=REQ-content-004 @source=docs/decision/records/2026-10-16-content-encoding.md#A2,docs/decision/records/2026-10-12-foundation-4.md#E9
Scenario: 同じ文字へ読める別のバイトも更新として検知する
Given 比較後に書き込み先のバイトが変わったがデコード後の文字は同じである
When 書き込み直前の更新を確認する
Then 元バイト列の変化として検知して書き込まない

@id=EX-content-007 @about=REQ-content-004 @source=docs/decision/records/2026-10-16-content-encoding.md#A2
Scenario: 設定を変更してもバックアップを元どおり戻す
Given EUC-JP の元バイト列をバックアップした後でそのパスの文字コード設定を UTF-8 に変更した
When rollback する
Then 保存した EUC-JP の元バイト列がそのまま復元される

@id=EX-content-008 @about=REQ-content-005,REQ-content-007 @source=docs/decision/records/2026-10-16-content-encoding.md#A7,docs/decision/records/2026-10-16-content-encoding.md#D3
Scenario: 同じ文章の文字コードだけが異なる
Given 元バイト列が異なる同じ日本語の文章の左が UTF-8 で右が EUC-JP と指定されている
When JSON の diff を出す
Then "encodings" に左右の文字コードが示され "byte_only_difference" は true で元バイトのハッシュが示される

@id=EX-content-009 @about=REQ-content-006 @source=docs/decision/records/2026-10-16-content-encoding.md#A6,docs/decision/records/2026-10-16-content-encoding.md#A13
Scenario: 読めない参照先で競合なしとしない
Given 左右は有効なテキストで参照先は明示された EUC-JP として読めない
When --ref を使って diff と --force のない部分マージを行う
Then diff はデコード失敗を示し部分マージは書き込まず停止する

@id=EX-content-010 @about=REQ-content-008 @source=docs/decision/records/2026-10-16-content-encoding.md#A15
Scenario: 全体コピーが変換しないことを事前に示す
Given 読み込み元は EUC-JP で書き込み先の内容設定は UTF-8 である
When merge と sync の全体コピーを --dry-run する
Then 元バイトをそのまま転送して文字コードは変換しないことが結果に示される

@id=EX-content-011 @about=REQ-content-002 @source=docs/decision/records/2026-10-16-content-encoding.md#A17,docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A4
Scenario: 改行だけの変更と同じ行の本文変更が競合する
Given 参照先は CRLF で左は LF に変え右は同じ行の本文を変えている
When 行の変更を比較して三者の競合を調べる
Then 改行の変更も行の変更として同じ行の競合になる

@id=EX-content-012 @about=REQ-content-002,REQ-merge-045 @source=docs/decision/records/2026-10-16-content-encoding.md#A13,docs/decision/records/2026-10-16-content-encoding.md#A18
Scenario: 参照先だけがバイナリでも競合を失わない
Given 左右は同じ文字コードのテキストで参照先の先頭 8,192 バイトに NUL があり三つの元バイト列は全て異なる
When diff と --force のない --dry-run の部分マージで三者の競合を調べる
Then diff はファイル全体で一つの競合を示し部分マージは書き込まず止まる

@id=EX-content-013 @about=REQ-content-006 @source=docs/decision/records/2026-10-16-content-encoding.md#A19
Scenario: 参照先だけのデコード失敗でも左右の差分を残す
Given 左右の差分は読めるが参照先は指定した文字コードでデコードできない
When JSON の diff を出す
Then 左右の差分とその差分件数は残り参照先との差と競合結果は出ず "errors" に理由が出て終了コード 2 になる
```
