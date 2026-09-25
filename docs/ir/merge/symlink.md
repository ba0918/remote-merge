# マージ先の symlink と種類の違い

ファイルやディレクトリをマージ・同期・削除するとき、書き込み先の種類と範囲を守る規則。

## Requirements

### REQ-merge-001: 種類の違う既存の対象を置き換えない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A3
- verification: unit

読み込み元と書き込み先の種類が通常ファイル・ディレクトリ・末尾の symlink の間で異なるとき、merge と sync は書き込み先を変更せず、その対象を理由付きでスキップする。

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
- source: docs/decision/records/2026-09-25-spec-migration.md#A6
- verification: unit

merge と sync は引数が絶対パスまたは ".." を含む相対パスなら、そのパスの書き込みを拒否する。

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

@id=EX-merge-013 @about=REQ-merge-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A6
Scenario: 親ディレクトリを遡る引数
Given マージ対象の引数に親ディレクトリを指す ".." が含まれる
When そのパスをマージする
Then 対象の書き込みは拒否される

@id=EX-merge-014 @about=REQ-merge-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A6
Scenario: 絶対パスの引数
Given マージ対象に絶対パスが渡される
When そのパスをマージする
Then 対象の書き込みは拒否される
```
