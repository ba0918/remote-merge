# Flags

## Flags

### FLAG-backup-001: root_dir 末尾の区切りで書き込み先が分かれる
- kind: contradiction
- related: REQ-backup-021
- source: docs/decision/records/2026-09-27-adopt-backup.md#A34

旧個別仕様 3.8 は root_dir の末尾の "/" を除いて書き込み先を区別するとするが、実装は保存先のキーを root_dir の表示文字列から作るため "/srv/app/" と "/srv/app" が別の書き込み先になる。コード読解と PathBuf の最小再現で確認し、本体を通した実行は未確認。単体テスト trailing_slash_does_not_change_remote_target_identity は構造体の等価だけを見ており、保存先のキーを検査していない。

### FLAG-backup-005: セッション ID の連番のあふれで panic する
- kind: gap
- related: REQ-backup-022, REQ-backup-023
- source: docs/decision/records/2026-09-27-backup-mutant-flags.md#A1

既存のセッション ID の N が u64 の最大値のとき、次の ID を作る処理が加算のあふれで panic する。集約先の予約ディレクトリを手で作らない限り起きないが、そのときの扱いは IR に定めがない。

### FLAG-backup-006: 予約や一時保存の入出力エラーの扱い
- kind: gap
- related: REQ-backup-002, REQ-backup-017, REQ-backup-023
- source: docs/decision/records/2026-09-27-backup-mutant-flags.md#A2

セッション ID の予約や一時的な保存場所の作成が「既に存在する」以外の入出力エラーで失敗したときの扱いが IR にない。今の実装はコマンド全体をエラーで止める。変異テストの見逃し 3 件はこのエラーを起こすテストがなければ落とせない。

### FLAG-backup-007: 書き換えられた集約先の扱い
- kind: gap
- related: REQ-backup-041
- source: docs/decision/records/2026-09-27-backup-mutant-flags.md#A3

集約先の中に製品が作らない名前や中身があるときの扱いが IR になく、定めがあるのは中身が消されたときだけである。変異テストの見逃し 4 件はこの状況でしか違いを生まない。
