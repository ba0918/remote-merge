# status の出力と終了コード

status の結果の集計、テキストと JSON の形、集計だけの表示、終了コード。

## Requirements

### REQ-cli-029: 全ファイルを種類ごとに数える
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-status.md#A4
- verification: unit

status の集計は --all と --summary の指定によらず、"equal" を含む全ファイルを "modified"・"left_only"・"right_only"・"equal" ごとに数える。

### REQ-cli-030: 差分の有無を終了コードで返す
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-status.md#A5
- verification: unit
- definition: TBL-cli-002

status は TBL-cli-002 に従って終了コードを返す。

### REQ-cli-031: 出力形式を選び、テキストでは記号で示す
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-status.md#A6, docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A5
- verification: unit
- definition: TBL-cli-003

status は --format に "text" と "json" を受け付けて "diff" を "text" と同じに扱い、ほかの値はエラーにする。
テキストでは "Comparing: 左 ↔ 右" の見出しに続けて 1 行 1 ファイルを TBL-cli-003 の記号とパスで示し、最後に "Summary: N modified, N left only, N right only, N equal" の行を出す。

### REQ-cli-032: JSON で比較対象・ファイル・集計を返す
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-status.md#A7, docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A5
- verification: unit

--format に "json" を指定した status は "left" と "right"（"label" と "root"）、"files"（"path"・"status"）、"summary" を出し、"status" の値を "modified"・"left_only"・"right_only"・"equal" とする。

### REQ-cli-033: 集計だけを表示できる
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-status.md#A8
- verification: unit

--summary を指定した status は、テキストではファイルの行を出さずに見出しと集計だけを出し、JSON では "files" を出さない。

### REQ-cli-070: -v で右の Agent の接続状態を示す
- kind: state_driven
- source: docs/decision/records/2026-10-04-cli-command-flag-resolution.md#A1
- verification: unit

-v を指定し右がリモートの status は、JSON では "agent" に右の Agent の接続状態を "connected" か "fallback" の文字列で出し、テキストでは集計の行（--ref があれば "Ref:" の行）の後に "Agent: connected" か "Agent: fallback (SSH exec)" の行を出す。
-v がないときと右がローカルのときは、JSON の "agent" もテキストの行も出さない。

## Decision tables

### TBL-cli-002: status の終了コード
- source: docs/decision/records/2026-09-27-adopt-status.md#A5

| 結果 | 終了コード |
|---|---|
| "modified"・"left_only"・"right_only" が一件もない | 0 |
| "modified"・"left_only"・"right_only" が一件以上ある | 1 |
| エラーで止まった | 2 |

### TBL-cli-003: テキストの記号
- source: docs/decision/records/2026-09-27-adopt-status.md#A6

| 判定 | 記号 |
|---|---|
| "modified" | "M" |
| "left_only" | "L" |
| "right_only" | "R" |
| "equal" | "=" |
