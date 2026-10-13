# 書き込み先の権限

マージ・同期時に作成するファイルと既存ファイルの所有者・権限を扱う規則。

## Requirements

### REQ-merge-012: 新規作成は設定された権限にする
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A40, docs/decision/records/2026-10-05-quality-merge.md#A4, docs/decision/records/2026-10-11-agent-operations.md#A42, docs/decision/records/2026-10-11-agent-operations-consistency-3.md#A2
- verification: unit

新しいファイルやディレクトリを作るとき、書き込み先に設定された権限で作る。--with-permissions を指定して新しいファイルを作るときは REQ-merge-036 による。
新しいディレクトリが作られたときに親から受け継いだ setuid・setgid は、REQ-paths-018 と同じく、ローカル・SSH・エージェントのどの経路でも残し、0o777 の範囲のビットを設定された権限にする。

### REQ-merge-013: 既存ファイルの権限を保持する
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A40
- verification: unit

既存ファイルをマージ・同期したとき、--with-permissions を指定していなければ書き込み先の所有者と権限を維持する。

### REQ-merge-014: 権限の複製は明示する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A40, docs/decision/records/2026-10-11-permission-copy.md#A1
- verification: unit

--with-permissions を指定したときだけ、読み込み元のファイル権限を書き込み先に反映する。

### REQ-merge-036: 権限の複製は新しいファイルにも走査した値で行う
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-merge.md#A4
- verification: unit

--with-permissions を指定した merge と sync は、新しく作るファイルにも書き込み先に設定された権限ではなく読み込み元の権限を付け、読み込み元の権限にはローカルとリモートのどちらでも走査したツリーの値を使う。

### REQ-merge-037: 権限の複製に失敗したファイルを失敗として出す
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-merge.md#A4, docs/decision/records/2026-10-05-quality-merge.md#A5, docs/decision/records/2026-10-11-permission-copy.md#A2, docs/decision/records/2026-10-11-agent-operations.md#A38, docs/decision/records/2026-10-11-agent-operations.md#A22
- verification: unit

--with-permissions を指定した merge と sync が書き込み先の権限の変更に失敗したとき、そのファイルを中身は書いたことが分かる error で failed に出し、終了コード 2 を返す。
この error は中身を書いたことを示すが、その文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。
読み込み元の権限が分からないとき（走査したツリーに権限の値がない）は、権限を写さず、中身は書いたことと権限を写せなかったことが分かる error で同じく failed に出し、終了コード 2 を返す。
読み込み元の権限に 0o777 を超えるビット（setuid・setgid・sticky）があるときも、権限を写さず、中身は書いたことと特別なビットのために写さなかったことが分かる error で同じく failed に出し、終了コード 2 を返す。
`接続断` のときは、そのファイルを含めて REQ-ssh-022・REQ-ssh-003・REQ-ssh-023 による。

### REQ-merge-042: 新しく作るものはログインした利用者の持ち物になる
- kind: ubiquitous
- source: docs/decision/records/2026-10-12-foundation-3.md#D36, docs/decision/records/2026-10-12-foundation-3.md#D39, docs/decision/records/2026-10-12-foundation-3.md#D43, docs/decision/records/2026-10-12-foundation-3.md#D47
- verification: unit

merge・sync・rollback が新しく作るファイル・symlink・ディレクトリは、どの経路でも、sudo=true を指定したかどうかによらず、ログインした利用者（ローカルの経路では手元の利用者）の持ち物になる。
所有者を付けられないときと、親ディレクトリを作った後に書き込み先のファイルか symlink を作れなかったときは、どの経路でも、作ったものを消して何も変えずにそのファイルを失敗にし、消せないときは残ったものを示す error で失敗にする。
作ったものとして所有者を付けて消すのは、その書き込みか symlink の作成が作ったと確かめられるもの（新しく作れたファイルとリンク、そのとき自分で作れたディレクトリ）だけで、同じ間に別の誰かが作ったものと、作った後に差し替えられたファイルは、所有者を変えず消さない。
ない親ディレクトリは root_dir より下にだけ作り、root_dir がない（ディレクトリでない）ときは、どの経路でも root_dir もその親も作らずに、そのファイルを root_dir がない失敗（"root_dir is gone: サーバ名:パス"）にする。
作ったものを消そうとしたときに、すでに消えていたもの（リンク切れの symlink を含めて何もないもの）は消せたものとし、別のもの（ディレクトリでないもの、別のファイル）に替わっていたものは消さずに残ったものとして示す。

## Examples

