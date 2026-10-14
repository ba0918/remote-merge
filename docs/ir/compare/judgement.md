# 二つの側の判定

比べる流れが、二つの側のパスの状態の組から、同じ・違う・片側だけ・種類の違い・両側にない・判定できないのどれかを決める規則と、二つの通常ファイルをメタデータで比べるか中身で比べるかの選び方、symlink とその他の種類の比べ方。
判定はどのコマンドでも同じで、コマンドはそれを自分の扱いに訳す（flow.md の REQ-compare-002）。

## Requirements

### REQ-compare-006: 状態の組を一つの表で判定する
- kind: algorithm
- source: docs/decision/records/2026-10-12-foundation-4.md#E2, docs/decision/records/2026-10-12-foundation-4.md#E4, docs/decision/records/2026-10-12-foundation-4.md#E7
- verification: unit
- definition: TBL-compare-001, TBL-compare-002

### REQ-compare-007: 比べ方をコマンドと指定から選ぶ
- kind: algorithm
- source: docs/decision/records/2026-10-12-foundation-4.md#E4
- verification: unit
- definition: TBL-compare-003

### REQ-compare-008: symlink はリンク文字列で比べる
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E6
- verification: unit

二つの側がともに末尾の symlink のとき、どのコマンドも、リンク先を辿らずにリンク文字列のバイト列が同じなら同じ、違えば違うと判定する。
diff は、それに加えて二つの symlink を辿った先の組を、もう一つの組として同じ TBL-compare-001 と TBL-compare-002 で判定し、リンクの組と辿った先の組のどちらかが同じでなければそのパスを差分ありとする（cli/symlink-diff.md の REQ-cli-020）。
--ref を使う merge で読み込み元・書き込み先・参照先の三つとも末尾の symlink のときは、cli/merge.md の REQ-cli-051 の確認をリンク文字列で行う。

### REQ-compare-009: その他の種類は読まずに判定できないとする
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E8, docs/decision/records/2026-10-12-foundation-4.md#E34, docs/decision/records/2026-10-12-foundation-5.md#F5
- verification: unit

二つの側のどちらかのパスがその他の種類（通常ファイル・ディレクトリ・symlink のどれでもない FIFO・ソケット・デバイス）のとき、比べる流れはそのパスの中身もハッシュも読まず、判定できないとし、reading.md の REQ-compare-013 のとおり扱う。
REQ-compare-008 で二つの symlink を辿った先の組を判定するときも同じで、辿った先のどちらかがその他の種類なら、辿った先の中身もハッシュも読まずに判定できないとする。
理由は、どのコマンドでも、"not a regular file: " に続けて、その他の種類だった側の名前（reading.md の REQ-compare-014 と同じ "left"・"right"・"ref"、rollback は "target"）をこの順に "; " でつないだものにする。
merge と sync は、その他の種類を書き込み先に作らず、書き換えず、--delete でも消さない。

## Decision tables

### TBL-compare-001: 二つの側の状態の組の判定
- source: docs/decision/records/2026-10-12-foundation-4.md#E2, docs/decision/records/2026-10-12-foundation-4.md#E4, docs/decision/records/2026-10-12-foundation-4.md#E6, docs/decision/records/2026-10-12-foundation-4.md#E8, docs/decision/records/2026-10-12-foundation-5.md#F57

上の行から順に当て、最初に当たった行の判定にする。

| 二つの側の状態 | 比べ方 | 判定 |
|---|---|---|
| 両側にない | - | 両側にない |
| どちらかの名前が UTF-8 として読めない（scan/listing.md の REQ-scan-012） | - | 判定できない |
| どちらかがその他の種類 | - | 判定できない（"not a regular file: 側"。REQ-compare-009） |
| どちらかがリンク先を辿れない symlink（scan/directory-links.md の REQ-scan-018） | - | 判定できない（理由は "cannot follow the symlink target: " に続けて、そうだった側の名前を REQ-compare-009 と同じ順に "; " でつないだもの） |
| 片側にだけある | - | 片側だけ |
| 種類（通常ファイル・ディレクトリ・末尾の symlink）が違う。ディレクトリとディレクトリへの symlink の組を含む | - | 種類の違い |
| 両方が末尾の symlink | - | リンク文字列が同じなら同じ、違えば違う（REQ-compare-008） |
| 両方がディレクトリ | - | 同じ（中の項目は、そのパスごとに判定する） |
| 両方が通常ファイルで大きさが違う | - | 違う（中身を読まない） |
| 両方が通常ファイルで大きさが同じで、両側がともに秒より細かい精度の更新時刻を持ち、両側に共通する最も細かい精度で更新時刻が同じ | メタデータ | 同じ（中身を読まない） |
| 両方が通常ファイルで大きさが同じで、上の行に当たらない（片側でも更新時刻が秒の精度しかない、更新時刻が違う、大きさか更新時刻が分からない） | メタデータ | 中身で決める（TBL-compare-002） |
| 両方が通常ファイルで大きさが同じ | 中身 | 中身で決める（TBL-compare-002） |

