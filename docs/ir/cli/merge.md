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
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A2, docs/decision/records/2026-10-12-foundation-4.md#E25
- verification: unit

merge は failed が空で種類の違い（REQ-merge-001）でスキップしたパスもなければ終了コード 0、failed か種類の違いのスキップが一件でもあれば 2、エラーで止まったときは 2 を返す。
--delete なしで書き込み先だけのファイルをスキップしたこと（REQ-merge-024）は、終了コードを変えない。

### REQ-cli-048: JSON の形
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A3, docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A2, docs/decision/records/2026-10-05-size-limit.md#A29, docs/decision/records/2026-10-05-size-limit.md#A30
- verification: unit

--format json の merge は merged（path・status・backup・ref_badge）、skipped（path・reason）、deleted、failed（path・error）を出し、deleted は空でも出し、ref（label と root）は参照先を使うときだけ出す。merged の status はファイル全体を書き込んだとき "ok"、--dry-run では "would merge" とする。
merged の ref_badge は、--force の有無によらず TBL-cli-005 に従う（三つとも同じ種類であり、どれかの中身を読めなかったファイルには出さない）。

### REQ-cli-049: テキスト出力の行
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A4, docs/decision/records/2026-10-04-cli-command-flag-resolution.md#A7
- verification: unit

merge のテキスト出力は、書き込んだファイルを "Merged: パス"（バックアップがあれば続けて " (backup: バックアップ)"）、--dry-run で書き込む予定のファイルを "Would merge: パス"、失敗したファイルを "Failed: パス (理由)" の行で出し、書き込む対象もスキップも失敗もないときは "no files to merge in the specified path(s)" を出す。
スキップしたファイルは、失敗したファイルの行より前に "Skipped: パス (理由)" の行で出す。

### REQ-cli-050: 左右と同じ参照先は使わない
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A5
- verification: unit

--ref に左と同じ指定をした merge は標準エラーに "Warning: --ref server is the same as left side; ref comparison skipped."、右と同じ指定では "Warning: --ref server is the same as right side; ref comparison skipped." を出し、参照先を使わずに続ける。

### REQ-cli-051: 書き込み先が参照先から変わったファイルを書かない
- kind: state_driven
- source: docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A1, docs/decision/records/2026-10-12-foundation-4.md#E6, docs/decision/records/2026-10-12-foundation-4.md#E10
- verification: unit

--ref があり参照先を実際に使い、--force のないファイル全体の merge は、--dry-run かどうかによらず（compare/flow.md の REQ-compare-005）、左右と参照先の三つとも中身を読めた通常ファイル（バイナリを含む）か三つとも末尾の symlink のもののうち読み込み元と書き込み先が違うものについて、通常ファイルは中身のバイト列で、symlink はリンク文字列で（compare/judgement.md の REQ-compare-008）比べて書き込み先が参照先から変わっていれば書き込まずに failed に出し、左右の両方が参照先から変わっていれば error を "three-way conflict"、書き込み先だけが変わっていれば "destination changed since reference" とし、読み込み元だけが参照先から変わったファイルは書き込む。--force があればこの確認をせずに書き込む。

### REQ-cli-073: 強制指定のないリモート間の merge を止める
- kind: state_driven
- source: docs/decision/records/2026-10-04-cli-command-flag-resolution.md#A8
- verification: unit

--force も --dry-run もないリモート間の merge は、確認を出さずに書き込まずに止まり、テキストでは "Warning: merging between two remote servers (左 → 右)" と "Use --force to proceed, or --dry-run to preview changes." の行を出し、JSON では failed に一件を出し、終了コード 2 を返す。

### REQ-cli-074: 確認のプロンプトを出さず --force で対象と確認を変える
- kind: ubiquitous
- source: docs/decision/records/2026-10-04-cli-command-flag-resolution.md#A9, docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A6, docs/decision/records/2026-10-05-quality-cli.md#A2
- verification: unit

CLI の merge は書き込む前に確認のプロンプトを出さない。
--force を指定した merge は、リモート間の merge を止めず、REQ-cli-051 の参照先に対する確認をせず、REQ-cli-079 のファイルの大きさの上限を当てない。

