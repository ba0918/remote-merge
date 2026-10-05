# 書き込み先の権限

マージ・同期時に作成するファイルと既存ファイルの所有者・権限を扱う規則。

## Requirements

### REQ-merge-012: 新規作成は設定された権限にする
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A40, docs/decision/records/2026-10-05-quality-merge.md#A4
- verification: unit

新しいファイルやディレクトリを作るとき、書き込み先に設定された権限で作る。--with-permissions を指定して新しいファイルを作るときは REQ-merge-036 による。

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

### REQ-merge-036: 権限の複製は新しいファイルにも走査した値で行う
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-merge.md#A4
- verification: unit

--with-permissions を指定した merge と sync は、新しく作るファイルにも書き込み先に設定された権限ではなく読み込み元の権限を付け、読み込み元の権限にはローカルとリモートのどちらでも走査したツリーの値を使う。

### REQ-merge-037: 権限の複製に失敗したファイルを失敗として出す
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-merge.md#A4, docs/decision/records/2026-10-05-quality-merge.md#A5
- verification: unit

--with-permissions を指定した merge と sync が書き込み先の権限の変更に失敗したとき、そのファイルを中身は書いたことが分かる error で failed に出し、終了コード 2 を返す。
この error は中身を書いたことを示すが、その文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。

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
```
