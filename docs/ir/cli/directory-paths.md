# ディレクトリを指定する CLI diff

末尾スラッシュの有無によらず、既存ディレクトリ配下の差分を表示する。include の外のディレクトリと、片側にだけあり比べるファイルのないディレクトリを指定したときの扱いも定める。

## Requirements

### REQ-cli-025: ディレクトリ指定は末尾スラッシュで変わらない
- kind: invariant
- source: docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A2, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A24
- verification: unit

既存ディレクトリまたはディレクトリへの symlink を CLI diff に指定したとき、末尾にスラッシュがあってもなくても同じリンク文字列と子ファイルを比較し、差分・終了状態を変えない。

### REQ-cli-077: include の外のディレクトリは差分なしとする
- kind: state_driven
- source: docs/decision/records/2026-10-05-ssh-directory-read.md#A1
- verification: unit

include 指定があり、include に一致しないディレクトリが左右の両方にあって、そのディレクトリを CLI diff に指定したとき、比べる経路の組と指定したパスの数によらず、その配下を include で絞った結果に比べるファイルがないものとして差分なしを報告し（テキストでは "0 file(s) with changes"）、終了コード 0 を返す。

### REQ-cli-087: 片側にだけあり比べるファイルのないディレクトリは差分なしとする
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A10, docs/decision/records/2026-09-29-adopt-diff-output.md#A7
- verification: unit

片側にだけあり、配下を include と exclude で絞ると比べるファイルがないディレクトリを CLI diff に指定したとき、"not found on either side" の警告もエラーも出さず、比べるファイル 0 件・差分なしを報告し（テキストでは "0 file(s) with changes"）、終了コード 0 を返す。
片側にだけあるディレクトリを展開した子について、ない側を空とみなす警告は走査方式によらず出さない。

### REQ-cli-096: 子の一覧を取れないディレクトリをエラーとして示す
- kind: event_driven
- source: docs/decision/records/2026-10-11-agent-operations.md#A11, docs/decision/records/2026-10-11-agent-operations.md#A20, docs/decision/records/2026-10-11-agent-operations.md#A23, docs/decision/records/2026-10-05-quality-cli.md#A5, docs/decision/records/2026-10-11-agent-operations.md#A31, docs/decision/records/2026-10-12-foundation-2.md#C12, docs/decision/records/2026-10-12-foundation-2.md#C13
- verification: unit

CLI diff は、指定したパスやその配下の子の一覧を、どの経路でも、ディレクトリではないもの（通常ファイル、ディレクトリでないものへの symlink、リンク切れの symlink、ないパス）と、一覧を取れなかったもの（読めないディレクトリ）とに分けて扱う。
一覧の途中の `接続断` は REQ-ssh-003 と REQ-ssh-023 により、一覧の中の symlink のリンク先か更新時刻を読めないときは REQ-paths-017 による。
ディレクトリではないものは、その種類に合わせて今までどおり比べる。
一覧を取れなかったものは、ファイルとしてもない側としても扱わず、そのパスと理由を JSON ではトップレベルの errors に、テキストでは標準エラーに出し、ほかに指定したパスの比較を続けて終了コード 2 を返す。
指定したディレクトリの配下を展開する走査が、読めないディレクトリ（REQ-scan-014）やリンク先を辿れない symlink（REQ-scan-018）のように走査全体を止める誤りに当たったときは、REQ-scan-020 により、その指定したパス全体を一覧を取れなかったものとし、配下のどのファイルも比べず、errors には指定したパスを、理由には止めた項目のパスを含めて出す。循環は REQ-cli-021 による。

## Examples

```gherkin
@id=EX-cli-051 @about=REQ-cli-025 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A2
Scenario: 同じディレクトリを二通りに指定する
Given 既存の src ディレクトリの子ファイルが左右で異なる
When CLI diff で src と src/ をそれぞれ指定する
Then 同じ子ファイルの差分が返り終了状態も同じになる

@id=EX-cli-052 @about=REQ-cli-025 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A2
Scenario: 差分のないディレクトリを二通りに指定する
Given 既存の src ディレクトリの配下は左右で同じである
When CLI diff で src と src/ をそれぞれ指定する
Then どちらも差分なしと報告する

@id=EX-cli-060 @about=REQ-cli-025 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A24
Scenario: ディレクトリへの symlink を二通りに指定する
Given shared は左右で異なるリンク文字列を持つディレクトリ symlink である
When CLI diff で shared と shared/ をそれぞれ指定する
Then 両方で同じリンク文字列の差と配下の差分が示される
@id=EX-cli-071 @about=REQ-cli-087 @source=docs/decision/records/2026-10-05-quality-cli.md#A10,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A2
Scenario: 片側にだけある中身の空なディレクトリを指定する
Given 左にだけ中身の空なディレクトリ "empty" がある
When CLI diff で "empty" と "empty/" をそれぞれ指定する
Then どちらも "not found on either side" を出さず差分なしで終了コード 0 を返す

@id=EX-cli-095 @about=REQ-cli-096 @source=docs/decision/records/2026-10-11-agent-operations.md#A11,docs/decision/records/2026-10-11-agent-operations.md#A31
Scenario: 読めないディレクトリを指定する
Given sudo=true を指定していないサーバへ root でない利用者でつなぎ、右の "secret" はその利用者が読めないディレクトリで、左の "secret" には "a.txt" がある
When CLI diff で "secret" を指定する
Then "secret" はファイルとしても右にないものとしても比べられず、左の "secret/a.txt" も files に出ず、"secret" と理由が errors に出て終了コード 2 を返す

@id=EX-cli-096 @about=REQ-cli-096 @source=docs/decision/records/2026-10-11-agent-operations.md#A11
Scenario: 通常ファイルを指定する
Given "a.txt" は左右とも通常ファイルで中身が違う
When CLI diff で "a.txt" を指定する
Then "a.txt" はファイルとして比べられて差分が出て、errors には何も出ない

@id=EX-cli-097 @about=REQ-cli-096 @source=docs/decision/records/2026-10-12-foundation-2.md#C12
Scenario: 指定したディレクトリの配下に読めないディレクトリがある
Given 右の "dir" の下の "dir/secret" はその利用者が読めないディレクトリで、左右の "dir/a.txt" は中身が違い、左右の "b.txt" も中身が違う
When CLI diff で "dir" と "b.txt" を指定する
Then "dir/a.txt" も "dir/secret" の配下も files に出ず、errors に "dir" が "dir/secret" を含む理由とともに出て、"b.txt" は比べられて差分が出て、終了コード 2 を返す

@id=EX-cli-098 @about=REQ-cli-096 @source=docs/decision/records/2026-10-12-foundation-2.md#C13
Scenario: 指定したディレクトリの配下にリンク先を辿れない symlink がある
Given 右の "dir" の下の "dir/loop" は自分自身を指す symlink で、左右の "dir/a.txt" は中身が違い、左右の "b.txt" も中身が違う
When CLI diff で "dir" と "b.txt" を指定する
Then "dir/a.txt" は files に出ず、errors に "dir" が "dir/loop" を含む理由とともに出て、"b.txt" は比べられて差分が出て、終了コード 2 を返す
```
