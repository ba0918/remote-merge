# ファイルの大きさの上限

差分の表示・書き込み・バックアップのためにファイルの中身をメモリに持って読むときの大きさの上限と、それを外す指定。中身が同じかを決めるハッシュは中身をメモリに持たないので、この上限の対象にしない（compare/reading.md の REQ-compare-010）。

## Requirements

### REQ-cli-079: 中身をメモリに持つ読み込みに一つの大きさの上限を読む前に当てる
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A2, docs/decision/records/2026-10-05-quality-cli.md#A12, docs/decision/records/2026-10-05-size-limit.md#A7, docs/decision/records/2026-10-07-size-limit-consistency.md#A2, docs/decision/records/2026-10-12-foundation-3.md#D7, docs/decision/records/2026-10-12-foundation-4.md#E5
- verification: unit

diff・merge・sync・rollback は、差分の表示・書き込み・バックアップのためにファイルの中身をメモリに持って読む前にその大きさを調べ、104,857,600 バイトを超えるファイルは中身を読まずに上限を超える失敗（paths/operations.md の REQ-paths-020）とし、compare/reading.md の TBL-compare-004 と TBL-compare-005 のとおり扱う。
中身が同じかを決めるためのハッシュ（compare/reading.md の REQ-compare-010）には、どのコマンドでもこの上限を当てない。
--force を指定したコマンドは、そのコマンドが行うバックアップのための読み込みを含めて、この上限を当てずに読む。
rollback がバックアップの保存先から写しを読むときは、この上限を当てない。rollback で上限を当てるのは、復元の前に書き込み先の今の中身をバックアップするための読み込みである。
status は中身をハッシュで比べるだけなので、この上限は当たらない（REQ-cli-094）。

## Examples

```gherkin
@id=EX-cli-081 @about=REQ-cli-079 @source=docs/decision/records/2026-10-05-quality-cli.md#A2,docs/decision/records/2026-10-05-quality-cli.md#A5
Scenario: 上限を超えるファイルをどの経路でも同じに扱う
Given 左右に 104,857,600 バイトを超える中身の違うファイルがある
When ローカルと SSH の組、ローカルとエージェントの組で、--force なしにそのファイルを指定した diff を実行する
Then どちらの組もそのファイルを読めなかったファイルとして errors に載せ終了コード 2 を返す

@id=EX-cli-082 @about=REQ-cli-079 @source=docs/decision/records/2026-10-05-quality-cli.md#A2
Scenario: --force で上限を外す
Given 左右に 104,857,600 バイトを超える中身の違うファイルがある
When --force を付けてそのファイルを指定した diff を実行する
Then そのファイルの中身が比べられる

@id=EX-cli-083 @about=REQ-cli-079 @source=docs/decision/records/2026-10-05-quality-cli.md#A2
Scenario: --force を付けた削除の前のバックアップ
Given バックアップが有効で、書き込み先にだけ 104,857,600 バイトを超えるファイルがある
When --delete と --force を付けて sync する
Then そのファイルは上限に止められずにバックアップされてから削除される
```
