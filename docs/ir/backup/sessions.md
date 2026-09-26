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

### REQ-backup-021: 書き込み先ごとにセッションを区別する
- kind: invariant
- source: docs/decision/records/2026-09-27-adopt-backup.md#A20
- verification: unit

バックアップセッションは書き込み先ごとに区別して保存し一覧する。リモートの書き込み先はホスト名・ポート・root_dir で決まりログインユーザー名を含めずホストの別名は別の書き込み先とし、ローカルは設定読み込み時のカレントディレクトリ基準の root_dir の絶対パスで symlink を辿らずに決まる。rollback --target は今の設定でその名前が指す書き込み先のセッションだけを扱う。

### REQ-backup-022: セッション ID の形式と順序
- kind: invariant
- source: docs/decision/records/2026-09-27-adopt-backup.md#A21, docs/decision/records/2026-09-27-backup-session-id-properties.md#A1
- verification: property

セッション ID は UTC の "YYYYMMDD-HHMMSS" で、同じ日時の ID が既にあるときだけ "-N"（N は 2 以上）を付ける。ID は日時の順、同じ日時なら N を数値として比べた順（"-N" なしが最初）に並べ、一覧の順と --session 省略時の最新をこの順で決め、--session は "-N" 付きの ID も受け付ける。

### REQ-backup-023: セッション ID は重複しない
- kind: invariant
- source: docs/decision/records/2026-09-27-adopt-backup.md#A22, docs/decision/records/2026-09-27-backup-session-id-properties.md#A2
- verification: property

セッション ID は集約先全体で重複せず、別々の操作が同じ秒に始まっても同時に走っても同じ ID にならない。

### REQ-backup-024: 一回の sync は一つの ID を共有する
- kind: invariant
- source: docs/decision/records/2026-09-27-adopt-backup.md#A23
- verification: unit

一回の sync が複数の書き込み先に書くとき、すべての書き込み先のバックアップに同じセッション ID を使う。

### REQ-backup-025: 期限切れを整理する時点
- kind: event_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A28
- verification: unit

期限切れバックアップの整理は TUI の起動時と dry-run でない merge・sync の開始時にだけ行い、rollback はどのオプションでも整理しない。バックアップが無効でも整理し、集約先の場所が決まらないときは整理しない。

### REQ-backup-026: 期限切れの境界
- kind: invariant
- source: docs/decision/records/2026-09-27-adopt-backup.md#A29
- verification: unit

セッション ID の日時部分（UTC）からの経過時間が retention_days × 24 時間以上のセッションを期限切れとし、一覧の期限切れ表示と整理の対象に同じ判定を使う。

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
