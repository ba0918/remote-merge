# 複数サーバへの同期

sync の読み込み元と書き込み先の指定、書き込み前の確認、書き込み先ごとの状態と結果の出力。

## Requirements

### REQ-cli-038: 読み込み元一つと書き込み先一つ以上を受け付ける
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-sync.md#A1
- verification: unit
- definition: TBL-cli-006

sync は --left に読み込み元を一つ、--right に書き込み先を一つ以上受け付け、TBL-cli-006 の指定ではエラーで止める。

### REQ-cli-039: 書き込み先を指定順に処理する
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-sync.md#A2
- verification: unit

sync は --right に指定した順に書き込み先を一つずつ処理する。

### REQ-cli-040: 書き込む前にまとめて一度だけ確認する
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-sync.md#A3
- verification: unit

--force も --dry-run もなく書き込む予定があるとき、sync は標準エラーに "Sync: 元 -> 先1, 先2" と書き込む予定のある書き込み先ごとの "[先] N files to merge, M files to delete" を出して "Proceed? [y/N] " と一度だけ尋ね、"y" か "Y" なら書き込み、それ以外なら "Sync cancelled." を出して何も書かずに終了コード 0 で終わる。

### REQ-cli-041: 書き込み先ごとの状態を判定する
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-sync.md#A4
- verification: unit
- definition: TBL-cli-007

sync は書き込み先ごとの status を TBL-cli-007 に従って "success"・"partial"・"failed" に判定する。

### REQ-cli-042: 全体を集計する
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-sync.md#A5
- verification: unit

sync の summary は total_servers に書き込み先の数、successful_servers に status が "success" の書き込み先の数、total_files_merged・total_files_deleted・total_files_failed に全書き込み先の merged・deleted・failed の件数の合計を出す。

### REQ-cli-043: JSON の形
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-sync.md#A6
- verification: unit

--format json の sync は left（label と root）、targets（target・merged・skipped・deleted・failed・status）、summary を出し、deleted は空でも出す。

### REQ-cli-044: 終了コード
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-sync.md#A7
- verification: unit

sync はすべての書き込み先の status が "success" なら終了コード 0、一つでも "partial" か "failed" があれば 2、エラーで止まったときは 2 を返す。

### REQ-cli-045: dry-run は書き込む予定を示す
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-sync.md#A8
- verification: unit

--dry-run の sync は書き込む予定のファイルを書き込み先ごとの merged に status "would merge" で並べ、書き込み先を変更しない。

### REQ-cli-071: 読み込み元を読めなければ全体を止める
- kind: event_driven
- source: docs/decision/records/2026-10-04-cli-command-flag-resolution.md#A4
- verification: unit

読み込み元への接続かツリーの取得に失敗した sync は、書き込み先ごとの結果を出さずにエラーで止まり、どの書き込み先も変更しない。

### REQ-cli-072: テキスト出力の形
- kind: ubiquitous
- source: docs/decision/records/2026-10-04-cli-command-flag-resolution.md#A5
- verification: unit

sync のテキスト出力は "Sync: 元 → 先1, 先2" の見出しに続けて、書き込み先ごとに "[先] 状態" の行と、書き込んだファイルを "ok"、--dry-run で書き込む予定のファイルを "plan"、スキップしたファイルを "skip"、失敗したファイルを "FAILED" で始めてパスを続けた行を出し、スキップと失敗の行にはパスの後に括弧で囲んだ理由を添える。
最後に "Summary: 成功した書き込み先の数/書き込み先の数 servers successful, N files merged" の行を出し、削除したファイルがあれば ", N files deleted"、失敗したファイルがあれば ", N files failed" を続ける。
どの書き込み先にも書き込む予定も削除する予定も失敗もないときは、見出しの前に "No files to sync." の行を出す。

## Decision tables

### TBL-cli-006: sync の指定のエラー
- source: docs/decision/records/2026-09-27-adopt-sync.md#A1, docs/decision/records/2026-10-04-cli-command-flag-resolution.md#A3

| 指定 | エラー |
|---|---|
| パスがない | 引数の解析エラー |
| --left がない | "--left is required for sync command" |
| --right がない | "--right requires at least one target server for sync command" |
| --right に同じ名前が重なる | "Duplicate target server: 名前" |
| --right の一つが --left と同じ | "--left and --right must be different (both resolved to '名前')" |
| 設定にないサーバ名を指定する | "Server '名前' not found in config" |

### TBL-cli-007: 書き込み先の状態
- source: docs/decision/records/2026-09-27-adopt-sync.md#A4

| 書き込めたファイル | 失敗 | status |
|---|---|---|
| ある | ない | "success" |
| ある | ある | "partial" |
| ない | ある | "failed" |
| ない | ない | "success" |
