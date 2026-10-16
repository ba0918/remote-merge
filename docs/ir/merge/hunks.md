# 変更のまとまりを選ぶマージ

テキストファイル全体ではなく、利用者が選んだ変更だけを書き込む操作。

## Requirements

### REQ-merge-009: 選択した変更だけを適用する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A29
- verification: unit

テキスト差分から選んだ hunk だけをマージしたとき、書き込み先にはその変更だけを適用し、選ばなかった変更は保持する。

### REQ-merge-028: --hunks の指定の誤りをエラーで止める
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-hunks.md#A1
- verification: unit
- definition: TBL-merge-001

--hunks の merge は TBL-merge-001 の指定ではエラーで止まり、終了コード 2 を返す。

### REQ-merge-029: --hunks の JSON の形
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-hunks.md#A2, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A3
- verification: unit

--format json の --hunks の merge は merged の一件に、書き込んだとき status "merged"、--dry-run では status "would merge"、hunks_applied に指定した番号、hunks_total に REQ-merge-032 の区切りで数えた hunk の数、direction に "left_to_right"、バックアップが有効なときだけ backup を出す。

### REQ-merge-030: --hunks のテキストの行
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-hunks.md#A3, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A3, docs/decision/records/2026-10-12-foundation-5.md#F13
- verification: unit

--hunks の merge のテキスト出力は、総数を REQ-merge-032 の区切りで数え、書き込んだファイルを "Merged: パス (hunks: 番号,番号/総数)"（バックアップがあれば続けて " (backup: バックアップ)"）、--dry-run では "Would merge: パス (hunks: 番号,番号/総数)" の行で出す。
差分のないファイルは書き込まず、テキストでは書き込んだときの行を出さずに "Skipped: パス (no changes)" の行で出し、終了コード 0 を返す。

### REQ-merge-031: 三者の競合でエラーで止める
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-merge-hunks.md#A4, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A4, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A11
- verification: unit

--ref があり --force のない --hunks の merge は、選んだ hunk の中に限らずファイルのどこかに参照先に対して左右が同じ箇所を異なる内容に変えた競合があれば、そのファイルを書き込まずに "three-way conflict: パス" のエラーで止め、--dry-run でも同じように止める。

### REQ-merge-032: --hunks の番号は diff の hunk で数える
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A3, docs/decision/records/2026-10-16-content-encoding.md#A3
- verification: unit

--hunks の番号は、同じ --left・--right・パスと同じ文字コード指定で実行した diff --format json の hunks と同じ区切り（変更の前後 3 行の文脈でまとめたもの）を 0 から数えたものとし、hunk の数と番号の範囲外の判定もこの区切りで数え、一つの区切りに複数の変更のかたまりが入るとき、その番号を選べばその全てを適用する。

### REQ-merge-045: 部分マージは解釈可能な同じ文字コード間に限る
- kind: state_driven
- source: docs/decision/records/2026-10-16-content-encoding.md#A5, docs/decision/records/2026-10-16-content-encoding.md#A6, docs/decision/records/2026-10-16-content-encoding.md#A16, docs/decision/records/2026-10-16-content-encoding.md#A18, docs/decision/records/2026-10-16-content-encoding.md#P1
- verification: unit

左右の両方に通常ファイルがある部分マージは、content/interpretation.md の解釈で両側がテキストとなり、使用する文字コードが同じ場合に限る。
文字コードの別名は同じ文字コードとし、異なる文字コードの組とデコード失敗では --force でも書き込まず、--dry-run でも同じように止める。
参照先を使うときはその側の文字コードで解釈し、参照先の文字コードが左右と異なるだけでは拒否しない。
参照先がバイナリなら REQ-cli-016 のファイル全体の競合判定を使い、競合があれば --dry-run でも止める。--force は参照先の確認だけを省略でき、左右の解釈の制約は省略しない。

### REQ-merge-046: 未選択箇所の元バイトと改行を保持する
- kind: invariant
- source: docs/decision/records/2026-10-16-content-encoding.md#A5, docs/decision/records/2026-10-16-content-encoding.md#A11
- verification: unit

部分マージは選んだ変更に読み込み元のバイト列を使い、選んでいない箇所には書き込み先の元バイト列を残す。
LF、CRLF、混在した改行、末尾改行の有無を含めて、全文の再エンコードや改行の正規化で未変更箇所を書き換えない。
BOM は REQ-content-003 に従う。

### REQ-merge-047: 存在しない側に文字コードを仮定しない
- kind: state_driven
- source: docs/decision/records/2026-10-16-content-encoding.md#A14
- verification: unit

片側のファイルがない部分マージは、ない側には文字コードを持たせず、存在する側の文字コードで空の内容として比較する。
新規作成する書き込み先には読み込み元の文字コードと BOM を引き継ぎ、存在しない書き込み先に別の文字コードの設定があっても変換しない。

