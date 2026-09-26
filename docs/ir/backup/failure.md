# バックアップできないときの扱い

バックアップの失敗の報告と、集約先の場所が決まらないときの書き込みと rollback の扱い。

## Requirements

### REQ-backup-017: バックアップ失敗を原因付きで報告する
- kind: event_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A14
- verification: unit

merge・--delete・sync でファイルのバックアップに失敗したとき、そのファイルを結果の failed に出し、その error を "backup failed: " に続けて原因を示す文字列にする。

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

### REQ-backup-020: TUI はバックアップできない書き込みを断る
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A18
- verification: unit

TUI でバックアップが有効なのに書き込み前のバックアップが取れないとき、ステータス行に "Backup failed: " に続けて原因を示し、書き込まない。w による書き込みは両側をバックアップし、どちらかが取れなければ両側とも書き込まない。集約先の場所が決まらなくても TUI は起動して差分を表示する。
