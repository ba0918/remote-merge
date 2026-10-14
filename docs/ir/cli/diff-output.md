# CLI diff の差分の出し方

CLI diff がどのファイルを比べ、差分をテキストと JSON でどう出し、どの終了コードを返すか。

## Requirements

### REQ-cli-052: 比べる対象をパスで選ぶ
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A1, docs/decision/records/2026-09-30-diff-root-dir-path.md#A1
- verification: unit

diff はパスを指定しないときと、指定したパスに "."・"./"・空の値・"/" のどれかがあるときは root_dir 全体、ファイルのパスを複数指定したときはそれぞれのファイル、ディレクトリのパスを指定したときは末尾の "/" の有無によらずその配下を比べる。

### REQ-cli-078: 絶対パスと ".." の引数でコマンド全体を止める
- kind: prohibition
- source: docs/decision/records/2026-10-05-quality-cli.md#A1, docs/decision/records/2026-10-12-foundation-4.md#E17
- verification: unit

diff は、パスの引数に絶対パスか ".." の要素を含むパスがあるとき、走査と読み込みの前にコマンド全体を絶対パスなら "absolute subpath not allowed: パス"、".." なら "path traversal not allowed: パス" のエラーで止め、終了コード 2 を返す。REQ-cli-052 が root_dir 全体の指定とする "/" はこのエラーにしない。

### REQ-cli-053: JSON の形
- kind: ubiquitous
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A2, docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A5, docs/decision/records/2026-10-12-foundation-4.md#E14, docs/decision/records/2026-10-15-flag-resolution.md#A4
- verification: unit

diff の JSON は files と summary を持ち、--max-files で打ち切ったときだけ truncated と changed_files_total を、比べられなかったパスがあるときだけ path と reason を持つ errors を持つ。files の各項目は path、label と root を持つ left と right、truncated、index・left_start・right_start と type が "context"・"added"・"removed" の lines と content を持つ hunks を持ち、binary・left_hash・right_hash・note は該当するときだけ持つ。
summary の files_with_changes は、compare/judgement.md の判定が違う・片側だけ・種類の違いになったファイル（ハッシュの違うバイナリとリンク文字列か辿った先の違う symlink を含む）の数とする。
hunks の left_start・right_start は 0 始まりの行番号とする。

### REQ-cli-054: 変更の行数で打ち切る
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A3
- verification: unit

--max-lines を指定したとき、diff は追加と削除の行を数え、その数が値に達したらそれ以降の行を出さずにそのファイルの truncated を true にし、テキストでは "... (output truncated)" を出す。文脈の行は数えず、0 と指定なしは無制限とする。

### REQ-cli-055: 出すファイルの数を打ち切る
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A4, docs/decision/records/2026-09-29-diff-max-files.md#A1, docs/decision/records/2026-09-29-diff-max-files.md#A2
- verification: unit

diff の --max-files の既定は 100 で 0 は無制限とする。0 でないときはパスの指定の仕方によらずディレクトリを展開した後の変更のあるファイルを数え、出す変更のあるファイルは先頭から値の件数までとし、それを超えたときは JSON に truncated を true、changed_files_total を変更のあるファイルの総数として出し、テキストでは "... and 総数から出した変更のあるファイルの数を引いた数 more files (truncated, use --max-files 0 for all)" を出す。打ち切ったときの summary の files_with_changes とテキストの "file(s) with changes" の数は、出した変更のあるファイルの数とする。

### REQ-cli-056: 終了コード
- kind: ubiquitous
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A5
- verification: unit

diff は差分がなければ 0、差分があれば 1、エラーなら 2 を返し、比べられないファイルによって 2 を返すときも通常の出力を出す。

### REQ-cli-057: 見つからないパスを知らせる
- kind: event_driven
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A6, docs/decision/records/2026-10-12-foundation-5.md#F83
- verification: unit

diff は指定したパスが左右のどちらにもないとき、パスごとに標準エラーへ "Warning: 'パス' not found on either side" を出し、指定したパスが全て見つからなければ "specified path(s) not found on either side" のエラーで終了コード 2 を返す。左右のどちらかの一覧が件数の上限か循環で途中までのときは、一覧に載らなかったパスもないとは決められないので、見つからないパスとしない。

### REQ-cli-058: テキストの形
- kind: ubiquitous
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A7, docs/decision/records/2026-09-29-adopt-diff-output.md#A26, docs/decision/records/2026-10-12-foundation-4.md#E14, docs/decision/records/2026-10-15-flag-resolution.md#A4
- verification: unit

diff のテキストは、ファイルごとに "--- a/パス (左のラベル)" と "+++ b/パス (右のラベル)" の見出し、"@@" の行、文脈・削除・追加の行を " "・"-"・"+" の接頭辞で出し、最後に "変更のあるファイルの数 file(s) with changes out of 走査したファイルの数 total" を出す。変更のあるファイルの数は、REQ-cli-053 の files_with_changes と同じ数とする。変更の行の前後に 3 行ずつ文脈の行を出し、文脈の範囲が重なるか接する変更は一つの "@@" の塊にまとめ、最初のファイルの見出しから書き始める。
"@@" の行の行番号は 0 始まりとし、先頭の行の変更は "@@ -0,1 +0,1 @@" になる。

### REQ-cli-060: 片側にだけあるファイル
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A10
- verification: unit

片側にだけある中身のあるテキストファイルの diff は、ない側を空として全ての行を削除または追加として出す。

### REQ-cli-086: 片側にだけある 0 バイトのファイルは差分ありとする
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A10
- verification: unit

片側にだけある 0 バイトのファイルの diff は、両側を空として差分なしとせず、片側だけの項目として出して差分ありと数え、終了コード 1 を返す。

## Examples

```gherkin
@id=EX-cli-069 @about=REQ-cli-078 @source=docs/decision/records/2026-10-05-quality-cli.md#A1
Scenario: 絶対パスを含む引数で diff する
Given diff の引数に "a.txt" と "/etc/passwd" がある
When diff を実行する
Then 比較の結果を出さずに "absolute subpath not allowed: /etc/passwd" のエラーで止まり終了コード 2 を返す

@id=EX-cli-070 @about=REQ-cli-086 @source=docs/decision/records/2026-10-05-quality-cli.md#A10
Scenario: 左にだけ空のファイルがある
Given 左にだけ 0 バイトのファイル "empty.txt" がある
When "empty.txt" を指定して diff を実行する
Then 片側だけの項目が出て差分ありとされ終了コード 1 を返す
```
