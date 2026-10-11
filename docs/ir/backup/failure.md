# バックアップできないときの扱い

バックアップの失敗の報告と、集約先の場所が決まらないときや集約先の準備に失敗したときの書き込みと rollback の扱い。

## Requirements

### REQ-backup-017: バックアップ失敗を原因付きで報告する
- kind: event_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A14, docs/decision/records/2026-10-11-agent-operations.md#A38, docs/decision/records/2026-10-11-agent-operations.md#A20, docs/decision/records/2026-10-11-agent-operations.md#A25
- verification: unit

merge・--delete・sync でファイルのバックアップに失敗したとき、そのファイルを結果の failed に出し、その error を "backup failed: " に続けて原因を示す文字列にする。
`接続断` のときは、そのファイルを含めて REQ-ssh-022・REQ-ssh-003・REQ-ssh-023 による。

### REQ-backup-018: 集約先が決まらないなら書き込みを始めない
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A16
- verification: unit

バックアップが有効で集約先の場所が決まらないとき、merge と sync は書き込む前にエラーで止まる。バックアップが無効なら、バックアップも期限切れの整理もせずに書き込む。

### REQ-backup-019: 集約先が決まらない rollback はエラーで終える
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A17
- verification: unit

集約先の場所が決まらないとき、rollback は --list と --dry-run を含むどのモードでも、バックアップの有効・無効に関係なくエラーで終わり、何も書き込まない。

### REQ-backup-045: 集約先の準備に失敗したら書き込みを始めない
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-backup.md#A4, docs/decision/records/2026-10-12-untested-items-consistency-2.md#A2
- verification: unit

バックアップが有効な merge・sync・rollback で、セッション ID の予約、集約先や書き込み先ごとの領域の作成、予約の読み取りが入出力エラーで失敗したときは、どの段階の失敗でも、どのファイルも書き込む前にコマンド全体をエラーで止める。書き換え前の内容を保存できないファイルの扱いは REQ-backup-002 による。ファイルごとに書き換え前の内容を保存するときに作る一時的な保存場所を作れないときも、そのファイルの書き換え前の内容を保存できないものとして REQ-backup-002 による。

## Examples

```gherkin
@id=EX-backup-026 @about=REQ-backup-045 @source=docs/decision/records/2026-10-05-quality-backup.md#A4
Scenario: 集約先に領域を作れない
Given バックアップが有効で集約先のディレクトリに書き込む権限がない
When 二つのファイルをマージする
Then どのファイルも書き込まれずにエラーで止まり、ファイルごとの "backup failed" にはならない
```
