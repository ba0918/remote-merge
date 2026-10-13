# 走査の一覧に載るもの

status・diff・merge・sync のツリーの走査が、symlink・その他の種類・root_dir・UTF-8 でない名前・更新時刻をどう一覧に載せるか、root_dir や項目の情報を読めないときにどう止まるか、走査の結果が経路によらないこと。

## Requirements

### REQ-scan-017: 走査の結果は経路によらない
- kind: invariant
- source: docs/decision/records/2026-10-12-foundation-2.md#C1, docs/decision/records/2026-10-12-foundation-2.md#C6
- verification: unit

同じ設定で同じ中身のツリーを走査するとき、ローカル・SSH・エージェントのどの経路でも、走査は同じ一覧、同じ不完全な一覧の報告、同じエラーを返す。
経路で違いうるのは、FLAG-scan-015 の更新時刻の桁だけである。

### REQ-scan-006: symlink はそれ自体を一覧に載せる
- kind: ubiquitous
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A1, docs/decision/records/2026-10-12-ssh-output-parsing.md#A1, docs/decision/records/2026-10-12-foundation-2.md#C1
- verification: unit

ツリーの走査は、symlink 自体を一覧の一つの項目としてリンク先の文字列とともに載せ、リンク先がない symlink も載せる。
項目の名前とリンク先の文字列は、TAB・改行・"\" を含めて、読んだとおりに載せる。

### REQ-scan-007: root_dir の symlink を辿る
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A2
- verification: unit

root_dir 自体がディレクトリへの symlink のとき、status・diff・merge・sync のツリーの走査はリンク先の配下を列挙し、設定の root_dir の末尾の "/" の有無で一覧は変わらない。

### REQ-scan-012: UTF-8 でない名前を置き換えて載せない
- kind: prohibition
- source: docs/decision/records/2026-10-05-quality-scan.md#A3, docs/decision/records/2026-10-05-quality-scan.md#A6, docs/decision/records/2026-10-05-quality-scan.md#A7, docs/decision/records/2026-10-12-untested-items-implement.md#A5, docs/decision/records/2026-10-12-untested-items-consistency.md#A4, docs/decision/records/2026-10-12-foundation-4.md#E7
- verification: unit

ツリーの走査は、UTF-8 として読めない名前のファイルを置換文字に置き換えた名前で一覧に載せず、読めない名前として報告する。報告するパスは、root_dir からのパスの UTF-8 として読める部分はそのままにし、読めない部分のバイトを一つずつ "\xNN"（NN は小文字の 16 進 2 桁）と書いたものとする。
そのファイルは compare/judgement.md の TBL-compare-001 で判定できないとし、compare/reading.md の TBL-compare-005 のとおり、status と diff は errors に、merge と sync はそのパスの failed の項目として出し、終了コード 2 を返す。理由の文言は IR で決めず、実装者が決め、テストはその文言を固定しない。

### REQ-scan-021: その他の種類はその種類のまま載せる
- kind: ubiquitous
- source: docs/decision/records/2026-10-12-foundation-4.md#E8
- verification: unit

ツリーの走査と子の一覧は、通常ファイル・ディレクトリ・symlink のどれでもないもの（FIFO・ソケット・デバイス）を、通常ファイルとせずにその他の種類として一覧に載せ、その中身を読まない。
その他の種類の項目は、件数の上限に当てる件数に数える（REQ-scan-010）。

### REQ-scan-020: diff のディレクトリの展開も同じ走査を使う
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-2.md#C3
- verification: unit

diff が名前で指定したディレクトリの配下を展開するときは、起点をそのディレクトリにした同じ走査を使い、配下の UTF-8 として読めない名前は REQ-scan-012 のとおり読めない名前として出す。

### REQ-scan-015: root_dir や項目の情報を読めなければ走査を止める
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-scan.md#A5, docs/decision/records/2026-10-05-quality-scan.md#A7, docs/decision/records/2026-10-12-foundation-2.md#C1
- verification: unit

root_dir がないか読めないとき、または symlink のリンク先の文字列か項目の更新時刻を読めないとき、走査は既定の値で埋めずにエラーで止まる。
root_dir がないか読めないときは走査の時点で止まり、走査する側がリモートなら "Remote root_dir not found: サーバ名:パス" を出す。走査する側がローカルのときの文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。

### REQ-scan-016: 走査の更新時刻は読んだとおりに載せる
- kind: ubiquitous
- source: docs/decision/records/2026-10-12-ssh-output-parsing-consistency-3.md#A1, docs/decision/records/2026-10-12-foundation-2.md#C1
- verification: unit

ツリーの走査は、項目の更新時刻を、1970 年より前のものも含めて読んだ値のままナノ秒まで載せる。
秒しか出さない find のサーバを SSH の経路で走査したときの秒未満の差は、FLAG-scan-015 に残す。

## Examples

```gherkin
@id=EX-scan-017 @about=REQ-scan-021,REQ-scan-017 @source=docs/decision/records/2026-10-12-foundation-4.md#E8
Scenario: 名前付きパイプとソケットの走査
Given root_dir に名前付きパイプ "pipe" と UNIX ドメインソケット "app.sock" がある
When ツリーを走査する
Then 三つの経路で同じく、"pipe" と "app.sock" は通常ファイルでなくその他の種類として一覧に載り、走査は止まらずに終わる

@id=EX-scan-014 @about=REQ-scan-012 @source=docs/decision/records/2026-10-05-quality-scan.md#A6,docs/decision/records/2026-10-05-quality-scan.md#A3
Scenario: UTF-8 として読めない名前のファイルがある
Given 左の root_dir に UTF-8 として読めない名前のファイルがある
When status --format json を実行する
Then その名前は置換文字に置き換えて files に載らず、errors に出て終了コード 2 を返す

@id=EX-scan-015 @about=REQ-scan-006,REQ-scan-017 @source=docs/decision/records/2026-10-12-ssh-output-parsing.md#A1,docs/decision/records/2026-10-12-foundation-2.md#C1
Scenario: 名前に TAB・改行・"\" を含むファイルの走査
Given root_dir に、名前に TAB を含む "x\ty.txt" と、名前に改行を含む "p\nq.txt" と、名前に "\" を含む "b\\c.txt" がある
When status を実行する
Then 三つの経路で同じく、その三つが同じ名前で一覧に一件ずつ載る

@id=EX-scan-016 @about=REQ-scan-006,REQ-scan-017 @source=docs/decision/records/2026-10-12-ssh-output-parsing.md#A1,docs/decision/records/2026-10-12-foundation-2.md#C1
Scenario: リンク先に TAB や末尾の改行を含む symlink の走査
Given root_dir に、リンク先が "a\tb.txt" の symlink と、リンク先が改行で終わる symlink と、リンク先に "\" を含む symlink がある
When ツリーを走査する
Then 三つの経路で同じく、三つの symlink は読んだとおりのリンク先で一覧に載る
```
