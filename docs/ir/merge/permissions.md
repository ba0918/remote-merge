# 書き込み先の権限

マージ・同期時に作成するファイルと既存ファイルの所有者・権限を扱う規則。

## Requirements

### REQ-merge-012: 新規作成は設定された権限にする
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A40
- verification: unit

新しいファイルやディレクトリを作るとき、書き込み先に設定された権限で作る。

### REQ-merge-013: 既存ファイルの権限を保持する
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A40
- verification: unit

既存ファイルをマージ・同期したとき、--with-permissions を指定していなければ書き込み先の所有者と権限を維持する。

### REQ-merge-014: 権限の複製は明示する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A40
- verification: unit

--with-permissions を指定したときだけ、読み込み元のファイル権限を書き込み先に反映する。

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
```
