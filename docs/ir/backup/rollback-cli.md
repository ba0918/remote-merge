# rollback コマンド

rollback の対象の選び方、確認、一覧と結果の出力、終了コード。

## Requirements

### REQ-backup-034: 選んだセッションの内容に戻す
- kind: event_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A1
- verification: unit

rollback を実行すると、書き込み先の選んだセッションの各ファイルをバックアップした内容に戻し、--session を省略したときは期限切れでない最新のセッションを選ぶ。

### REQ-backup-035: 強制指定なしでは確認してから戻す
- kind: event_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A2, docs/decision/records/2026-10-04-merge-backup-flag-resolution.md#A7
- verification: unit

--force と --dry-run のどちらもないとき、rollback は "Restore N file(s) from session S to T? [y/N]" と尋ね、"y" または "yes" と答えたときだけ書き戻す。それ以外の答えでは標準エラーに "Aborted." を出し、何も書き戻さずに終了コード 0 で終わる。

### REQ-backup-037: 復元結果の出力形式
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-backup.md#A4
- verification: unit

rollback の結果はテキストでは戻したファイルを "✓"、スキップを "-" と理由、失敗を "✗" と理由で示し、"Restored N file(s)" にスキップ数と失敗数があればそれを続けた集計行を出す。JSON では target、session_id、restored（path と pre_rollback_backup）、skipped（path と reason）、failed（path と error）を出す。

### REQ-backup-038: rollback の終了コード
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-backup.md#A5
- verification: unit

rollback はスキップも失敗もなく一件以上戻したとき 0、スキップか失敗があるか戻したファイルがないとき 2、対象のセッションが見つからないとき 2 を返し、--dry-run と --list は 0 を返す。

### REQ-backup-039: 復元先の指定
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A6
- verification: unit

--list がないとき rollback は --target がなければエラーで止まり、--list で --target を省略したときはローカルを対象にする。

### REQ-backup-040: セッション一覧の出力形式
- kind: ubiquitous
- source: docs/decision/records/2026-09-27-adopt-backup.md#A7
- verification: unit

rollback --list はセッションを新しい順に並べ、テキストでは各ファイルを "path (N bytes)"、置き換えた symlink を "path -> link_target (symlink)"、期限切れのセッションに "[expired]" を付けて示す。JSON ではセッションごとに session_id、files（path と size、symlink は link_target で size なし）、file_count を出し、期限切れには "expired": true を付ける。

### REQ-backup-041: 記録が欠けたセッションは無いものとして扱う
- kind: state_driven
- source: docs/decision/records/2026-09-27-adopt-backup.md#A8
- verification: unit

集約先の中身が一部消されて記録が欠けたセッションは一覧に出さず一覧のコマンドはエラーにせず、そのセッションを指定した rollback は何も書き込まずセッションなしのエラーにする。

### REQ-backup-042: 利用者向け文書を挙動と一致させる
- kind: invariant
- source: docs/decision/records/2026-09-27-adopt-backup.md#A31
- verification: review
- how_to_verify: "skills/remote-merge/SKILL.md" と "skills/remote-merge/references/json-schemas.md" のバックアップと rollback の説明を読み、集約先の場所、backup の形式、rollback の一覧と結果の項目がこの文書と同じ directory の要件に一致することを人が確かめる。

利用者向けスキル文書のバックアップ構造・backup・rollback の説明は、集約先の場所とバックアップ・rollback の挙動に一致する。
