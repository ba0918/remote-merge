# 走査の一覧に載るもの

status・diff・merge・sync のツリーの走査が、symlink・root_dir・UTF-8 でない名前をどう一覧に載せるか、root_dir や項目の情報を読めないときにどう止まるか。

## Requirements

### REQ-scan-006: symlink はそれ自体を一覧に載せる
- kind: ubiquitous
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A1
- verification: unit

ツリーの走査は、ローカル・SSH・エージェントのどの経路でも symlink 自体を一覧の一つの項目としてリンク先の文字列とともに載せ、リンク先がない symlink も載せる。

### REQ-scan-007: root_dir の symlink を辿る
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A2
- verification: unit

root_dir 自体がディレクトリへの symlink のとき、status・diff・merge・sync のツリーの走査はリンク先の配下を列挙し、設定の root_dir の末尾の "/" の有無で一覧は変わらない。

### REQ-scan-012: UTF-8 でない名前を置き換えて載せない
- kind: prohibition
- source: docs/decision/records/2026-10-05-quality-scan.md#A3, docs/decision/records/2026-10-05-quality-scan.md#A6, docs/decision/records/2026-10-05-quality-scan.md#A7
- verification: unit

ツリーの走査は、UTF-8 として読めない名前のファイルを置換文字に置き換えた名前で一覧に載せず、読めない名前として報告する。
status と diff は、そのファイルを REQ-cli-088 と REQ-cli-089 の読めないファイルと同じく errors に出す。merge と sync は、そのファイルをそのパスの failed の項目として出す。failed の理由の文言は IR で決めず、実装者が決め、テストはその文言を固定しない。そのファイルがあるとき、status・diff・merge・sync は終了コード 2 を返す。

### REQ-scan-015: root_dir や項目の情報を読めなければ走査を止める
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-scan.md#A5, docs/decision/records/2026-10-05-quality-scan.md#A7
- verification: unit

root_dir がないか読めないとき、ローカル・SSH・エージェントのどの経路も走査の時点でエラーで止まり、リモートでは "Remote root_dir not found: サーバ名:パス" を出す。symlink のリンク先の文字列か更新時刻を読めないときも、既定の値で埋めずにエラーで止まる。include の起点がないときは config/include.md の REQ-config-032 による。
ローカルの root_dir がないか読めないときのエラーの文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。

## Examples

```gherkin
@id=EX-scan-014 @about=REQ-scan-012 @source=docs/decision/records/2026-10-05-quality-scan.md#A6,docs/decision/records/2026-10-05-quality-scan.md#A3
Scenario: UTF-8 として読めない名前のファイルがある
Given 左の root_dir に UTF-8 として読めない名前のファイルがある
When status --format json を実行する
Then その名前は置換文字に置き換えて files に載らず、errors に出て終了コード 2 を返す
```
