# リモートとの接続断

リモートとの `接続断` が操作の途中で起きたときに、やり直す範囲、変更の扱い、その回の処理を止める範囲。

## Requirements

### REQ-ssh-003: 読み取りだけ一度再試行する
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A26, docs/decision/records/2026-10-05-quality-ssh.md#A3, docs/decision/records/2026-10-11-agent-operations.md#A20, docs/decision/records/2026-10-11-agent-operations.md#A21, docs/decision/records/2026-10-11-agent-operations.md#A25, docs/decision/records/2026-10-11-agent-operations.md#A32, docs/decision/records/2026-10-11-agent-operations.md#A35, docs/decision/records/2026-10-11-agent-operations.md#A37, docs/decision/records/2026-10-11-agent-operations-consistency.md#A2, docs/decision/records/2026-10-12-foundation-3.md#D11, docs/decision/records/2026-10-12-foundation-5.md#F23, docs/decision/records/2026-10-12-foundation-5.md#F81
- verification: unit

リモートの読み取りだけの操作（ファイルの読み込み、ツリーの走査、パスの確認、stat、種類の調べ、子の一覧、ハッシュ、書き込み前の確認）の途中で `接続断` が起きたとき、つなぎ直して（エージェントは起動し直して）その操作を一度だけ試し、再び失敗したら（つなぎ直せないときを含む）、その書き込み先への処理（status・diff ではそのコマンド）を止めて終了コード 2 を返す。
merge・sync・rollback で止めたときは、REQ-ssh-022 と同じ形で結果を出す。書き込み先の中身をまとめて読む処理（比べるときに書き込み先のハッシュと中身をまとめて取る処理、変更の前に書き込み先のパスをまとめて確かめる処理、バックアップが無効な rollback が書き戻す前にまとめてハッシュを取る処理を含む）で止めたときは、そのまとめに入っていたファイルをすべて理由付きの読み込みの失敗として failed に出し、1 ファイルずつ読み直さない。

### REQ-ssh-004: 結果不明の変更を自動反復しない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A26, docs/decision/records/2026-10-11-agent-operations.md#A38, docs/decision/records/2026-10-11-agent-operations.md#A8, docs/decision/records/2026-10-11-agent-operations.md#A19, docs/decision/records/2026-10-12-foundation-3.md#D11
- verification: unit

リモートへの書き込み・symlink の作成・削除・権限の変更の最中に `接続断` が起きたとき、同じ変更を自動で繰り返さない。

### REQ-ssh-022: 変更の最中の接続断で、そのファイルを確かめてから書き込み先への処理を止める
- kind: event_driven
- source: docs/decision/records/2026-10-11-agent-operations.md#A38, docs/decision/records/2026-10-11-agent-operations.md#A39, docs/decision/records/2026-10-11-agent-operations.md#A40, docs/decision/records/2026-10-11-agent-operations.md#A41, docs/decision/records/2026-10-11-agent-operations.md#A8, docs/decision/records/2026-10-11-agent-operations.md#A9, docs/decision/records/2026-10-11-agent-operations.md#A21, docs/decision/records/2026-10-11-agent-operations.md#A25, docs/decision/records/2026-10-11-agent-operations.md#A34, docs/decision/records/2026-10-11-agent-operations-consistency.md#A1, docs/decision/records/2026-10-12-foundation-3.md#D11
- verification: unit

merge・sync（--delete を含む）・rollback が、書き込み先への書き込み・symlink の作成・削除・権限の変更の最中に `接続断` に出会ったとき、一度だけつなぎ直して、そのファイルの今の状態を読んで確かめる。
書き込みと symlink の作成は、書こうとしたものと同じなら「書けた」、書く前と同じ（書く前になかったなら、ない）なら「書いていない」、どちらでもなければ「書きかけ」とする。削除は、なければ「消せた」、あれば「消していない」とする。権限の変更は、読んだ権限で「変えた」か「変えていない」とする。
「書けた」「消せた」「変えた」ファイルは済んだファイルとして結果に出し、それ以外はその結果が分かる error で failed に出す。
ただし、「書けた」「消せた」「変えた」の後にそのファイルの変更が残っているとき（新しいファイルや作ったディレクトリの権限の変更、--with-permissions の権限の複製、symlink を置き換えるために消した後の作成）は、残りの変更を行わず、そのファイルを、済んだ変更と手を付けていない変更の両方が分かる error で failed に出す。
「書きかけ」の error には、バックアップを取っていればその場所も出し、自動では書き戻さない。
つなぎ直しか確かめる読み込みが失敗したときだけ、そのファイルを結果が分からないことが分かる error で failed に出す。
前の操作の後に接続が使えなくなっていて変更の要求を送れなかったときも、同じく確かめて出す。
中身を書いた後の権限の変更が `接続断` になったときは、そのファイルの error を、中身は書いたことと確かめた権限の結果の両方が分かるものにする。
どの場合も、その書き込み先でまだ処理していない残りのファイルを変更せずに、手を付けていないことが分かる error で failed に出し、終了コード 2 を返す。済んだファイルはそのまま結果に出す。
`接続断` が起きないときは、確かめるための要求を増やさない。ただし、バックアップが無効な rollback では、書き戻す前に、書き戻す先にある通常ファイルのハッシュを、書き戻すファイルの数によらずまとめて取り、確かめの「書く前」に使う。取ったものは保存しない。
sync の他の書き込み先は REQ-merge-015 のとおり処理を続ける。
`接続断` でない失敗（相手が失敗を返した）は、そのファイルだけを failed に出して残りを続ける。
error の文言は IR で決めず、実装者が英語で書く。テストは、書いていない・書きかけ・結果が分からない・手を付けていない・中身は書いたことを区別できることを確かめ、文言を固定しない。

