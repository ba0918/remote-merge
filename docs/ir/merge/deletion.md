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
