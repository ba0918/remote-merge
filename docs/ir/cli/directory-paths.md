# ディレクトリを指定する CLI diff

末尾スラッシュの有無によらず、既存ディレクトリ配下の差分を表示する。

## Requirements

### REQ-cli-025: ディレクトリ指定は末尾スラッシュで変わらない
- kind: invariant
- source: docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A2, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A24
- verification: unit

既存ディレクトリまたはディレクトリへの symlink を CLI diff に指定したとき、末尾にスラッシュがあってもなくても同じリンク文字列と子ファイルを比較し、差分・終了状態を変えない。

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
```
