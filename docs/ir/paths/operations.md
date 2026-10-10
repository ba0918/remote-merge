# 経路ごとの削除・権限の変更・子の一覧

ローカル・SSH・エージェントの経路が、削除・権限の変更・子の一覧を同じ結果で行うこと、受け付けない権限の値、失敗の伝え方、一覧の各項目の値。

## Requirements

### REQ-paths-013: 三つの操作はどの経路も同じ結果にする
- kind: invariant
- source: docs/decision/records/2026-10-11-agent-operations.md#A7, docs/decision/records/2026-10-11-agent-operations.md#A12, docs/decision/records/2026-10-11-agent-operations.md#A16, docs/decision/records/2026-10-11-agent-operations.md#A17, docs/decision/records/2026-10-11-agent-operations.md#A31
- verification: unit

同じファイルシステムの状態と同じパスに対して、同じ利用者・同じ権限で行う削除・権限の変更・子の一覧は、ローカル・SSH・エージェントのどの経路でも同じ成否になり、同じものを変える。
途中の symlink は、root_dir の外を指していても辿る。
削除は、末尾の symlink ならリンクそのものを消してリンク先を変えず、ディレクトリとないパスは消さずに失敗する。--delete は末尾の symlink を REQ-merge-002 のとおりスキップするので、末尾の symlink の削除は merge と rollback が symlink を置き換えるときに起きる。
末尾の "/" は除いたパスとして扱う。
子の一覧は、ディレクトリとディレクトリへの symlink ならその直下の子を返す。
ただし SSH の経路の子の一覧は、名前に TAB か改行を含む子（FLAG-paths-001）と exclude の当て方（FLAG-paths-002）で、まだ他の経路と違う。

### REQ-paths-015: 0o777 を超える権限は付けない
- kind: prohibition
- source: docs/decision/records/2026-10-11-agent-operations.md#A13, docs/decision/records/2026-10-11-agent-operations.md#A22
- verification: unit

権限の変更は、どの経路でも、0o777 を超える値（setuid・setgid・sticky を含む）を付けずにエラーにする。--with-permissions で読み込み元の権限に 0o777 を超えるビットがあるときは REQ-merge-037 による。

### REQ-paths-016: 三つの操作の失敗は操作・パス・原因を示す
- kind: ubiquitous
- source: docs/decision/records/2026-10-11-agent-operations.md#A14
- verification: unit

削除・権限の変更・子の一覧が失敗したとき、どの経路でも、そのエラーの文言は行おうとした操作、パス、失敗の原因を含む。文言そのものは IR で決めず、実装者が英語で書く。テストは三つを含むことを確かめ、文言を固定しない。

### REQ-paths-017: 子の一覧の項目の値は経路によらない
- kind: invariant
- source: docs/decision/records/2026-10-11-agent-operations.md#A15, docs/decision/records/2026-10-11-agent-operations.md#A17, docs/decision/records/2026-10-11-agent-operations.md#A23, docs/decision/records/2026-10-11-agent-operations.md#A36, docs/decision/records/2026-10-11-agent-operations.md#A31
- verification: unit

子の一覧は、どの経路でも、各項目の種類、大きさ、権限（0o7777 の範囲の権限のビット）、更新時刻（ナノ秒まで）、symlink のリンク先（読んだ文字列そのまま）を同じ値で返す。
symlink のリンク先か更新時刻を読めないときは、走査の REQ-scan-015 と同じく、既定の値で埋めずにエラーで止まる。
エージェントの子の一覧は、exclude をローカルの経路と同じく、直下の子の root_dir からのパスに当てる。
SSH の経路の更新時刻は、リモートの find が出した桁までとし、秒しか出さない find でナノ秒を記録するファイルシステムを一覧したときの差は FLAG-paths-003 に残す。

## Examples

```gherkin
@id=EX-paths-026 @about=REQ-paths-013 @source=docs/decision/records/2026-10-11-agent-operations.md#A7,docs/decision/records/2026-10-11-agent-operations.md#A31
Scenario: 三つの経路で末尾の symlink を削除する
Given ローカル・SSH・エージェントの root_dir に、同じくファイル "t.txt" を指す symlink "l" がある
When それぞれの経路で "l" を削除する
Then どの経路でも "l" だけが消え、"t.txt" は残る

@id=EX-paths-027 @about=REQ-paths-013 @source=docs/decision/records/2026-10-11-agent-operations.md#A7,docs/decision/records/2026-10-11-agent-operations.md#A31
Scenario: 三つの経路でディレクトリを削除しようとする
Given ローカル・SSH・エージェントの root_dir に、同じくディレクトリ "d" がある
When それぞれの経路で "d" を削除する
Then どの経路でも失敗し、"d" とその中身は残る

@id=EX-paths-028 @about=REQ-paths-013 @source=docs/decision/records/2026-10-11-agent-operations.md#A7
Scenario: root_dir の外を指す途中の symlink の先の権限を変える
Given root_dir の "shared" が root_dir の外のディレクトリを指し、その中に "a.txt" がある
When ローカル・SSH・エージェントの経路で "shared/a.txt" の権限を 0o640 に変える
Then どの経路でも成功し、root_dir の外の "a.txt" の権限が 0o640 になる

@id=EX-paths-031 @about=REQ-paths-015 @source=docs/decision/records/2026-10-11-agent-operations.md#A13
Scenario: setuid の付いた値
Given root_dir に "a.sh" がある
When ローカル・SSH・エージェントの経路で "a.sh" の権限を 0o4755 に変える
Then どの経路でもエラーになり、"a.sh" の権限は変わらない

@id=EX-paths-032 @about=REQ-paths-016 @source=docs/decision/records/2026-10-11-agent-operations.md#A14
Scenario: ないパスの権限を変える
Given root_dir に "missing.txt" がない
When ローカル・SSH・エージェントの経路で "missing.txt" の権限を変える
Then どの経路でもエラーの文言は権限の変更であること、"missing.txt"、ないことを含む

@id=EX-paths-033 @about=REQ-paths-017 @source=docs/decision/records/2026-10-11-agent-operations.md#A15,docs/decision/records/2026-10-11-agent-operations.md#A31
Scenario: 同じディレクトリを三つの経路で一覧する
Given ローカル・SSH・エージェントの root_dir に、同じくナノ秒まで更新時刻の違うファイル "a.txt"、権限 0o2755 のディレクトリ "s"、リンク先が " t " の symlink "l" がある
When それぞれの経路で root_dir の子を一覧する
Then どの経路でも三つの項目の種類・大きさ・権限・ナノ秒までの更新時刻・リンク先 " t " が一致する

@id=EX-paths-034 @about=REQ-paths-017 @source=docs/decision/records/2026-10-11-agent-operations.md#A23,docs/decision/records/2026-10-11-agent-operations.md#A14
Scenario: リンク先を読めない symlink のある一覧
Given ローカルの経路で一覧するディレクトリの中の symlink "l" について、単体テストでリンク先の読み取りの失敗を差し込む
When そのディレクトリの子を一覧する
Then 一覧は "???" をリンク先にした項目を返さず、"l" とリンク先を読めないことを示すエラーで止まる
```
