# 走査の一覧に載るもの

status・diff・merge・sync のツリーの走査が、symlink・root_dir・UTF-8 でない名前・更新時刻をどう一覧に載せるか、root_dir や項目の情報を読めないときにどう止まるか。

## Requirements

### REQ-scan-006: symlink はそれ自体を一覧に載せる
- kind: ubiquitous
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A1, docs/decision/records/2026-10-12-ssh-output-parsing.md#A1
- verification: unit

ツリーの走査は、ローカル・SSH・エージェントのどの経路でも symlink 自体を一覧の一つの項目としてリンク先の文字列とともに載せ、リンク先がない symlink も載せる。
項目の名前とリンク先の文字列は、TAB・改行・"\" を含めて、どの経路でも読んだとおりに載せる。

### REQ-scan-007: root_dir の symlink を辿る
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A2
- verification: unit

root_dir 自体がディレクトリへの symlink のとき、status・diff・merge・sync のツリーの走査はリンク先の配下を列挙し、設定の root_dir の末尾の "/" の有無で一覧は変わらない。

### REQ-scan-012: UTF-8 でない名前を置き換えて載せない
- kind: prohibition
- source: docs/decision/records/2026-10-05-quality-scan.md#A3, docs/decision/records/2026-10-05-quality-scan.md#A6, docs/decision/records/2026-10-05-quality-scan.md#A7, docs/decision/records/2026-10-12-untested-items-implement.md#A5
- verification: unit

ツリーの走査は、UTF-8 として読めない名前のファイルを置換文字に置き換えた名前で一覧に載せず、読めない名前として報告する。報告するパスは、root_dir からのパスの UTF-8 として読める部分はそのままにし、読めない部分のバイトを一つずつ "\xNN"（NN は小文字の 16 進 2 桁）と書いたものとする。
status と diff は、そのファイルを REQ-cli-088 と REQ-cli-089 の読めないファイルと同じく errors に出す。merge と sync は、そのファイルをそのパスの failed の項目として出す。failed の理由の文言は IR で決めず、実装者が決め、テストはその文言を固定しない。そのファイルがあるとき、status・diff・merge・sync は終了コード 2 を返す。

### REQ-scan-015: root_dir や項目の情報を読めなければ走査を止める
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-scan.md#A5, docs/decision/records/2026-10-05-quality-scan.md#A7, docs/decision/records/2026-10-12-ssh-output-parsing-consistency-2.md#A1, docs/decision/records/2026-10-12-ssh-output-parsing-consistency-3.md#A2
- verification: unit

root_dir がないか読めないとき、ローカル・SSH・エージェントのどの経路も走査の時点でエラーで止まり、リモートでは "Remote root_dir not found: サーバ名:パス" を出す。symlink のリンク先の文字列か更新時刻を読めないときも、既定の値で埋めずにエラーで止まる。リンク先がないのではなく、ループしているか途中にディレクトリでないものを含むためにリンク先を辿れない symlink があるときも、ディレクトリへの symlink かを決められないので、それを載せずにエラーで止まる（リンク先がない symlink は REQ-scan-006 のとおり載せる）。include の起点がないときは config/include.md の REQ-config-032 による。
exclude に当たる symlink と、exclude に当たるディレクトリの下の symlink は、リンク先を辿れなくても走査を止めずに一覧に載せない（config/filters.md の REQ-config-021・REQ-config-022。SSH の経路の古い find の差は FLAG-config-016）。
ローカルの root_dir がないか読めないときのエラーの文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。

### REQ-scan-016: 走査の更新時刻は読んだとおりに載せる
- kind: ubiquitous
- source: docs/decision/records/2026-10-12-ssh-output-parsing-consistency-3.md#A1
- verification: unit

ツリーの走査は、ローカル・SSH・エージェントのどの経路でも、項目の更新時刻を、1970 年より前のものも含めて読んだ値のままナノ秒まで載せる。
SSH の経路の更新時刻は、リモートの find が出した桁までとし、秒しか出さない find でナノ秒を記録するファイルシステムを走査したときの差は FLAG-scan-015 に残す。

## Examples

```gherkin
@id=EX-scan-014 @about=REQ-scan-012 @source=docs/decision/records/2026-10-05-quality-scan.md#A6,docs/decision/records/2026-10-05-quality-scan.md#A3
Scenario: UTF-8 として読めない名前のファイルがある
Given 左の root_dir に UTF-8 として読めない名前のファイルがある
When status --format json を実行する
Then その名前は置換文字に置き換えて files に載らず、errors に出て終了コード 2 を返す

@id=EX-scan-015 @about=REQ-scan-006 @source=docs/decision/records/2026-10-12-ssh-output-parsing.md#A1
Scenario: 名前に TAB・改行・"\" を含むファイルの走査
Given root_dir に、名前に TAB を含む "x\ty.txt" と、名前に改行を含む "p\nq.txt" と、名前に "\" を含む "b\\c.txt" がある
When ローカル・SSH・エージェントのそれぞれの経路で status を実行する
Then どの経路でも、その三つが同じ名前で一覧に一件ずつ載る

@id=EX-scan-016 @about=REQ-scan-006 @source=docs/decision/records/2026-10-12-ssh-output-parsing.md#A1
Scenario: リンク先に TAB や末尾の改行を含む symlink の走査
Given root_dir に、リンク先が "a\tb.txt" の symlink と、リンク先が改行で終わる symlink と、リンク先に "\" を含む symlink がある
When ローカル・SSH・エージェントのそれぞれの経路で走査する
Then どの経路でも、三つの symlink は読んだとおりのリンク先で一覧に載る
```