### REQ-cli-092: 三者の存在の組み合わせで書くかを決める
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A9, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A1, docs/decision/records/2026-10-05-quality-cli.md#A13, docs/decision/records/2026-10-05-quality-cli.md#A16, docs/decision/records/2026-10-05-size-limit.md#A24, docs/decision/records/2026-10-05-size-limit.md#A30, docs/decision/records/2026-10-12-foundation-4.md#E10, docs/decision/records/2026-10-12-foundation-4.md#E3
- verification: unit
- definition: TBL-cli-009

--ref があり参照先を実際に使い、--force のないファイル全体の merge は、--dry-run かどうかによらず、左右と参照先にファイルがないこと（それぞれの側の状態から決める。compare/flow.md の REQ-compare-003）と読み込みに失敗したことを区別し、TBL-cli-009 に従って書くかどうかを決め、ないと分かっている側の中身は読みに行かない。
読み込み元と書き込み先の種類が違うファイルは、TBL-cli-009 によらず REQ-merge-001 に従う。
TBL-cli-009 が文言を決めていない failed の error は実装者が決め、テストはその文言を固定しない。

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

### TBL-cli-009: 参照先を使う merge の三者の存在
- source: docs/decision/records/2026-10-05-quality-cli.md#A9, docs/decision/records/2026-10-05-quality-cli.md#A13, docs/decision/records/2026-10-05-quality-cli.md#A16, docs/decision/records/2026-10-05-size-limit.md#A23, docs/decision/records/2026-10-05-size-limit.md#A30, docs/decision/records/2026-10-05-size-limit.md#A32

| 読み込み元 | 書き込み先 | 参照先 | 扱い |
|---|---|---|---|
| ある | ない | ない | --force なしで書き込む |
| ある | ない | ある | 書かずに failed に error "destination changed since reference" で出す |
| ある | ある | ある | REQ-cli-051 に従う |
| ある | ある（読み込み元と同じ種類） | ある（読み込み元と種類が違う。種類は REQ-merge-001 の通常ファイル・ディレクトリ・末尾の symlink） | 中身を読まずに、書かずに failed に error "three-way conflict" で出す |
| ある | ある | ない | 書き込み先が変わっていないことを確かめられないので、書かずに failed に出す（error の文言は実装者が決める） |
| 三つのどれかの読み込みに失敗した | - | - | 書かずに failed に error "three-way comparison incomplete" で出す。--dry-run でも同じく failed に出し、参照先との関係のバッジを出さない |

## Examples

```gherkin
@id=EX-cli-101 @about=REQ-cli-047 @source=docs/decision/records/2026-10-12-foundation-4.md#E25
Scenario: 種類の違いでスキップしたパスのある merge
Given 読み込み元の "x" は通常ファイルで書き込み先の "x" はディレクトリであり、"a.txt" は書き込める
When "x" と "a.txt" を merge する
Then "a.txt" は書き込まれ、"x" は skipped に出て、終了コード 2 を返す

@id=EX-cli-102 @about=REQ-cli-047 @source=docs/decision/records/2026-10-12-foundation-4.md#E25
Scenario: 書き込み先だけのファイルのスキップは終了コードを変えない
Given 書き込み先にだけ "extra.txt" があり、ほかのファイルは書き込める
When --delete なしで root_dir 全体を merge する
Then "extra.txt" は skipped に出て、終了コード 0 を返す

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
@id=EX-cli-084 @about=REQ-cli-092 @source=docs/decision/records/2026-10-05-quality-cli.md#A9
Scenario: 左にだけある新しいファイルを参照先付きで書く
Given "new.txt" は左にだけあり、右にも参照先にもない
When --ref を指定し --force なしで左から右へ "new.txt" を merge する
Then 右に "new.txt" が作られる

@id=EX-cli-085 @about=REQ-cli-092 @source=docs/decision/records/2026-10-05-quality-cli.md#A9
Scenario: 参照先にあり書き込み先から消えたファイル
Given "a.txt" は左と参照先にあり右にない
When --ref を指定し --force なしで左から右へ "a.txt" を merge する
Then 右は変わらず failed に error "destination changed since reference" が出る

@id=EX-cli-094 @about=REQ-cli-092 @source=docs/decision/records/2026-10-05-quality-cli.md#A13,docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A1
Scenario: 左右にあり参照先にないファイル
Given "b.txt" は左と右にあり中身が違い、参照先にない
When --ref を指定し --force なしで左から右へ "b.txt" を merge する
Then 右の "b.txt" は変わらず、"b.txt" は failed に出る
```
