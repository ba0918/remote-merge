# 差分一覧の表示

ファイル一覧の既定表示と、全件表示・内容比較を要求する操作、各ファイルの差分の判定。

## Requirements

### REQ-cli-006: 既定では差分だけを列挙する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A21
- verification: unit

status は左右の片方にだけあるファイルと変更されたファイルを既定で列挙し、等しいファイルを省く。

### REQ-cli-007: 全件表示を選べる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A21
- verification: unit

status に --all を指定したときは、等しいファイルも一覧に含める。

### REQ-cli-008: 内容で再比較できる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A21, docs/decision/records/2026-10-05-size-limit.md#A31, docs/decision/records/2026-10-05-size-limit.md#A32, docs/decision/records/2026-10-12-foundation-4.md#E4
- verification: unit

status に --checksum を指定したときは、compare/judgement.md の TBL-compare-003 の中身の比べ方で比べ、メタデータが等しくても中身が違えば変更として報告する。
左右で種類の違うパスは、TBL-compare-001 のとおり中身を読まずに変更として報告する。

### REQ-cli-027: メタデータと中身で差分を判定する
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-status.md#A2, docs/decision/records/2026-10-12-foundation-4.md#E2, docs/decision/records/2026-10-12-foundation-4.md#E4
- verification: unit
- definition: TBL-cli-001

status は左右のファイルを compare/flow.md の比べる流れで判定し（--checksum がなければメタデータの比べ方）、その判定を TBL-cli-001 に従って "left_only"・"right_only"・"modified"・"equal" に訳す。

### REQ-cli-028: symlink は中身を読まずに判定する
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-status.md#A3, docs/decision/records/2026-10-05-quality-cli.md#A6, docs/decision/records/2026-10-12-foundation-4.md#E6
- verification: unit

status は --checksum の有無によらず、片方でも symlink のファイルの中身を読まず、compare/judgement.md の REQ-compare-008 のとおり両方が symlink ならリンク先の文字列が同じときだけ "equal"、違えば "modified" とし、片方だけが symlink なら種類の違いとして "modified" とする。

### REQ-cli-094: status は大きさの上限を当てずに中身を比べる
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A12, docs/decision/records/2026-10-05-quality-cli.md#A2, docs/decision/records/2026-10-12-foundation-4.md#E5
- verification: unit

status は中身を compare/reading.md の REQ-compare-010 のとおりハッシュで比べ、中身をメモリに持って読まないので、REQ-cli-079 の大きさの上限は当たらず、104,857,600 バイトを超えるファイルも中身を比べて判定する。

### REQ-cli-095: status は中身を少しずつ読んでハッシュで比べる
- kind: ubiquitous
- source: docs/decision/records/2026-10-05-quality-cli.md#A12
- verification: review
- how_to_verify: status が中身を読み比べる実装（ローカルとエージェントの各経路）を読み、ファイル全体をメモリに読み込まず、一定の大きさずつ読みながらハッシュを計算して比べていることを確かめる。

status はファイルの中身全体をメモリに持たず、中身を少しずつ読みながらハッシュを計算して左右を比べる。

## Decision tables

### TBL-cli-001: 判定の status の値
- source: docs/decision/records/2026-09-27-adopt-status.md#A2, docs/decision/records/2026-10-05-quality-cli.md#A3, docs/decision/records/2026-10-12-foundation-4.md#E2, docs/decision/records/2026-10-12-foundation-4.md#E7

| 判定（compare/judgement.md の TBL-compare-001・TBL-compare-002） | status の値 |
|---|---|
| 片側だけ（左にある） | "left_only" |
| 片側だけ（右にある） | "right_only" |
| 種類の違い | "modified" |
| 違う | "modified" |
| 同じ | "equal" |
| 判定できない | 値を付けず、compare/reading.md の TBL-compare-005 による |

## Examples

```gherkin
@id=EX-cli-011 @about=REQ-cli-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: 同じファイルを既定表示から省く
Given 左右に等しいファイルと変更されたファイルがある
When status を既定の指定で実行する
Then 差分のあるファイルだけが一覧に出る

@id=EX-cli-012 @about=REQ-cli-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: 片側にだけあるファイルを見せる
Given 左側にだけ存在するファイルがある
When status を実行する
Then そのファイルは左側のみとして一覧に出る

@id=EX-cli-013 @about=REQ-cli-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: 等しいファイルも一覧に含める
Given 左右に等しいファイルがある
When --all を指定して status を実行する
Then そのファイルも一覧に含まれる

@id=EX-cli-014 @about=REQ-cli-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: 全件指定をしない
Given 左右に等しいファイルがある
When --all なしで status を実行する
Then そのファイルは一覧に含まれない

@id=EX-cli-015 @about=REQ-cli-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: メタデータは同じで中身が違う
Given 左右のファイルのサイズと更新時刻が同じで中身が違う
When --checksum で status を実行する
Then そのファイルは変更として報告される

@id=EX-cli-016 @about=REQ-cli-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A21
Scenario: 内容まで等しい
Given 左右のファイルの中身が等しい
When --checksum と --all で status を実行する
Then そのファイルは等しいと報告される

@id=EX-cli-062 @about=REQ-cli-028 @source=docs/decision/records/2026-09-27-adopt-status.md#A3
Scenario: 同じ中身を指す symlink と通常ファイル
Given 左は通常ファイルで、右は同じ中身のファイルを指す同じサイズの symlink である
When status を実行する
Then そのファイルは "modified" と報告される
@id=EX-cli-089 @about=REQ-cli-027 @source=docs/decision/records/2026-10-05-quality-cli.md#A3
Scenario: 片側の更新時刻が秒の精度しかない
Given 左右のファイルはサイズが同じで中身が違い、ローカルの更新時刻はナノ秒、リモートは秒の精度しかなく同じ秒を指す
When status を実行する
Then 中身を読み比べてそのファイルを "modified" と報告する

@id=EX-cli-090 @about=REQ-cli-028 @source=docs/decision/records/2026-10-05-quality-cli.md#A6
Scenario: --checksum でリンク文字列が同じ symlink を比べる
Given 左右の symlink は同じリンク文字列を持ち参照先の中身が違う
When --checksum と --all で status を実行する
Then そのファイルは参照先を読まずに "equal" と報告される

@id=EX-cli-093 @about=REQ-cli-094 @source=docs/decision/records/2026-10-05-quality-cli.md#A12,docs/decision/records/2026-10-05-quality-cli.md#A2,docs/decision/records/2026-09-27-adopt-status.md#A2
Scenario: 上限を超える大きさのファイルを status で比べる
Given 左右に 104,857,600 バイトを超え、サイズが同じで更新時刻と中身が違うファイルがある
When status を実行する
Then そのファイルは読めないファイルとされず "modified" と報告される
```
