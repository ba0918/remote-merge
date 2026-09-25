# 読み取り中の SSH 切断

SSH 接続が途中で切れた場合に、読み取りを再開する範囲と書き込みの扱い。

## Requirements

### REQ-ssh-003: 読み取りだけ一度再試行する
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A26
- verification: unit

SSH の読み取り中に接続が切れたとき、再接続してその読み取りを一度だけ試し、再び失敗したら失敗を報告する。

### REQ-ssh-004: 結果不明の書き込みを自動反復しない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A26
- verification: unit

SSH の書き込み中に接続が切れて更新結果が不明なとき、同じ書き込みを自動で繰り返さない。

## Examples

```gherkin
@id=EX-ssh-005 @about=REQ-ssh-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A26
Scenario: 一度の読み取り断から回復する
Given SSH の読み取り中に一度だけ接続が切れる
When 同じファイルを読み直す
Then 再接続後の読み取り結果が返る

@id=EX-ssh-006 @about=REQ-ssh-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A26
Scenario: 再接続後も読み取れない
Given SSH の読み取りが再接続後にも失敗する
When ファイルを読み取る
Then 二回目の失敗が報告され自動再試行が続かない

@id=EX-ssh-007 @about=REQ-ssh-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A26
Scenario: 書き込み結果が不明である
Given SSH の書き込み中に接続が切れる
When マージ結果を判定する
Then 同じ更新は自動再実行されず失敗が報告される

@id=EX-ssh-008 @about=REQ-ssh-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A26
Scenario: 読み取り中だけの切断
Given SSH の読み取り中に接続が切れる
When ファイルを読み直す
Then 読み取りだけが再試行される
```
