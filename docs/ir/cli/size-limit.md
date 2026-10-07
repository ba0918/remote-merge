# ファイルの大きさの上限

比較・書き込み・バックアップのためにファイルの中身を読むときの大きさの上限と、それを外す指定。status は中身をメモリに持たずに少しずつ読んでハッシュで比べるので、この上限の対象にしない（cli/status.md の REQ-cli-094）。

## Requirements

### REQ-cli-079: すべての操作に一つの大きさの上限を読む前に当てる
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A2, docs/decision/records/2026-10-05-quality-cli.md#A12, docs/decision/records/2026-10-05-size-limit.md#A7, docs/decision/records/2026-10-07-size-limit-consistency.md#A2
- verification: unit

diff・merge・sync・rollback は、ローカル・SSH・エージェントのどの経路でも、比較・書き込み・バックアップのためにファイルの中身を読む前にその大きさを調べ、104,857,600 バイトを超えるファイルは中身を読まずに、各コマンドの中身を読めなかったファイルの扱いに従う。
--force を指定したコマンドは、そのコマンドが行うバックアップのための読み込みを含めて、この上限を当てずに読む。
rollback がバックアップの保存先から写しを読むときは、この上限を当てない。rollback で上限を当てるのは、復元の前に書き込み先の今の中身をバックアップするための読み込みである。
status はこの上限を当てず、REQ-cli-094 による。

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
