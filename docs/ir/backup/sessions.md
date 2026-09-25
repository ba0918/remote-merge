# バックアップセッションと期限

復元する一回の操作を一覧から選ぶ際の単位と期限切れの扱い。

## Requirements

### REQ-backup-006: 一操作を一つのセッションにまとめる
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A27
- verification: unit

一回の書き込み操作で変更した複数ファイルのバックアップは一つのセッションとして識別し、rollback --list でそのセッションを一覧に出す。

### REQ-backup-007: 期限切れの復元には明示を求める
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A27, docs/decision/records/2026-09-25-spec-migration.md#A48
- verification: unit

期限切れのバックアップセッションは通常の復元候補から外し、保存データが残っていて利用者が明示的な強制指定をしたときだけ復元できる。

### REQ-backup-009: 無関係な書き込み先の履歴を掃除しない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A37
- verification: unit

期限切れバックアップの自動整理は現在設定された書き込み先の履歴だけを対象とし、設定にない書き込み先の履歴を消さない。

## Examples

```gherkin
@id=EX-backup-011 @about=REQ-backup-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A27
Scenario: 複数ファイルを同期する
Given 書き込み先に変更されるファイルが二つある
When 一回の同期操作で両方を更新する
Then 両方のバックアップは同じセッションとして一覧に現れる

@id=EX-backup-012 @about=REQ-backup-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A27
Scenario: 別々の操作を実行する
Given 書き込み先に変更されるファイルがある
When 二回の独立したマージを実行する
Then 操作ごとに区別できるセッションが一覧に現れる

@id=EX-backup-013 @about=REQ-backup-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A27
Scenario: 期限切れを通常復元する
Given バックアップセッションが期限切れである
When 強制指定なしでそのセッションを復元する
Then ファイルは変更されない

@id=EX-backup-014 @about=REQ-backup-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A48
Scenario: 期限切れを明示的に復元する
Given バックアップセッションが期限切れで保存データは残っている
When 強制指定してそのセッションを復元する
Then 保存された内容が書き戻される

@id=EX-backup-019 @about=REQ-backup-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A48
Scenario: 整理済みの期限切れ履歴を指定する
Given 期限切れのセッションの保存データは自動整理で削除された
When 強制指定してそのセッションを復元する
Then ファイルは変更されない

@id=EX-backup-017 @about=REQ-backup-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A37
Scenario: 現在の設定から外れたサーバの履歴
Given 集約先に現在は設定されていないサーバの期限切れ履歴がある
When 期限切れバックアップを自動整理する
Then そのサーバの履歴は残る

@id=EX-backup-018 @about=REQ-backup-009 @source=docs/decision/records/2026-09-25-spec-migration.md#A37
Scenario: 現在の設定にあるサーバの履歴
Given 集約先に現在設定されているサーバの期限切れ履歴がある
When 期限切れバックアップを自動整理する
Then そのサーバの期限切れ履歴が整理対象になる
```