## Examples
```gherkin
@id=EX-ssh-005 @about=REQ-ssh-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A26
Scenario: 一度の読み取り断から回復する
Given リモートの読み取り中に一度だけ接続が切れる
When 同じファイルを読み直す
Then 再接続後の読み取り結果が返る

@id=EX-ssh-006 @about=REQ-ssh-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A26
Scenario: 再接続後も読み取れない
Given リモートの読み取りが再接続後にも失敗する
When ファイルを読み取る
Then 二回目の失敗が報告され自動再試行が続かない

@id=EX-ssh-007 @about=REQ-ssh-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A26,docs/decision/records/2026-10-11-agent-operations.md#A38
Scenario: 書き込み結果が不明である
Given リモートの書き込み中に接続が切れる
When マージ結果を判定する
Then 同じ更新は自動再実行されず、書き込み先を確かめた結果が報告される


@id=EX-ssh-019 @about=REQ-ssh-003 @source=docs/decision/records/2026-10-05-quality-ssh.md#A3
Scenario: 走査の途中で一度だけ切れる
Given リモートの走査の途中で一度だけ接続が切れる
When status を実行する
Then 再接続して走査を一度やり直し、やり直した走査の一覧で比べる

@id=EX-ssh-038 @about=REQ-ssh-004 @source=docs/decision/records/2026-10-11-agent-operations.md#A38
Scenario: エージェントへの書き込みの応答が来ない
Given エージェントが "a.txt" の書き込みの要求を受けて書く前に、応答を返さずに接続が切れる
When "a.txt" を merge する
Then 同じ書き込みは繰り返されず、エージェントを起動し直して確かめ、"a.txt" は書いていない失敗になる

@id=EX-ssh-039 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A38
Scenario: 三つのファイルの merge の二つ目で切れる
Given "a.txt"・"b.txt"・"c.txt" を merge し、"b.txt" の書き込みの要求が書かれる前に接続が切れる
When merge の結果を見る
Then "a.txt" は書き込まれ、"b.txt" は書いていない失敗、"c.txt" は手を付けていない失敗として failed に出て、"b.txt" と "c.txt" は書き込み先で変わらず、終了コード 2 を返す

@id=EX-ssh-040 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A8,docs/decision/records/2026-10-11-agent-operations.md#A38,docs/decision/records/2026-10-11-agent-operations.md#A32
Scenario: 1 ファイルの書き込みが失敗しただけ
Given "a.txt"・"b.txt"・"c.txt" を merge し、書き込み先の "b.txt" に書き込む権限がない
When merge の結果を見る
Then "b.txt" だけが failed に出て、"a.txt" と "c.txt" は書き込まれ、終了コード 2 を返す



@id=EX-ssh-043 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A9,docs/decision/records/2026-10-11-agent-operations.md#A38
Scenario: --delete の削除の最中に切れる
Given 書き込み先にだけ "x.txt" と "y.txt" があり、--delete の sync で "x.txt" の削除の要求が行われる前に接続が切れる
When sync の結果を見る
Then "x.txt" は消していない失敗、"y.txt" は手を付けていない失敗として failed に出て、"x.txt" と "y.txt" は書き込み先に残り、終了コード 2 を返す

@id=EX-ssh-044 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A9,docs/decision/records/2026-10-11-agent-operations.md#A38
Scenario: rollback の書き戻しの最中に切れる
Given リモートの書き込み先へ "a.txt" と "b.txt" を書き戻す rollback で、"a.txt" の書き戻しの要求が行われる前に接続が切れる
When rollback の結果を見る
Then "a.txt" は書いていない失敗、"b.txt" は手を付けていない失敗として failed に出て、終了コード 2 を返す

@id=EX-ssh-045 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A21,docs/decision/records/2026-10-11-agent-operations.md#A38,docs/decision/records/2026-10-11-agent-operations.md#A32
Scenario: 二つの書き込み先の sync で一方だけ切れる
Given 書き込み先 "s1" と "s2" への sync で、"s1" への書き込みの最中に接続が切れる
When sync の結果を見る
Then "s1" の残りのファイルは手を付けていない失敗になり、"s2" へは全てのファイルが書き込まれ、終了コード 2 を返す

@id=EX-ssh-046 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A38
Scenario: 次の変更の前にエージェントが終わっている
Given "a.txt"・"b.txt"・"c.txt" を merge し、"a.txt" を書いた後にエージェントが終わっている
When merge の結果を見る
Then "a.txt" は書き込まれ、"b.txt" は書いていない失敗、"c.txt" は手を付けていない失敗として failed に出て、終了コード 2 を返す

@id=EX-ssh-047 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A38
Scenario: 中身を書いた後の権限の変更の最中に切れる
Given 新しいファイル "a.txt" を merge し、中身を書いた後の権限の変更の要求が行われる前に接続が切れる
When merge の結果を見る
Then "a.txt" は中身は書いたことと権限は変えていないことの両方が分かる error で failed に出て、終了コード 2 を返す

@id=EX-ssh-048 @about=REQ-ssh-003 @source=docs/decision/records/2026-10-11-agent-operations.md#A20,docs/decision/records/2026-10-11-agent-operations.md#A25,docs/decision/records/2026-10-11-agent-operations.md#A32
Scenario: やり直した読み取りもまた切れる
Given "a.txt"・"b.txt" を merge し、"a.txt" の読み取りが再接続の後にも接続断で失敗する
When merge の結果を見る
Then "a.txt" は理由付きで、"b.txt" は手を付けていない失敗として failed に出て、どちらも書き込まれず、終了コード 2 を返す

@id=EX-ssh-050 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A38,docs/decision/records/2026-10-11-agent-operations.md#A32
Scenario: 書けた後、応答の前に切れる
Given 書き込み先にある "a.txt" と "b.txt" を merge し、"a.txt" の書き込みが終わった後、その応答を返す前に接続が切れる
When merge の結果を見る
Then "a.txt" は書き込まれた結果に出て、"b.txt" は手を付けていない失敗として failed に出て、終了コード 2 を返す

@id=EX-ssh-054 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations-consistency.md#A1,docs/decision/records/2026-10-11-agent-operations.md#A38
Scenario: 新しいファイルを書けた後、応答の前に切れる
Given 書き込み先にない "a.txt" と書き込み先にある "b.txt" を merge し、"a.txt" の書き込みが終わった後、その応答を返す前に接続が切れる
When merge の結果を見る
Then "a.txt" は中身は書いたことと権限の変更に手を付けていないことの両方が分かる error で、"b.txt" は手を付けていない失敗として failed に出て、終了コード 2 を返す

@id=EX-ssh-051 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A38,docs/decision/records/2026-10-11-agent-operations.md#A39
Scenario: 分けて送る大きな書き込みの途中で切れる
Given バックアップを有効にして大きな "big.bin" を merge し、分けて送る書き込みの途中で接続が切れて、書き込み先に途中までの中身が残る
When merge の結果を見る
Then "big.bin" は書きかけであることとバックアップの場所が分かる error で failed に出て、書き戻されず、終了コード 2 を返す

@id=EX-ssh-052 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A38
Scenario: 切れた後につなぎ直せない
Given "a.txt"・"b.txt" を merge し、"a.txt" の書き込みの最中に接続が切れ、つなぎ直しもできない
When merge の結果を見る
Then "a.txt" は結果が分からない失敗、"b.txt" は手を付けていない失敗として failed に出て、終了コード 2 を返す

@id=EX-ssh-053 @about=REQ-ssh-022 @source=docs/decision/records/2026-10-11-agent-operations.md#A40,docs/decision/records/2026-10-11-agent-operations.md#A38
Scenario: バックアップが無効な rollback の書き戻しの途中で切れる
Given バックアップを無効にして、リモートの書き込み先にある "a.txt" と "b.txt" を書き戻す rollback で、"a.txt" の書き戻しの要求が行われる前に接続が切れる
When rollback の結果を見る
Then "a.txt" は書いていない失敗、"b.txt" は手を付けていない失敗として failed に出て、結果が分からない失敗にはならず、終了コード 2 を返す
```