### TBL-compare-002: 中身で決める組の判定
- source: docs/decision/records/2026-10-12-foundation-4.md#E5, docs/decision/records/2026-10-12-foundation-4.md#E7

| 両側の中身の読み込み（reading.md の REQ-compare-010） | 判定 |
|---|---|
| 両側を読めて、バイト列が同じ | 同じ |
| 両側を読めて、バイト列が違う | 違う |
| どちらかの側が `接続断` 以外の失敗になった | 判定できない（reading.md の REQ-compare-012） |
| どちらかの側が `接続断` になった | 判定せず、ssh/reconnect.md の REQ-ssh-003 による |

### TBL-compare-003: 二つの通常ファイルの比べ方
- source: docs/decision/records/2026-10-12-foundation-4.md#E4

| コマンド | 指定 | 比べ方 |
|---|---|---|
| status | --checksum なし | メタデータ |
| status | --checksum あり | 中身 |
| diff | パスの指定の仕方によらない | 中身 |
| merge・sync | 名前で指定したファイル | 中身 |
| merge・sync | --checksum あり | 中身 |
| merge・sync | --checksum なしで、指定したディレクトリの下か root_dir 全体の下のファイル | メタデータ |

## Examples

```gherkin
@id=EX-compare-019 @about=REQ-compare-009 @source=docs/decision/records/2026-10-12-foundation-5.md#F5
Scenario: 参照先だけがその他の種類のときの理由
Given 左右の "a" は通常ファイルで、参照先の "a" は名前付きパイプである
When --ref を指定して status を実行する
Then 参照先との組の理由は "not a regular file: ref" で、サーバの名前を含まない

@id=EX-compare-006 @about=REQ-compare-006 @source=docs/decision/records/2026-10-12-foundation-4.md#E4
Scenario: メタデータで比べて秒の精度しかない側がある
Given 左右の "a.txt" はサイズが同じで中身が違い、左の更新時刻はナノ秒、右は秒の精度しかなく同じ秒を指す
When メタデータの比べ方で判定する
Then 中身で決めることになり、中身を読んで違うと判定する

@id=EX-compare-007 @about=REQ-compare-006 @source=docs/decision/records/2026-10-12-foundation-4.md#E4
Scenario: 大きさが違えば中身を読まずに違うとする
Given 左右の "a.txt" は大きさが違う
When 中身の比べ方で判定する
Then 中身を読まずに違うと判定する

@id=EX-compare-008 @about=REQ-compare-007 @source=docs/decision/records/2026-10-12-foundation-4.md#E4
Scenario: diff は root_dir 全体でも中身で比べる
Given 左右の "a.txt" はサイズと更新時刻がナノ秒まで同じで中身が違う
When パスを指定せずに diff を実行する
Then "a.txt" の中身の差分が出て終了コード 1 を返す

@id=EX-compare-009 @about=REQ-compare-008 @source=docs/decision/records/2026-10-12-foundation-4.md#E6
Scenario: 三つとも symlink の merge をリンク文字列で確かめる
Given 読み込み元・書き込み先・参照先の "link" は symlink で、読み込み元と参照先のリンク文字列が同じで、書き込み先だけが違うリンク文字列を持つ
When --ref を指定し --force なしで "link" を merge する
Then 書き込み先の "link" は変わらず、failed に error "destination changed since reference" で出る

@id=EX-compare-010 @about=REQ-compare-009 @source=docs/decision/records/2026-10-12-foundation-4.md#E8
Scenario: FIFO を読みに行かない
Given 左右の root_dir に名前付きパイプ "pipe" がある
When status を実行する
Then status は止まらずに終わり、"pipe" はパスと理由 "not a regular file" とともに errors に出て、終了コード 2 を返す

@id=EX-compare-011 @about=REQ-compare-009 @source=docs/decision/records/2026-10-12-foundation-4.md#E8
Scenario: 書き込み先だけのソケットを --delete で消さない
Given 書き込み先にだけ UNIX ドメインソケット "app.sock" がある
When --delete を付けて sync する
Then "app.sock" は消えず、failed に理由 "not a regular file" で出る
```
