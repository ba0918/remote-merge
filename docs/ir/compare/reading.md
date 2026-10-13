# 比べるための読み込みと変更の直前の判定

比べる流れが、中身が同じかをどう決め、どの中身をいつ読み、読み込みの失敗の種類をどう扱うか、判定できないパスをどのコマンドでも同じに扱うこと、merge と sync が変更の直前にもう一度判定すること。

## Requirements

### REQ-compare-010: 中身が同じかはハッシュで決める
- kind: ubiquitous
- source: docs/decision/records/2026-10-12-foundation-4.md#E5
- verification: unit

比べる流れは、TBL-compare-001 が中身で決めるとした二つの通常ファイルについて、両側の中身の SHA-256 のハッシュを取り、ハッシュが同じならバイト列が同じ、違えば違うとする。
ハッシュは中身を少しずつ読みながら取り、中身全体をメモリに持たない。
ハッシュを取るときは cli/size-limit.md の REQ-cli-079 の大きさの上限を当てず、ハッシュを取る道具のないリモートで中身を送って手元でハッシュを取るとき（ssh/compatibility.md の TBL-ssh-002）も当てない。

### REQ-compare-011: 中身を読むのはコマンドがその中身を使うときだけにする
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E5, docs/decision/records/2026-10-12-foundation.md#A7
- verification: unit

比べる流れが中身をメモリに持って読むのは、diff が違う・片側だけ・種類の違いと判定したファイルの差分を示すとき、merge・sync・rollback が書き込むとき、バックアップのときだけであり、判定のためには読まない。
この読み込みには、REQ-cli-079 の大きさの上限を当て、上限を超えるファイルは中身を読まずに上限を超える失敗にする。
読むものは、経路ごとにまとめて尋ねる（paths/operations.md の REQ-paths-013）。

### REQ-compare-012: 読み込みの失敗の種類をどのコマンドでも同じに扱う
- kind: algorithm
- source: docs/decision/records/2026-10-12-foundation-4.md#E3, docs/decision/records/2026-10-12-foundation-4.md#E7
- verification: unit
- definition: TBL-compare-004

### REQ-compare-013: 判定できないパスをどのコマンドでも同じに扱う
- kind: algorithm
- source: docs/decision/records/2026-10-12-foundation-4.md#E7, docs/decision/records/2026-10-12-foundation-4.md#E12
- verification: unit
- definition: TBL-compare-005

### REQ-compare-014: 判定できない理由は失敗した側ごとに示す
- kind: ubiquitous
- source: docs/decision/records/2026-10-12-foundation-4.md#E7
- verification: unit

読み込みの失敗で判定できないパスの理由は、どのコマンドでも、"read failed: " に続けて、読めなかった側ごとの "left: 原因"・"right: 原因"・"ref: 原因" をこの順に "; " でつないだものにする。
原因は、その側の読み込みの失敗の種類（paths/operations.md の REQ-paths-020）から一か所で作る文言とする。

### REQ-compare-015: 変更の直前に書き込み先をもう一度判定する
- kind: event_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E9, docs/decision/records/2026-10-12-foundation-4.md#E32
- verification: unit

merge と sync は、書き込み・symlink の作成と置き換え・削除・--hunks の書き込みのどれでも、その変更の直前に書き込み先のそのパスの状態をもう一度集め、通常ファイルならハッシュも取り、比べたときと種類・リンク文字列・ハッシュのどれかが違うとき、または比べたときになかったパスが現れたときは、変えずにそのパスを failed に出してほかのパスを続ける。
そのために、比べる流れは、書き換えるか消す予定の書き込み先の通常ファイルのハッシュを比べるときに取る。
もう一度集めた状態では通常ファイルだったパスのハッシュが、ない の失敗（paths/operations.md の REQ-paths-020）になったときは、比べた後にそのパスが変わったものとして、変えずに failed に出す。
これは変更の直前の判定だけの扱いで、比べるときの読み込みの失敗からないことを決めない REQ-compare-003 は変わらない。
もう一度集めた状態かハッシュがそれ以外の `接続断` でない失敗になったときは、変えずに REQ-compare-013 の判定できないとして扱う。

## Decision tables

### TBL-compare-004: 比べるための読み込みの失敗の扱い
- source: docs/decision/records/2026-10-12-foundation-4.md#E3, docs/decision/records/2026-10-12-foundation-4.md#E7, docs/decision/records/2026-10-12-foundation-4.md#E5

