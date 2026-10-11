# マージ先の symlink と種類の違い

ファイルやディレクトリをマージ・同期・削除するとき、書き込み先の種類と範囲を守る規則。

## Requirements

### REQ-merge-001: 種類の違う既存の対象を置き換えない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A3, docs/decision/records/2026-10-12-skip-subtree.md#A1, docs/decision/records/2026-10-12-skip-subtree.md#A2, docs/decision/records/2026-10-12-skip-subtree.md#A3, docs/decision/records/2026-10-12-skip-subtree.md#A4, docs/decision/records/2026-10-12-skip-subtree-consistency.md#A1
- verification: unit

読み込み元と書き込み先の種類が通常ファイル・ディレクトリ・末尾の symlink の間で異なるとき、merge と sync は書き込み先を変更せず、その対象を理由付きでスキップする。
ただし、ディレクトリと、ディレクトリへの symlink の組は種類の違いとせず、組そのものは置き換えず結果にも出さず、その中のパスをふつうに比べて merge・sync・--delete の対象にする（書き込み先の側が symlink なら REQ-merge-004 による）。
スキップした対象の配下は、--dry-run を含めて書かず、--delete でも消さない。結果の skipped にはスキップした対象を一件だけ出し、配下は出さない。
--delete を付けて書き込み先にだけある配下のパスを直接指定し、その祖先が読み込み元と書き込み先で種類が違うときも、その祖先をスキップした対象として扱い、そのパスを消さず、skipped には祖先を一件だけ出す。読み込み元の配下のパスを直接指定し、書き込み先の祖先がディレクトリでないときは REQ-paths-011 による。

### REQ-merge-002: 削除対象の symlink を残す
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A3
- verification: unit

merge と sync の削除指定で書き込み先だけに末尾の symlink があるとき、リンクを削除せず理由付きでスキップする。

### REQ-merge-003: 同じ種類の symlink はリンク先を更新する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A3
- verification: unit

読み込み元と書き込み先がともに末尾の symlink のとき、merge と sync は書き込み先の symlink のリンク先を読み込み元に合わせて更新する。

### REQ-merge-004: 意図した共有リンクの先に書き込める
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A6
- verification: unit

書き込み先の途中に既存の symlink があるとき、merge と sync はリンク先が root_dir の外でもローカル・SSH・エージェントの全経路でリンク先の既存ファイルをバックアップしてから更新する。

### REQ-merge-007: 引数によるパス脱出を拒否する
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A6, docs/decision/records/2026-10-05-quality-merge.md#A1
- verification: unit

merge と sync は、パスの引数に絶対パスか ".." の要素を含むパスがあるとき、ローカル・SSH・エージェントのどの経路でも、走査と書き込みの前にコマンド全体を絶対パスなら "absolute subpath not allowed: パス"、".." なら "path traversal not allowed: パス" のエラーで止め、終了コード 2 を返し、どのファイルも書き込まない。root_dir 全体を指す "/" の引数は従来どおり受け付ける。

### REQ-merge-023: 書き込み先にない symlink はそのまま作る
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A1
- verification: unit

読み込み元が末尾の symlink で書き込み先にそのパスがないとき、merge と sync は書き込み先に同じリンク先の symlink を作り、リンクの先へは辿らない。

## Examples

