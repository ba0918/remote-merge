# 同期時の削除

書き込み先にしかないファイルを削除対象に含める条件。

## Requirements

### REQ-merge-008: 削除は明示指定に限る
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A18
- verification: unit

merge と sync は書き込み先だけにある通常ファイルを既定では残し、--delete を明示したときだけ削除する。

### REQ-merge-016: 削除前にバックアップする
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A47, docs/decision/records/2026-09-25-spec-migration.md#A9
- verification: unit

バックアップが有効な場合、--delete による通常ファイルの削除前に内容を利用者マシンの集約先へ保存する。保存に失敗した対象は削除せず、保存した対象は rollback で再作成できる。

### REQ-merge-024: 削除しないときは書き込み先だけのファイルをスキップとして出す
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A2
- verification: unit

--delete のない merge と sync は、書き込み先にだけあるファイルを変更せず、skipped に reason "right-only file (use --delete to remove)" で出す。

### REQ-merge-026: 削除の結果を JSON に出す
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A4
- verification: unit

merge は削除したファイルを deleted に path、status "ok"、バックアップが有効なときだけ backup（"セッションID/パス"）で出す。

### REQ-merge-027: 削除をテキストに出す
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A5
- verification: unit

merge のテキスト出力は削除したファイルを "Deleted: パス" の行で出し、バックアップがあれば続けて " (backup: バックアップ)" を出す。

### REQ-merge-034: 削除できなかった対象は失敗として出す
- kind: event_driven
- source: docs/decision/records/2026-10-04-merge-backup-flag-resolution.md#A10
- verification: unit

--delete の merge と sync は、削除の直前に書き込み先の種類を調べられなかったファイルと、削除に失敗したファイルを failed に出し、他のファイルの処理は続ける。

## Examples

```gherkin
@id=EX-merge-015 @about=REQ-merge-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A18
Scenario: 書き込み先だけにある通常ファイルを残す
Given 書き込み先だけに通常ファイルがある
When --delete を付けずに同期する
Then その通常ファイルは残る

@id=EX-merge-016 @about=REQ-merge-008 @source=docs/decision/records/2026-09-25-spec-migration.md#A18
Scenario: 明示的に余分な通常ファイルを消す
Given 書き込み先だけに通常ファイルがある
When --delete を付けて同期する
Then その通常ファイルは削除される

@id=EX-merge-032 @about=REQ-merge-016 @source=docs/decision/records/2026-09-25-spec-migration.md#A47
Scenario: 削除したファイルを復元する
Given バックアップが有効で書き込み先だけに通常ファイルがある
When --delete で削除してからそのセッションを復元する
Then 削除前の内容でファイルが再作成される

@id=EX-merge-033 @about=REQ-merge-016 @source=docs/decision/records/2026-09-25-spec-migration.md#A47
Scenario: 削除前の保存が失敗する
Given バックアップが有効で書き込み先のファイルを集約先へ保存できない
When --delete で同期する
Then ファイルは削除されず失敗が報告される
```
