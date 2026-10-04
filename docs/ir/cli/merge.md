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
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A3, docs/decision/records/2026-10-04-cli-command-flag-resolution.md#A6
- verification: unit

--format json の merge は merged（path・status・backup・ref_badge）、skipped（path・reason）、deleted、failed（path・error）を出し、deleted は空でも出し、ref（label と root）は参照先を使うときだけ出す。merged の status はファイル全体を書き込んだとき "ok"、--dry-run では "would merge" とする。
--force のない merge が書き込みの対象から外した機密ファイルの skipped の reason は "sensitive file" とする。

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

### REQ-cli-051: 書き込み先が参照先から変わったファイルを書かない
- kind: state_driven
- source: docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A1
- verification: unit

--ref があり参照先を実際に使い、--force も --dry-run もないファイル全体の merge は、左右と参照先の三つとも中身を読めた通常ファイル（バイナリを含む）のうち読み込み元と書き込み先の中身が違うものについて、中身をバイト列で比べて書き込み先が参照先から変わっていれば書き込まずに failed に出し、左右の両方が参照先から変わっていれば error を "three-way conflict"、書き込み先だけが変わっていれば "destination changed since reference" とし、読み込み元だけが参照先から変わったファイルは書き込む。--force があればこの確認をせずに書き込む。

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

## Examples

```gherkin
@id=EX-cli-063 @about=REQ-cli-051 @source=docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A1
Scenario: 左右が別々の箇所を変えた
Given 参照先に対して読み込み元と書き込み先が別々の行を変えたテキストのファイルがある
When --ref を指定し --force なしで merge する
Then 書き込み先は変わらず failed に error "three-way conflict" が出る

@id=EX-cli-064 @about=REQ-cli-051 @source=docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A1
Scenario: 書き込み先だけが変わった
Given 読み込み元は参照先と同じで書き込み先だけが参照先から変わったファイルがある
When --ref を指定し --force なしで merge する
Then 書き込み先は変わらず failed に error "destination changed since reference" が出る

@id=EX-cli-065 @about=REQ-cli-051 @source=docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A1
Scenario: 読み込み元だけが変わった
Given 書き込み先は参照先と同じで読み込み元だけが参照先から変わったファイルがある
When --ref を指定し --force なしで merge する
Then 書き込み先は読み込み元の中身に更新される

@id=EX-cli-066 @about=REQ-cli-051 @source=docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A1
Scenario: 強制して書く
Given 参照先に対して読み込み元と書き込み先が別々の行を変えたテキストのファイルがある
When --ref と --force を指定して merge する
Then 書き込み先は読み込み元の中身に更新される
```