```gherkin
@id=EX-merge-001 @about=REQ-merge-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A3
Scenario: 通常ファイルの上書き先が symlink である
Given 読み込み元が通常ファイルで書き込み先が末尾の symlink である
When そのパスをマージする
Then 書き込み先のリンクとリンク先のファイルは変わらずスキップされる

@id=EX-merge-002 @about=REQ-merge-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A3,docs/decision/records/2026-09-25-spec-migration.md#A53
Scenario: 同じ種類の通常ファイルはマージできる
Given 元と先が通常ファイルで中身が異なり読み取り・確認・バックアップが成功し競合や外部更新がない
When 書き込み元と先を明示してそのパスをマージする
Then 書き込み先は読み込み元の中身に更新される

@id=EX-merge-003 @about=REQ-merge-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A3
Scenario: 削除候補の symlink は残る
Given 書き込み先だけに末尾の symlink がある
When 削除指定で同期する
Then symlink は残り理由付きでスキップされる

@id=EX-merge-004 @about=REQ-merge-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A3,docs/decision/records/2026-09-25-spec-migration.md#A18
Scenario: 削除対象が symlink でない
Given 書き込み先だけに通常ファイルがある
When 削除指定で同期する
Then 通常ファイルは削除される

@id=EX-merge-005 @about=REQ-merge-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A3
Scenario: symlink 同士のリンク先を揃える
Given 読み込み元と書き込み先の symlink が異なるリンク先を指している
When そのパスをマージする
Then 書き込み先の symlink 自体が読み込み元のリンク先を指す

@id=EX-merge-006 @about=REQ-merge-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A3
Scenario: リンク先が同じ symlink 同士
Given 読み込み元と書き込み先の symlink が同じリンク先を指している
When そのパスをマージする
Then 書き込み先のリンク先は変わらない

@id=EX-merge-007 @about=REQ-merge-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A6
Scenario: 途中の symlink が共有ディレクトリを指す
Given 書き込み先の親ディレクトリが root_dir の外を指す symlink である
When その配下のファイルをマージする
Then ローカル・SSH・エージェントのどの経路でも既存ファイルはバックアップされリンク先の内容が更新される

@id=EX-merge-008 @about=REQ-merge-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A6
Scenario: 途中の symlink が範囲内を指す
Given 書き込み先の親ディレクトリが root_dir の中を指す symlink である
When その配下のファイルをマージする
Then root_dir 内のファイルは更新される

@id=EX-merge-013 @about=REQ-merge-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A6,docs/decision/records/2026-10-05-quality-merge.md#A1
Scenario: 親ディレクトリを遡る引数
Given マージ対象の引数に親ディレクトリを指す ".." が含まれる
When そのパスをマージする
Then 対象の書き込みは拒否される

@id=EX-merge-014 @about=REQ-merge-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A6,docs/decision/records/2026-10-05-quality-merge.md#A1
Scenario: 絶対パスの引数
Given マージ対象に絶対パスが渡される
When そのパスをマージする
Then 対象の書き込みは拒否される

@id=EX-merge-040 @about=REQ-merge-007 @source=docs/decision/records/2026-10-05-quality-merge.md#A1
Scenario: 正しいパスと ".." を含むパスを一緒に渡す
Given 引数に正しいパス "a.txt" と "../b.txt" がある
When その二つをマージする
Then "path traversal not allowed: ../b.txt" のエラーで止まり "a.txt" も書き込まれず終了コード 2 を返す

@id=EX-merge-049 @about=REQ-merge-023 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A10
Scenario: リンク先が "-" で始まる symlink を作る
Given 読み込み元の symlink のリンク先が "--target-directory=sub" で、書き込み先にそのパスがなく "sub" ディレクトリがある
When そのパスをマージする
Then 書き込み先のそのパスにリンク先が "--target-directory=sub" の symlink ができ、"sub" の中には何も作られない

@id=EX-merge-057 @about=REQ-merge-001 @source=docs/decision/records/2026-10-12-skip-subtree.md#A1,docs/decision/records/2026-10-12-skip-subtree.md#A2
Scenario: 読み込み元のディレクトリの配下を、書き込み先の通常ファイルの下に書かない
Given 読み込み元の "x" は "x/a.txt" を持つディレクトリで、書き込み先の "x" は通常ファイルである
When "x" を含む範囲を merge する
Then 書き込み先の "x" は変わらず、skipped には "x" だけが種類の違いの理由で出る

@id=EX-merge-058 @about=REQ-merge-001,REQ-merge-004 @source=docs/decision/records/2026-10-12-skip-subtree.md#A4
Scenario: 書き込み先のディレクトリへの symlink の中をディレクトリとして比べる
Given 読み込み元の "linked" は "file.txt" を持つディレクトリで、書き込み先の "linked" は root_dir の外のディレクトリを指す symlink で、その先に中身の違う "file.txt" がある
When "linked" を含む範囲を merge する
Then リンク先の "file.txt" はバックアップされてから読み込み元の中身に更新され、書き込み先の "linked" は symlink のまま残り、skipped に "linked" は出ない

@id=EX-merge-059 @about=REQ-merge-001 @source=docs/decision/records/2026-10-12-skip-subtree.md#A3,docs/decision/records/2026-10-12-skip-subtree.md#A2
Scenario: 種類の違いでスキップしたパスの配下を --delete で消さない
Given 読み込み元の "x" は通常ファイルで、書き込み先の "x" は、読み込み元にない "x/only.txt" を持つディレクトリである
When --delete を付けて "x" を含む範囲を merge する
Then 書き込み先の "x/only.txt" は消えず、skipped には "x" だけが出る

@id=EX-merge-060 @about=REQ-merge-001 @source=docs/decision/records/2026-10-12-skip-subtree.md#A4
Scenario: 読み込み元のディレクトリへの symlink の中を、書き込み先のディレクトリと比べる
Given 読み込み元の "linked" はディレクトリへの symlink で、その先に "file.txt" があり、書き込み先の "linked" は中身の違う "linked/file.txt" を持つディレクトリである
When "linked" を含む範囲を merge する
Then 書き込み先の "linked/file.txt" は読み込み元の中身に更新され、書き込み先の "linked" はディレクトリのまま残り、skipped に "linked" は出ない

@id=EX-merge-061 @about=REQ-merge-001 @source=docs/decision/records/2026-10-12-skip-subtree-consistency.md#A1,docs/decision/records/2026-10-12-skip-subtree.md#A3
Scenario: 種類の違う祖先の下の、書き込み先だけのパスを直接指定しても --delete で消さない
Given 読み込み元の "x" は通常ファイルで、書き込み先の "x" は、読み込み元にない "x/only.txt" を持つディレクトリである
When --delete を付けて "x/only.txt" を merge する
Then 書き込み先の "x/only.txt" は消えず、skipped には "x" だけが種類の違いの理由で出る
```
