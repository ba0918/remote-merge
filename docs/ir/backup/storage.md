# バックアップの保存と失敗時の書き込み

書き換え前の内容の保存先と、バックアップが失敗したファイルの扱い。

## Requirements

### REQ-backup-001: バックアップは集約先だけに保存する
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A9
- verification: unit

バックアップが有効なとき、merge・sync・rollback で書き換える前の内容は利用者マシンの専用領域にだけ保存し、書き込み先の root_dir 配下には作らない。

### REQ-backup-002: バックアップできないファイルは書き込まない
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A10
- verification: unit

バックアップが有効で書き換え前の内容を保存できないファイルは書き込まず失敗として報告し、同じ操作の他のファイルの処理は続ける。

### REQ-backup-004: 集約先を XDG 規約で決める
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A12
- verification: unit

XDG_DATA_HOME が絶対パスならバックアップはその下の "remote-merge/backups/" に保存し、未設定・空・相対値ならホームの ".local/share/remote-merge/backups/" に保存する。

### REQ-backup-005: 無効化したバックアップは作らない
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A13
- verification: unit

バックアップが明示的に無効の場合、merge と sync は復元可能なコピーを作らず、バックアップの有無を理由に書き込みを拒否しない。

### REQ-backup-011: 集約先は所有者だけが読み書きできる
- kind: invariant
- source: docs/decision/records/2026-09-27-adopt-backup.md#A9
- verification: unit

集約先の中に作るディレクトリは 0700、ファイルは 0600 とし、元ファイルの権限をバックアップ側の権限に使わない。

### REQ-backup-012: 書き込み先を人が識別できる記録を置く
- kind: invariant
- source: docs/decision/records/2026-09-27-adopt-backup.md#A10
- verification: unit

集約先の書き込み先ごとの領域には、リモートならホスト名・ポート・root_dir、ローカルなら root_dir の絶対パスを書いた人が読めるファイルを置く。

### REQ-backup-013: 書き込み直前の内容を保存する
- kind: invariant
- source: docs/decision/records/2026-09-27-adopt-backup.md#A11
- verification: unit

通常ファイルのバックアップには、差分表示時に読んだ内容ではなく、上書きまたは削除の直前に書き込み先から読み直した内容を保存する。

### REQ-backup-014: 書き込む側だけをバックアップする
- kind: prohibition
- source: docs/decision/records/2026-09-27-adopt-backup.md#A12
- verification: unit

操作で上書きまたは削除する書き込み先のファイルだけをバックアップし、読み込むだけの側にはセッションを作らない。

### REQ-backup-015: 新規作成は記録しない
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A13
- verification: unit

書き込み先に元のファイルがないときは何も記録せず結果に backup を出さず、何も記録しなかった操作のセッションは集約先に残さず一覧にも出さない。

### REQ-backup-016: 旧バックアップディレクトリを扱わない
- kind: prohibition
- source: docs/decision/records/2026-09-27-adopt-backup.md#A30
- verification: unit

書き込み先にある既存の ".remote-merge-backup/" は読みも書きもせず、rollback の一覧にも status の結果にも出さない。

## Examples

```gherkin
@id=EX-backup-001 @about=REQ-backup-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A9
Scenario: リモートのファイルを上書きする
Given バックアップが有効で書き込み先の root_dir に既存ファイルがある
When そのファイルをマージする
Then 書き換え前の内容は利用者マシンの専用領域に保存され root_dir 配下にはバックアップが作られない

@id=EX-backup-002 @about=REQ-backup-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A9
Scenario: ローカルへファイルを上書きする
Given バックアップが有効でローカルの書き込み先に既存ファイルがある
When そのファイルへ同期する
Then 書き換え前の内容は利用者マシンの専用領域に保存され書き込み先の root_dir 配下には作られない

@id=EX-backup-003 @about=REQ-backup-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A10
Scenario: 一件のバックアップだけ失敗する
Given 二つの書き込み対象のうち一件だけバックアップに失敗する
When 両方をマージする
Then 失敗した対象は変わらず失敗が報告され、もう一件は処理される

@id=EX-backup-007 @about=REQ-backup-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A12
Scenario: XDG のデータ領域が絶対パスである
Given XDG_DATA_HOME が絶対パスに設定されている
When バックアップの保存先を決める
Then XDG_DATA_HOME の下の "remote-merge/backups/" が選ばれる

@id=EX-backup-008 @about=REQ-backup-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A12
Scenario: XDG のデータ領域が相対パスである
Given XDG_DATA_HOME が相対パスに設定されている
When バックアップの保存先を決める
Then ホームの ".local/share/remote-merge/backups/" が選ばれる

@id=EX-backup-009 @about=REQ-backup-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A13
Scenario: バックアップを無効化してマージする
Given バックアップを無効に設定している
When 既存ファイルをマージする
Then コピーを作らずに書き込み先が更新される

@id=EX-backup-010 @about=REQ-backup-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A13,docs/decision/records/2026-09-25-spec-migration.md#A9
Scenario: バックアップを有効にしたままマージする
Given バックアップが有効である
When 既存ファイルをマージする
Then 書き込み前の内容が保存される
```
