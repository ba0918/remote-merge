# status の比較対象と三者比較

status が比べる左右の決め方と、参照先を加えた三者比較の表示。

## Requirements

### REQ-cli-034: 指定と既定サーバから左右を決める
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-status.md#A1
- verification: unit
- definition: TBL-cli-004

status は TBL-cli-004 に従って左右を決め、左右が同じとき、設定にないサーバ名を指定したとき、既定サーバが要るのにサーバが一つも設定されていないときはエラーで止める。
既定サーバで補ったために左右が同じになったときは、エラーにその旨を含める。

### REQ-cli-035: 参照先との違いを印で示す
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-status.md#A9, docs/decision/records/2026-10-12-foundation-4.md#E24
- verification: unit
- definition: TBL-cli-005

--ref を指定した status は、各ファイルに参照先との違いを TBL-cli-005 の印で示し、JSON では "ref" に参照先の "label" と "root"、"summary" に "ref_differs"・"ref_only"・"ref_missing" を出す。
テキストでは見出しに "(ref: 参照先の名前)" を添え、集計の後に "Ref: N differs, N ref-only, N ref-missing" の行を出す。
一覧は左右の和集合なので、参照先にだけあるファイルは diff の REQ-cli-068 と同じく一覧に載せない。

### REQ-cli-037: 左右と同じ参照先では三者比較をしない
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-status.md#A11
- verification: unit

--ref の参照先が左と同じとき、status は "Warning: --ref server is the same as left side; ref comparison skipped." を標準エラーに出し、三者比較をせずに比較を続ける。
参照先が右と同じときは "Warning: --ref server is the same as right side; ref comparison skipped." を出し、同じく三者比較をせずに続ける。

## Decision tables

### TBL-cli-004: 左右の決め方
- source: docs/decision/records/2026-09-27-adopt-status.md#A1

| --left | --right | 左 | 右 |
|---|---|---|---|
| なし | なし | "local" | 既定サーバ |
| なし | あり | "local" | --right の指定先 |
| あり | なし | --left の指定元 | 既定サーバ |
| あり | あり | --left の指定元 | --right の指定先 |

### TBL-cli-005: 参照先との違いの印
- source: docs/decision/records/2026-09-27-adopt-status.md#A9, docs/decision/records/2026-10-05-size-limit.md#A29, docs/decision/records/2026-10-05-size-limit.md#A30, docs/decision/records/2026-10-05-size-limit.md#A32, docs/decision/records/2026-10-12-foundation-4.md#E24

| 左右 | 参照先 | 中身 | JSON の "ref_badge" | テキストの印 |
|---|---|---|---|---|
| 両方にある | ない | - | "missing_in_ref" | " [ref-]" |
| 片方だけにある | ない | - | "missing_in_ref" | " [ref-]" |
| 片方だけにある | ある | - | "differs" | " [ref≠]" |
| 両方にある | ある | 三つのどれかが違う | "differs" | " [ref≠]" |
| 両方にある | ある | 三つの種類（REQ-merge-001 の通常ファイル・ディレクトリ・末尾の symlink）がそろわない。中身は読まない | "differs" | " [ref≠]" |
| 両方にある | ある | 三つとも同じ種類で、どれかを読めない | 出さない | 出さない |
| 両方にある | ある | 三つとも同じ | "all_equal" | 出さない |

## Examples

```gherkin
@id=EX-cli-099 @about=REQ-cli-035 @source=docs/decision/records/2026-10-12-foundation-4.md#E24
Scenario: 片方だけにあり参照先にもないファイルの印
Given "new.txt" は左にだけあり、右にも参照先にもない
When --ref を指定して status --format json を実行する
Then "new.txt" の ref_badge は "missing_in_ref" になる

@id=EX-cli-100 @about=REQ-cli-035 @source=docs/decision/records/2026-10-12-foundation-4.md#E24
Scenario: 三つとも同じファイルの印
Given "same.txt" は左右と参照先で中身が同じである
When --ref と --all を指定して status --format json を実行する
Then "same.txt" の ref_badge は "all_equal" になり、テキストでは印が付かない
```
