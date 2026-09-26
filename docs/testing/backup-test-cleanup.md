# バックアップと rollback のテスト整理の記録

バックアップと rollback の要件（REQ-backup-011 から REQ-backup-041）の根拠テストを整理した過程の記録。
変異テストの結果、各要件の根拠にしたテスト、利用者の判断を待つ削除候補を残す。

## 整理前の変異テスト

整理を始める前のコミット 6e9cfe7 で、次のコマンドを一度だけ実行した。
三つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest）で整理後と比べられるようにした。

```sh
scripts/mutants.sh src/backup/mod.rs src/service/rollback.rs src/runtime/backup_store.rs
```

全体の集計は `mutants: caught=96 survived=16 timeout=0 unviable=14 equivalent=0`（実行時間は約 32 分）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/backup/mod.rs | 25 | 7 | 0 | 10 |
| src/service/rollback.rs | 42 | 0 | 0 | 2 |
| src/runtime/backup_store.rs | 29 | 9 | 0 | 2 |

### 見逃し

位置は変異が入る行、変異は kotowari mutants の出力の文言のまま。

| 位置 | 変異 |
|---|---|
| src/backup/mod.rs:45 | replace RemoteTargetIdentity::host -> &str with "" |
| src/backup/mod.rs:45 | replace RemoteTargetIdentity::host -> &str with "xyzzy" |
| src/backup/mod.rs:49 | replace RemoteTargetIdentity::port -> u16 with 0 |
| src/backup/mod.rs:49 | replace RemoteTargetIdentity::port -> u16 with 1 |
| src/backup/mod.rs:79 | replace backup_timestamp -> String with "xyzzy".into() |
| src/backup/mod.rs:79 | replace backup_timestamp -> String with String::new() |
| src/backup/mod.rs:112 | replace > with < in parse_session_id |
| src/runtime/backup_store.rs:117 | replace == with != in BackupStore::reserve_session |
| src/runtime/backup_store.rs:117 | replace match guard error.kind() == std::io::ErrorKind::AlreadyExists with true in BackupStore::reserve_session |
| src/runtime/backup_store.rs:242 | replace += with *= in BackupStore::cleanup_expired |
| src/runtime/backup_store.rs:310 | replace && with \|\| in BackupStore::list_sessions |
| src/runtime/backup_store.rs:347 | replace match guard path == rel_path with true in BackupStore::read_record |
| src/runtime/backup_store.rs:359 | replace match guard path == rel_path with true in BackupStore::read_record |
| src/runtime/backup_store.rs:383 | replace == with != in create_temporary_entry |
| src/runtime/backup_store.rs:383 | replace match guard error.kind() == std::io::ErrorKind::AlreadyExists with false in create_temporary_entry |
| src/runtime/backup_store.rs:383 | replace match guard error.kind() == std::io::ErrorKind::AlreadyExists with true in create_temporary_entry |

`src/backup/mod.rs:112` の `replace > with < in parse_session_id` は、この実行では見逃しだったが、スクリプトの確認のために直前に `src/backup/mod.rs` だけを対象にした実行（caught=26 survived=6 unviable=10）では検知されていた。
二つの実行のテストの集合は同じ（どちらも 2,846 件）で、検知の有無が実行ごとに変わる理由は確かめていない。
整理後の比較では、この変異を見逃しの基準に含める。
