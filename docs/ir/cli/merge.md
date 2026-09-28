# merge の指定と結果

merge が受け付ける指定とそのエラー、終了コード、JSON とテキストの出力、参照先を使うときの扱い。

## Requirements

### REQ-cli-046: 指定の誤りをエラーで止める
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A1
- verification: unit
- definition: TBL-cli-008

merge は TBL-cli-008 の指定ではエラーで止まり、終了コード 2 を返す。

### REQ-cli-047: 終了コード
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A2
- verification: unit

merge は failed が空なら終了コード 0、failed が一件でもあれば 2、エラーで止まったときは 2 を返す。

### REQ-cli-048: JSON の形
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A3
- verification: unit

--format json の merge は merged（path・status・backup・ref_badge）、skipped（path・reason）、deleted、failed（path・error）を出し、deleted は空でも出し、ref（label と root）は参照先を使うときだけ出す。merged の status はファイル全体を書き込んだとき "ok"、--dry-run では "would merge" とする。

### REQ-cli-049: テキスト出力の行
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A4
- verification: unit

merge のテキスト出力は、書き込んだファイルを "Merged: パス"（バックアップがあれば続けて " (backup: バックアップ)"）、--dry-run で書き込む予定のファイルを "Would merge: パス"、失敗したファイルを "Failed: パス (理由)" の行で出し、書き込む対象もスキップも失敗もないときは "no files to merge in the specified path(s)" を出す。

### REQ-cli-050: 左右と同じ参照先は使わない
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A5
- verification: unit

--ref に左と同じ指定をした merge は標準エラーに "Warning: --ref server is the same as left side; ref comparison skipped."、右と同じ指定では "Warning: --ref server is the same as right side; ref comparison skipped." を出し、参照先を使わずに続ける。

### REQ-cli-051: 競合のあるファイルを失敗として出す
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A6
- verification: unit

--ref があり --force も --dry-run もない merge は、参照先に対して左右が異なる変更をした競合のあるファイルを書き込まずに failed に error "three-way conflict" で出し、競合のない他のファイルは書き込む。

## Decision tables

### TBL-cli-008: merge の指定のエラー
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A1

| 指定 | エラー |
|---|---|
| パスがない | 引数の解析エラー |
| --left か --right がない | "--left and --right are required for merge command (e.g. --left local --right staging)" |
| --left と --right が同じ | "--left and --right must be different (both resolved to '名前')" |
| 設定にないサーバ名を指定する | "Server '名前' not found in config" |
| --format に text・json・diff 以外を指定する | "Unknown format: '値' (expected text, json, or diff)" |
