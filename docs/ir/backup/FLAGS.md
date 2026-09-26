# Flags

## Flags

### FLAG-backup-001: root_dir 末尾の区切りで書き込み先が分かれる
- kind: contradiction
- related: REQ-backup-021
- source: docs/decision/records/2026-09-27-adopt-backup.md#A34

旧個別仕様 3.8 は root_dir の末尾の "/" を除いて書き込み先を区別するとするが、実装は保存先のキーを root_dir の表示文字列から作るため "/srv/app/" と "/srv/app" が別の書き込み先になる。コード読解と PathBuf の最小再現で確認し、本体を通した実行は未確認。単体テスト trailing_slash_does_not_change_remote_target_identity は構造体の等価だけを見ており、保存先のキーを検査していない。

### FLAG-backup-002: 辿れないパスがあるときの他ファイルの扱い
- kind: ambiguity
- related: REQ-backup-003
- source: docs/decision/records/2026-09-27-adopt-backup.md#A35

旧個別仕様 3.12 は辿り直せなかったファイルを "cannot resolve path: <原因>" で failed にするとするが、実装は一件でも辿れないとセッション内の全ファイルを failed にする。他のファイルを戻すべきかは旧資料からも spec-migration の A11 からも決まらない。

### FLAG-backup-003: 機密ファイルのスキップと終了コード
- kind: ambiguity
- related: REQ-backup-036, REQ-backup-038
- source: docs/decision/records/2026-09-27-adopt-backup.md#A36

旧個別仕様 3.12 は機密ファイルのスキップの終了コードへの影響を「今までどおり（戻したファイルが 0 件なら 2）」とし、旧総合仕様と実装は一件でもスキップがあれば 2 とする。他のファイルを戻せたときに機密ファイルのスキップで 2 を返すべきかが定まらない。

### FLAG-backup-004: 確認を断ったときの終了コード
- kind: gap
- related: REQ-backup-035
- source: docs/decision/records/2026-09-27-adopt-backup.md#A37

rollback の確認に "y" または "yes" 以外で答えると "Aborted." を出して何も書き戻さず終了コード 0 で終わる。旧資料に記述がなく、テストもない。