| 失敗の種類（REQ-paths-020） | 扱い |
|---|---|
| ない | 状態を集めた後に変わったものとして判定できない（flow.md の REQ-compare-003） |
| ディレクトリでない | 判定できない |
| 権限がない | 判定できない |
| 上限を超える（中身をメモリに持つ読み込みだけで起きる） | 判定できない。--force を指定したコマンドは上限を当てない |
| 道具がない | 判定できない。理由に道具の名前を含める |
| I/O | 判定できない |
| `接続断` | 判定できないとせず、ssh/reconnect.md の REQ-ssh-003・REQ-ssh-022・REQ-ssh-023 による |

### TBL-compare-005: 判定できないパスのコマンドごとの扱い
- source: docs/decision/records/2026-10-12-foundation-4.md#E7, docs/decision/records/2026-10-12-foundation-4.md#E12

| コマンド | 扱い |
|---|---|
| status | そのパスを "modified" とも "equal" とも "left_only"・"right_only" ともせず、パスと理由を JSON ではトップレベルの errors に、テキストでは標準エラーに出し、ほかのパスを続けて終了コード 2 を返す |
| diff | そのパスを files に載せず差分のあるファイルの数に入れず、パスと理由を JSON ではトップレベルの errors に、テキストでは標準エラーに出し、ほかのパスを続けて終了コード 2 を返す |
| merge・sync（--dry-run を含む） | そのパスを書かず消さず、failed にパスと理由を出し、ほかのパスを続けて終了コード 2 を返す（sync はその書き込み先の status に数える） |
| rollback（--dry-run を含む） | そのパスを書き戻さず、failed にパスと理由を出し、ほかのパスを続けて終了コード 2 を返す |

## Examples

```gherkin
@id=EX-compare-012 @about=REQ-compare-010 @source=docs/decision/records/2026-10-12-foundation-4.md#E5
Scenario: 上限を超える同じ中身のファイルを比べる
Given 左右に 104,857,600 バイトを超える同じ中身の "big.bin" がある
When --force なしで "big.bin" を指定した diff と merge を実行する
Then どちらも "big.bin" を読めないファイルとせず、diff は差分なし、merge は書き込まずに終わる

@id=EX-compare-013 @about=REQ-compare-011 @source=docs/decision/records/2026-10-12-foundation-4.md#E5
Scenario: 上限を超える違う中身のファイルを diff で比べる
Given 左右に 104,857,600 バイトを超える中身の違う "big.bin" がある
When --force なしで "big.bin" を指定した diff を実行する
Then "big.bin" は中身が読まれず、上限を超えることを理由に errors に出て、終了コード 2 を返す

@id=EX-compare-014 @about=REQ-compare-012 @source=docs/decision/records/2026-10-12-foundation-4.md#E7
Scenario: どの種類の失敗も判定できないにする
Given 左右の "a.txt" は中身で決める組で、右の中身の読み込みが、ない・ディレクトリでない・権限がない・上限を超える・道具がない・I/O のそれぞれの失敗になる
When それぞれの場合に組を判定する
Then どの場合も判定できないになり、同じとも違うとも片側だけともならない

@id=EX-compare-015 @about=REQ-compare-013 @source=docs/decision/records/2026-10-12-foundation-4.md#E7
Scenario: 同じ読めないファイルを status・diff・merge で同じに扱う
Given 左右にある "a.txt" はサイズが同じで中身が違い、右の "a.txt" は読む権限がない
When status、diff、--checksum を付けた merge をそれぞれ実行する
Then status と diff は "a.txt" を errors に、merge は failed に、同じ理由で出し、どれも終了コード 2 を返す

@id=EX-compare-016 @about=REQ-compare-014 @source=docs/decision/records/2026-10-12-foundation-4.md#E7
Scenario: 両側とも読めない理由
Given "a.txt" は左では読む権限がなく、右ではディレクトリでない途中の要素の下にあると読み込みが答える
When "a.txt" の組を判定する
Then 理由は "read failed: left: " で始まり、"; right: " に右の原因が続く

@id=EX-compare-017 @about=REQ-compare-015 @source=docs/decision/records/2026-10-12-foundation-4.md#E9
Scenario: 比べた後に中身だけ変わった書き込み先を --hunks で書かない
Given "a.txt" を比べた後、--hunks の merge が書く前に、書き込み先の "a.txt" の中身がサイズと更新時刻を変えずに書き換えられる
When --hunks 0 で "a.txt" を merge する
Then 書き込み先の "a.txt" は書き換えられた中身のまま変わらず、"a.txt" は failed に出る

@id=EX-compare-018 @about=REQ-compare-015 @source=docs/decision/records/2026-10-12-foundation-4.md#E9
Scenario: 比べた後に書き換えられた書き込み先だけのファイルを消さない
Given 書き込み先にだけある "old.txt" を比べた後、削除の前に別の操作が "old.txt" の中身を書き換える
When --delete を付けて sync する
Then "old.txt" は消えず、failed に出て、ほかのファイルの処理は続く
```
