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

## 要件ごとの根拠テスト

根拠テストは `tests/contract/` の下に IR の文書ごとのモジュールとして置いた。
一つの要件に複数の経路や場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
「元のテスト」は移したテストの移し元で、移したものは移し元から消した。
移すときに要件の文の一部しか確かめていなかったものは、足りない確認を書き足した。

### 保存（docs/ir/backup/storage.md）

根拠テストは全て `tests/contract/backup_storage.rs` にある。

| 要件 | 根拠テスト | 元のテストと変えたところ |
|---|---|---|
| REQ-backup-011 | aggregate_store_entries_are_owner_only_regardless_of_the_original_permissions | tests/local_backup_store.rs の aggregate_store_entries_are_owner_only_and_describe_target。元のファイルの権限を 0664・0644 に明示し、--delete で保存したものも含めて全ての項目を確かめる |
| REQ-backup-012 | each_target_area_has_a_readable_description_of_the_write_target | 同じテストの説明ファイルの確認を分けた。元は root_dir を含むことだけを見ていたため、ホスト名とポート（既定値でない 2222）と、ローカルの書き込み先の絶対パスを確かめる |
| REQ-backup-013 | overwrite_backs_up_the_destination_content_read_immediately_before_writing、deletion_backs_up_the_destination_content_read_immediately_before_removing | tests/local_backup_store.rs の rollback_restores_the_content_seen_immediately_before_merge_writes。要件の「削除の直前」を確かめる削除の場合を書き足した |
| REQ-backup-014 | only_the_overwritten_file_on_the_written_side_is_backed_up | tests/local_backup_store.rs の read_only_side_has_no_session_after_one_way_merge。書き込んだ側のセッションに上書きしたファイルだけが入ることを書き足した |
| REQ-backup-015 | creating_a_new_file_records_nothing_and_leaves_no_session、creating_a_new_file_in_a_missing_directory_records_nothing_and_leaves_no_session | tests/local_backup_store.rs の new_file_merge_records_no_backup と new_file_in_a_missing_directory_records_no_backup。集約先に何も残らないことと一覧に出ないことを両方に書き足した |
| REQ-backup-016 | legacy_backup_directory_is_left_alone_and_hidden_from_list_and_status | tests/local_backup_store.rs の legacy_backup_directory_is_ignored_by_list_and_status。merge の後も既存の ".remote-merge-backup/" の中身が変わらないことと、status の全件の結果に出ないことを書き足した |

REQ-backup-012 のテストは、`RemoteTargetIdentity::port` を 1 を返すように書き換えると失敗することを、コミットに含めない一時的な書き換えで確かめた。