```gherkin
@id=EX-merge-024 @about=REQ-merge-012 @source=docs/decision/records/2026-09-25-spec-migration.md#A40
Scenario: 新しいファイルを作る
Given 書き込み先に新規ファイルの権限が設定されている
When 存在しないファイルをマージする
Then 新規ファイルには書き込み先の設定した権限が付く

@id=EX-merge-025 @about=REQ-merge-012 @source=docs/decision/records/2026-09-25-spec-migration.md#A40
Scenario: 新しいディレクトリを作る
Given 書き込み先に新規ディレクトリの権限が設定されている
When 配下にないディレクトリへファイルを同期する
Then 新規ディレクトリには設定した権限が付く

@id=EX-merge-052 @about=REQ-merge-012 @source=docs/decision/records/2026-10-11-agent-operations.md#A42,docs/decision/records/2026-10-11-agent-operations-consistency-3.md#A2
Scenario: setgid の親の下に新しいディレクトリを作る
Given 書き込み先の新規ディレクトリの権限が 0o775 に設定され、書き込み先の "shared" の権限が 0o2775 で、"shared/new" はない
When ローカル・SSH・エージェントのそれぞれの経路で "shared/new/a.txt" を merge する
Then どの経路でも作られた "shared/new" の権限は 0o2775 になる

@id=EX-merge-026 @about=REQ-merge-013 @source=docs/decision/records/2026-09-25-spec-migration.md#A40
Scenario: 権限複製を指定せず上書きする
Given 書き込み元と先のファイル権限が違う
When 権限複製を指定せずマージする
Then 書き込み先の所有者と権限は変わらない

@id=EX-merge-027 @about=REQ-merge-013 @source=docs/decision/records/2026-09-25-spec-migration.md#A40
Scenario: 元と同じ権限の既存ファイル
Given 書き込み元と先のファイル権限が同じである
When 権限複製を指定せずマージする
Then 書き込み先の所有者と権限は維持される

@id=EX-merge-028 @about=REQ-merge-014 @source=docs/decision/records/2026-09-25-spec-migration.md#A40
Scenario: 権限複製を指定する
Given 書き込み元と先のファイル権限が違う
When --with-permissions でマージする
Then 書き込み先のファイル権限は元に揃う

@id=EX-merge-029 @about=REQ-merge-014 @source=docs/decision/records/2026-09-25-spec-migration.md#A40
Scenario: 権限複製を指定しない
Given 書き込み元と先のファイル権限が違う
When --with-permissions なしでマージする
Then 書き込み先のファイル権限は維持される

@id=EX-merge-043 @about=REQ-merge-036 @source=docs/decision/records/2026-10-05-quality-merge.md#A4
Scenario: リモートの読み込み元から新しいファイルを権限付きで作る
Given リモートの読み込み元のファイルの権限が 0o600 で、書き込み先にはそのファイルがなく 0o664 が設定されている
When --with-permissions でマージする
Then 書き込み先に作ったファイルの権限は 0o600 になる

@id=EX-merge-044 @about=REQ-merge-037 @source=docs/decision/records/2026-10-05-quality-merge.md#A4
Scenario: 権限の変更が拒まれる
Given 書き込み先のファイルの中身は書けるが権限は変えられない
When --with-permissions でマージする
Then そのファイルは中身を書いたことが分かる error で failed に出て終了コード 2 を返す

@id=EX-merge-051 @about=REQ-merge-037 @source=docs/decision/records/2026-10-11-agent-operations.md#A22
Scenario: 読み込み元に setuid がある
Given ローカルの "run.sh" の権限が 0o4755 で、書き込み先の "run.sh" の権限が 0o644 である
When --with-permissions で "run.sh" を merge する
Then 中身は書き込まれ、権限は 0o644 のまま変わらず、"run.sh" は中身は書いたことと特別なビットのために写さなかったことが分かる error で failed に出て、終了コード 2 を返す

@id=EX-merge-056 @about=REQ-merge-014 @source=docs/decision/records/2026-10-11-permission-copy.md#A1
Scenario: 読み込み元の権限が 0 のファイルを権限ごと写す
Given 読み込み元の "a.txt" の権限が 0 で、書き込み先の "a.txt" の権限が 0o644 で中身が違う
When --with-permissions を付けて "a.txt" を merge する
Then 書き込み先の "a.txt" の権限は 0 になる

@id=EX-merge-063 @about=REQ-merge-042 @source=docs/decision/records/2026-10-12-foundation-3.md#D36
Scenario: 三つの経路で新しいディレクトリの下に新しいファイルを作る
Given ローカル・SSH・エージェントの書き込み先に "new" ディレクトリがない
When それぞれの経路で "new/a.txt" を merge する
Then どの経路でも "new" と "new/a.txt" の所有者は書いた利用者である

@id=EX-merge-064 @about=REQ-merge-042 @source=docs/decision/records/2026-10-12-foundation-3.md#D36
Scenario: sudo=true のサーバで新しいディレクトリの下に新しいファイルを作る
Given sudo=true のサーバの root の持ち物の root_dir に "new" ディレクトリがない
When "new/a.txt" を merge する
Then "new" と "new/a.txt" の所有者はログインした利用者である
```