## Decision tables

### TBL-merge-001: --hunks の指定のエラー
- source: docs/decision/records/2026-09-28-adopt-merge-hunks.md#A1, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A6

| 指定 | エラー |
|---|---|
| パスが二つ以上 | "--hunks requires exactly one path (got N)" |
| --delete と併せる | "--hunks and --delete cannot be used together" |
| 番号が hunk の数以上 | "Hunk index N is out of range (total hunks: M)" |
| 読み込み元か書き込み先が symlink | "Hunk merge is not supported for symlink files: 'パス'" |
| 読み込み元か書き込み先がバイナリ | "Hunk merge is not supported for binary files: 'パス'" |

## Examples

```gherkin
@id=EX-merge-068 @about=REQ-merge-030 @source=docs/decision/records/2026-10-12-foundation-5.md#F13
Scenario: 差分のないファイルを --hunks で指定する
Given 左右の "a.txt" は同じ中身である
When --hunks 0 で "a.txt" を merge する
Then テキストでは "Skipped: a.txt (no changes)" の行が出て "Merged: a.txt" は出ず、終了コード 0 を返す

@id=EX-merge-017 @about=REQ-merge-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A29
Scenario: 二つの変更のうち一つを選ぶ
Given テキスト差分に独立した二つの hunk がある
When 一方だけを選んでマージする
Then 選んだ変更だけが書き込み先に適用される

@id=EX-merge-018 @about=REQ-merge-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A29
Scenario: 全ての変更を選ぶ
Given テキスト差分に二つの hunk がある
When 両方を選んでマージする
Then 両方の変更が書き込み先に適用される

@id=EX-merge-038 @about=REQ-merge-032 @source=docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A3
Scenario: 近い二つの変更は一つの hunk になる
Given 変わらない行を 2 行だけ挟んだ二つの変更があるテキストのファイルがある
When diff --format json で hunk を調べ --hunks 0 で merge する
Then diff は hunk を一つだけ示し、merge は二つの変更の両方を書き込み先に適用する

@id=EX-merge-039 @about=REQ-merge-032 @source=docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A3
Scenario: 離れた二つの変更は別々の hunk になる
Given 変わらない行を 7 行以上挟んだ二つの変更があるテキストのファイルがある
When --hunks 1 で merge する
Then 二つ目の変更だけが書き込み先に適用される

@id=EX-merge-069 @about=REQ-merge-045,REQ-merge-046,REQ-merge-032 @source=docs/decision/records/2026-10-16-content-encoding.md#A3,docs/decision/records/2026-10-16-content-encoding.md#A5
Scenario: EUC-JP の表示番号で部分マージする
Given 左右は EUC-JP と指定され独立した変更が二つある
When JSON の diff で調べた二つ目の hunk だけを部分マージする
Then その変更だけが EUC-JP の元バイトで適用され未選択箇所の元バイトは変わらない

@id=EX-merge-070 @about=REQ-merge-045 @source=docs/decision/records/2026-10-16-content-encoding.md#A5,docs/decision/records/2026-10-16-content-encoding.md#P1
Scenario: 異なる文字コード間の部分マージを拒否する
Given 左右の既存ファイルはそれぞれ UTF-8 と EUC-JP と指定されている
When --force または --dry-run を指定して部分マージする
Then 異なる文字コードである理由を示して止まりファイルは変わらない

@id=EX-merge-071 @about=REQ-merge-046 @source=docs/decision/records/2026-10-16-content-encoding.md#A5
Scenario: 混在改行と未選択の拡張文字を保持する
Given 左右は CP932 と指定され書き込み先の未選択箇所には拡張文字と LF と CRLF の混在がある
When 別の箇所の hunk を部分マージする
Then 未選択箇所の拡張文字と改行を含む元バイト列がそのまま残る

@id=EX-merge-072 @about=REQ-merge-047 @source=docs/decision/records/2026-10-16-content-encoding.md#A14
Scenario: 未作成の書き込み先に読み込み元の文字コードを引き継ぐ
Given 読み込み元は EUC-JP で書き込み先にファイルはなくその接続先の既定値は UTF-8 である
When 全ての hunk を部分マージする
Then 読み込み元の EUC-JP の元バイト列でファイルが作られる

@id=EX-merge-073 @about=REQ-content-003,REQ-merge-046 @source=docs/decision/records/2026-10-16-content-encoding.md#A5,docs/decision/records/2026-10-16-content-encoding.md#A11
Scenario: 最初の変更を選んでも既存の BOM を変えない
Given 左右は UTF-8 で読み込み元には BOM がなく書き込み先には BOM がある
When 最初の行の変更を部分マージする
Then 書き込み先の BOM が残り本文の選んだ変更だけが適用される
```
