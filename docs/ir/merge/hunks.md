# 変更のまとまりを選ぶマージ

テキストファイル全体ではなく、利用者が選んだ変更だけを書き込む操作。

## Requirements

### REQ-merge-009: 選択した変更だけを適用する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A29
- verification: unit

テキスト差分から選んだ hunk だけをマージしたとき、書き込み先にはその変更だけを適用し、選ばなかった変更は保持する。

### REQ-merge-010: 部分マージ前に衝突と書き込みを確認する
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A29
- verification: unit

hunk マージは衝突の有無と書き込みの確認を経ずに書き込み先を変更しない。

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
- source: docs/decision/records/2026-09-28-adopt-merge-hunks.md#A3, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A3
- verification: unit

--hunks の merge のテキスト出力は、総数を REQ-merge-032 の区切りで数え、書き込んだファイルを "Merged: パス (hunks: 番号,番号/総数)"（バックアップがあれば続けて " (backup: バックアップ)"）、--dry-run では "Would merge: パス (hunks: 番号,番号/総数)" の行で出す。

### REQ-merge-031: 三者の競合でエラーで止める
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-merge-hunks.md#A4, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A4, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A11
- verification: unit

--ref があり --force のない --hunks の merge は、選んだ hunk の中に限らずファイルのどこかに参照先に対して左右が同じ箇所を異なる内容に変えた競合があれば、そのファイルを書き込まずに "three-way conflict: パス" のエラーで止め、--dry-run でも同じように止める。

### REQ-merge-032: --hunks の番号は diff の hunk で数える
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A3
- verification: unit

--hunks の番号は、同じ --left・--right・パスで実行した diff --format json の hunks と同じ区切り（変更の前後 3 行の文脈でまとめたもの）を 0 から数えたものとし、hunk の数と番号の範囲外の判定もこの区切りで数え、一つの区切りに複数の変更のかたまりが入るとき、その番号を選べばその全てを適用する。

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

@id=EX-merge-019 @about=REQ-merge-010 @source=docs/decision/records/2026-09-25-spec-migration.md#A29
Scenario: 確認を拒否する
Given 一つの hunk が選ばれている
When 利用者が書き込み前の確認を拒否する
Then 書き込み先は変更されない

@id=EX-merge-020 @about=REQ-merge-010 @source=docs/decision/records/2026-09-25-spec-migration.md#A29
Scenario: 衝突を検出する
Given 適用先で対象行が変更されている
When 選択した hunk を適用する
Then 衝突が報告され確認なしに書き込み先は変更されない

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
```
