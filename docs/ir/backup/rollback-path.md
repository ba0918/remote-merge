# 復元前のリンク先確認

rollback の書き戻し方と、バックアップ時と異なる場所への書き戻しを防ぐ規則。

## Requirements

### REQ-backup-003: リンク先が変わったセッションは書き戻さない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A11, docs/decision/records/2026-10-04-merge-backup-flag-resolution.md#A8
- verification: unit

rollback はセッション内の書き戻し対象すべてのリンク先を事前に確認し、一件でもバックアップ時と別の場所を指すなら、そのセッションのファイルは一件も書き戻さず理由を報告する。書き戻し対象のどれか一件でもリンク先を辿れないときも、そのセッションのファイルは一件も書き戻さず、すべてを failed に error "cannot resolve path: " に続く原因で報告する。

### REQ-backup-008: 復元プレビューでは書き換えない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A32
- verification: unit

rollback の dry-run はセッション内の復元予定とブロック理由を報告し、書き込み先を変更しない。

### REQ-backup-010: 変更した末尾 symlink を元に戻す
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A46, docs/decision/records/2026-09-25-spec-migration.md#A11, docs/decision/records/2026-09-25-spec-migration.md#A54
- verification: unit

末尾 symlink 同士のマージ後、リンク文字列がマージ直後と同じなら、rollback は保存されたマージ前のリンク文字列へ戻す。第三者がリンク文字列を変えていればセッション全体を書き戻さない。

### REQ-backup-027: 書き戻す前の内容を退避する
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A15
- verification: unit

バックアップが有効なとき、rollback は書き戻す前の現在の内容を新しいセッションに保存してその ID を pre_rollback_backup に示す。保存できないファイルは書き戻さず error "backup failed: " に続く原因で失敗とし、戻す先にファイルがなければ保存も pre_rollback_backup の表示もしない。

### REQ-backup-028: 無効時は退避せずに書き戻す
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A19
- verification: unit

バックアップが無効なとき、rollback は現在の内容を保存せずに書き戻す。

### REQ-backup-029: 書き戻しで権限を変えない
- kind: invariant
- source: docs/decision/records/2026-09-27-adopt-backup.md#A24
- verification: unit

rollback が既存ファイルに書き戻すとき、そのファイルの所有者と権限を維持する。

### REQ-backup-030: 削除したファイルを作り直す
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A47
- verification: unit

--delete で削除したファイルのバックアップを rollback すると、そのファイルを保存した内容で再作成する。

### REQ-backup-031: 消えた親ディレクトリは作らない
- kind: prohibition
- source: docs/decision/records/2026-09-27-adopt-backup.md#A25
- verification: unit

戻す先にファイルがなく親ディレクトリもないとき、rollback はそのファイルを書き戻さずスキップ理由 "parent directory no longer exists" を報告し、ディレクトリを作らない。

### REQ-backup-032: 置き換えた symlink は復元しない
- kind: prohibition
- source: docs/decision/records/2026-09-27-adopt-backup.md#A26
- verification: unit

同じ種類の symlink 同士の更新以外で symlink を通常ファイルに置き換えたか削除した記録は、--force の有無に関係なく書き戻さず、スキップ理由 "symlink restore not supported" を報告する。

### REQ-backup-033: 場所の変化をスキップ理由で示す
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A27
- verification: unit

rollback は本当のパスが記録と違うとき（リンク切れの symlink、通常ファイルから symlink への変化、削除済みファイルの親の付け替えを含む）スキップ理由 "path now resolves to a different location" を報告し、末尾 symlink の更新記録が第三者に変えられたときは "symlink changed after merge" を報告する。

## Examples

```gherkin
@id=EX-backup-005 @about=REQ-backup-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A11
Scenario: 一件の親 symlink が付け替えられた
Given 二つのファイルを変更したセッションのうち一件の親 symlink が別の場所に付け替えられた
When そのセッションを復元する
Then どちらのファイルも書き戻されずリンク先変更の理由が報告される

@id=EX-backup-006 @about=REQ-backup-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A11,docs/decision/records/2026-09-25-spec-migration.md#A9
Scenario: リンク先が変わっていない
Given セッション内のどのファイルもバックアップ時と同じ場所を指す
When そのセッションを復元する
Then リンク先の変更を理由にセッション全体が拒否されることはない

@id=EX-backup-015 @about=REQ-backup-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A32
Scenario: 復元の予定を先に確認する
Given バックアップセッションに複数のファイルがある
When rollback の dry-run を実行する
Then 復元予定が報告されどのファイルも変わらない

@id=EX-backup-016 @about=REQ-backup-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A32
Scenario: リンク先が変わったことをプレビューで知る
Given バックアップ後に一件の親 symlink が別の場所を指す
When rollback の dry-run を実行する
Then ブロック理由が報告されどのファイルも変わらない

@id=EX-backup-020 @about=REQ-backup-010 @source=docs/decision/records/2026-09-25-spec-migration.md#A46
Scenario: マージでリンク先を変更した後に復元する
Given 末尾 symlink のリンク文字列をマージし、その後は変わっていない
When そのセッションを復元する
Then リンク文字列がマージ前の文字列に戻る

@id=EX-backup-021 @about=REQ-backup-010 @source=docs/decision/records/2026-09-25-spec-migration.md#A46,docs/decision/records/2026-09-25-spec-migration.md#A54
Scenario: マージ後に第三者がリンクを付け替えた
Given 同じセッションで二件を変更し、末尾 symlink はマージ後に別のリンク先へ付け替えられた
When そのセッションを復元する
Then 二件とも書き戻されず第三者が付け替えたリンクも変わらない

@id=EX-backup-022 @about=REQ-backup-010 @source=docs/decision/records/2026-09-25-spec-migration.md#A54
Scenario: 同じ実パスを指す別表記への変更
Given マージ後に第三者が末尾 symlink の文字列を変えたが到達先は同じである
When そのセッションを復元する
Then セッションのどのファイルも書き戻されない
```
