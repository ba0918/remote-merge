# 内容比較の読み取り失敗

ファイルの内容が読み取れないときに同一判定や書き込みを行わないための規則。

## Requirements

### REQ-merge-017: 読めないファイルは上書きしない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A51, docs/decision/records/2026-10-12-foundation-4.md#E7
- verification: unit

merge・sync の内容比較で元または先を読み取れないファイルは、compare/reading.md の TBL-compare-005 のとおり、同一または空の内容とみなさず、書き込みを止めてそのファイルの失敗を報告する。他のファイルは処理を続ける。

### REQ-merge-019: 読めなかった側と原因を示す
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A1, docs/decision/records/2026-10-12-foundation-4.md#E7
- verification: unit

merge・sync の中身の読み比べで読み取りに失敗したファイルの failed の error は、どのコマンドとも同じ compare/reading.md の REQ-compare-014 の形（"read failed: " に続けて、読めなかった側ごとの "left: 原因" と "right: 原因" を "; " でつないだもの）にする。

## Examples

```gherkin
@id=EX-merge-034 @about=REQ-merge-017 @source=docs/decision/records/2026-09-25-spec-migration.md#A51
Scenario: 書き込み元を読めない
Given 二つの対象のうち一件の読み込み元だけを読めない
When 両方の内容を比較してマージする
Then 読めない対象は変更されず失敗が報告され、もう一件は処理される

@id=EX-merge-035 @about=REQ-merge-017 @source=docs/decision/records/2026-09-25-spec-migration.md#A51
Scenario: 書き込み先を読めない
Given 既存の書き込み先ファイルを読み取れない
When そのファイルを同期する
Then 空の内容として扱われず書き込み先は変わらない
```
