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
@id=EX-cli-071 @about=REQ-cli-087 @source=docs/decision/records/2026-10-05-quality-cli.md#A10
Scenario: 片側にだけある中身の空なディレクトリを指定する
Given 左にだけ中身の空なディレクトリ "empty" がある
When CLI diff で "empty" と "empty/" をそれぞれ指定する
Then どちらも "not found on either side" を出さず差分なしで終了コード 0 を返す
```
