# 復元前のリンク先確認

複数ファイルの復元前に、バックアップ時と異なる場所への書き戻しを防ぐ規則。

## Requirements

### REQ-backup-003: リンク先が変わったセッションは書き戻さない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A11
- verification: unit

rollback はセッション内の書き戻し対象すべてのリンク先を事前に確認し、一件でもバックアップ時と別の場所を指すなら、そのセッションのファイルは一件も書き戻さず理由を報告する。

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
