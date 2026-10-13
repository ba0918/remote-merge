# 比べるファイルを読めないときの status と diff

status と diff が比べるべきファイルや参照先のファイルの中身を読めなかったときの出力と終了コード。読めないときの判定と、どのコマンドにも共通する扱いは compare/reading.md の REQ-compare-012・REQ-compare-013・REQ-compare-014 に置き、ここには status と diff に固有のことだけを置く。

## Requirements

### REQ-cli-088: status は読めないファイルを判定せずに知らせる
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A5, docs/decision/records/2026-10-05-quality-cli.md#A14, docs/decision/records/2026-10-05-quality-cli.md#A16, docs/decision/records/2026-10-05-size-limit.md#A26, docs/decision/records/2026-10-05-size-limit.md#A29, docs/decision/records/2026-10-11-agent-operations.md#A38, docs/decision/records/2026-10-11-agent-operations.md#A20, docs/decision/records/2026-10-11-agent-operations.md#A25, docs/decision/records/2026-10-12-foundation-4.md#E7
- verification: unit

status は、中身を読み比べるファイルを片側でも読めなかったとき、compare/reading.md の TBL-compare-005 のとおり、そのファイルを "modified" とも "equal" とも判定せず、そのパスと REQ-compare-014 の理由を JSON ではトップレベルの errors に、テキストでは標準エラーに出し、他のファイルの比較を続けて終了コード 2 を返す。
--ref を指定した status で、左右と参照先の三つともにあるファイルの参照先との組だけが判定できないときは（compare/flow.md の REQ-compare-004）、そのファイルの左右の判定を残し、TBL-cli-005 の印を出さず、そのパスと理由を同じく errors に出して終了コード 2 を返す。
JSON の summary には errors に出したファイルの件数を出す。
summary の件数のキー名とテキストで示す文言は IR で決めず、実装者が決める。テストはトップレベルの errors の項目と終了コードを確かめ、件数のキー名と文言を固定しない。
`接続断` のときは、そのファイルを含めて REQ-ssh-022・REQ-ssh-003・REQ-ssh-023 による。

### REQ-cli-089: diff は読めない側を空とみなさない
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A5, docs/decision/records/2026-10-05-quality-cli.md#A2, docs/decision/records/2026-10-05-size-limit.md#A15, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A20, docs/decision/records/2026-10-11-agent-operations.md#A38, docs/decision/records/2026-10-11-agent-operations.md#A20, docs/decision/records/2026-10-11-agent-operations.md#A25, docs/decision/records/2026-10-12-foundation-4.md#E7
- verification: unit

diff は、比べるファイルか差分を示すための中身を片側でも読めなかったとき（REQ-cli-079 の大きさの上限を超えたときを含む）、読めない側を空として全ての行を差分に出さず、compare/reading.md の TBL-compare-005 のとおり、そのパスと REQ-compare-014 の理由を JSON ではトップレベルの errors に、テキストでは標準エラーに出し、他のファイルの比較を続けて終了コード 2 を返す。読めなかったファイルは JSON の files に項目として載せず、summary の差分のあるファイルの数に入れない。
`接続断` のときは、そのファイルを含めて REQ-ssh-022・REQ-ssh-003・REQ-ssh-023 による。

### REQ-cli-090: 参照先を読めない理由を参照先にないことと分ける
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A5, docs/decision/records/2026-10-05-size-limit.md#A33, docs/decision/records/2026-10-12-foundation-4.md#E3
- verification: unit

--ref を指定した diff は、参照先の同じパスがないかを参照先の状態から決め（compare/flow.md の REQ-compare-003）、状態ではあった参照先のファイルの読み込みが失敗したときは、失敗の種類によらず REQ-cli-063 のとおりそのファイルに "ref" だけを出し、参照先にないときと区別してそのパスと理由を errors に載せ、終了コード 2 を返す。参照先の同じパスがファイルと種類（REQ-merge-001 の通常ファイル・ディレクトリ・末尾の symlink）の違うものであるときは読み込みの失敗とせず、参照先を読まずに "ref" だけを出し、errors に載せない。

## Examples

```gherkin
@id=EX-cli-086 @about=REQ-cli-088 @source=docs/decision/records/2026-10-05-quality-cli.md#A5
Scenario: status で中身を読み比べるファイルを読めない
Given 左右でサイズが同じで更新時刻が違うファイルがあり、右のファイルは読む権限がない
When status --format json を実行する
Then そのファイルは "modified" とも "equal" ともされず、errors にパスと理由が出て終了コード 2 を返す

@id=EX-cli-087 @about=REQ-cli-089 @source=docs/decision/records/2026-10-05-quality-cli.md#A5
Scenario: diff で片側のファイルを読めない
Given 左右にある "a.txt" の右側は読む権限がない
When diff を実行する
Then "a.txt" の全ての行を差分として出さず、パスと理由がエラーとして出て終了コード 2 を返す

@id=EX-cli-088 @about=REQ-cli-090 @source=docs/decision/records/2026-10-05-quality-cli.md#A5
Scenario: 参照先のファイルを読めない
Given 参照先の "a.txt" は読む権限がない
When --ref を指定して diff --format json を実行する
Then "a.txt" には "ref" だけが出て、errors にパスと理由が載り終了コード 2 を返す
```
