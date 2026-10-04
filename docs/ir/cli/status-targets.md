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
- source: docs/decision/records/2026-09-27-adopt-status.md#A9
- verification: unit
- definition: TBL-cli-005

--ref を指定した status は、各ファイルに参照先との違いを TBL-cli-005 の印で示し、JSON では "ref" に参照先の "label" と "root"、"summary" に "ref_differs"・"ref_only"・"ref_missing" を出す。
テキストでは見出しに "(ref: 参照先の名前)" を添え、集計の後に "Ref: N differs, N ref-only, N ref-missing" の行を出す。

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
- source: docs/decision/records/2026-09-27-adopt-status.md#A9

| 左右 | 参照先 | 中身 | JSON の "ref_badge" | テキストの印 |
|---|---|---|---|---|
| 両方にある | ない | - | "missing_in_ref" | " [ref-]" |
| 片方だけにある | ある | - | "differs" | " [ref≠]" |
| 両方にある | ある | 三つのどれかが違う | "differs" | " [ref≠]" |
