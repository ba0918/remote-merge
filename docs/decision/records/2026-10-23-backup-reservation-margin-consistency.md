# セッション ID の予約を残す 24 時間の幅と、予約した時刻の前提

## Context

期限切れの整理でセッション ID の予約を消すとき、日時から 24 時間に満たない ID の予約は残す（[REQ-backup-047](../../ir/backup/sessions.md#REQ-backup-047)）。この 24 時間は、ID の日時がコマンドを始めた時刻だったとき（9fd90a5b）に決めた。
その後、ID の日時を予約した時刻にした（[backup-session-id-reservation-time A1](./2026-10-23-backup-session-id-reservation-time.md#A1)）。それでも、REQ-backup-047・CHANGELOG.md・整理のコードのコメントが述べる残す理由と、24 時間より狭い案を退けた記録の根拠は、始めた時刻の前提のままだった。
この記録は、「予約を消してよいのは、これから予約する操作が受け取りうるどの ID よりも前の秒の ID だけ」という前提で、それらを見直した結果を残す。

Position: 24 時間の但し書きとコードは変えず、残す理由の文を直した（A1）。前の二つの記録の根拠は、それぞれの Revisions で読み替えた。

## Agreements

- A1 [REQ-backup-047](../../ir/backup/sessions.md#REQ-backup-047) の、日時から 24 時間に満たない ID の予約を残す但し書きと、それを実装したコード（crates/remote-merge-engine/src/backup_store_cleanup.rs の cleanup_expired）は変えない。残す理由は「整理するコマンドを始めた秒の ID は、その秒のうちにこれから予約する別の操作が受け取りうるので、その秒を含めて 24 時間を幅として残す」と書き直す（REQ-backup-047、CHANGELOG.md、cleanup_expired のコメント）。
  - why: ID の日時は予約するときのシステムの時刻（crates/remote-merge-engine/src/backup_store.rs の reserve_session の reserved_at。本番は crates/remote-merge/src/runtime/mod.rs の RuntimeTargets::production が渡す ReservationTime::System）で、整理はその予約より前に、コマンドを始めた時刻 now で期限切れを判定する（crates/remote-merge/src/cli/plan_apply.rs の start_backup_session）。整理が予約を消した後で予約する操作は、整理するコマンドを始めた秒以後の ID を受け取る。そのため [REQ-backup-023](../../ir/backup/sessions.md#REQ-backup-023) を保つのに残す必要があるのは、その秒の ID の予約だけである（保持期間 0 日のときに期限切れになる）。下の Verification の測定では、整理するコマンドを始めた秒より前の ID の予約を消す狭い形でも、予約した時刻の ID なら測定 2 の食い違いは起きなかった。REQ-backup-023 と REQ-backup-047 に印を付けた契約テストも成功した。[backup-reservations-cleanup-consistency A2](./2026-10-23-backup-reservations-cleanup-consistency.md#A2) がこの狭い案を退けた根拠（同じ記録の測定 2）は、ID の日時がコマンドを始めた時刻のときにだけ再現する。それでも 24 時間を変えないのは、IR とコードが一致して REQ-backup-023 を満たしていて、IR の部分どうしの矛盾も、24 時間が実現できないことも示されていないからである。狭くして変わるのは、保持期間 0 日のときに整理の後も集約先に残る予約の数（[backup-reservations-cleanup-consistency A3](./2026-10-23-backup-reservations-cleanup-consistency.md#A3)）で、これは利用者が見る振る舞いを変える設計の選択になる。保持期間が 1 日以上なら、期限切れのセッションはどれも 24 時間以上前の ID なので、二つの形に違いはない。これまでの理由の文は、その秒の ID を守る理由で 24 時間の全体を説明していて、24 時間に満たないどの ID にもその秒のうちに予約する操作がいるように読めた。守るのはその秒の ID で、24 時間はそれを含む幅だと分かるように直した。
  - rejected: 但し書きを、整理するコマンドを始めた秒の ID だけを残す形に狭める案。測定では REQ-backup-023 を保つが、IR が誤っていると示されていない但し書きを変え、保持期間 0 日のときに集約先に残るものを変えるので、この突き合わせでは選ばない。残す理由の文をそのままにする案は、ID の日時を予約した時刻にした後の製品の振る舞いと合わない。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- 測定はリポジトリに入れない使い捨ての結合テストで行った。crates/remote-merge-engine/tests/ に置いたテストから BackupStore を直接呼び、`cargo nextest run -p remote-merge-engine --all-features -E 'test(/zz_probe/)' --no-capture` で ID を出した。流れは同じ記録の測定 2 を実際の時刻で起こしたものである。保持期間 0 日で、A が予約して保存して終える。次の秒まで待ち、B がそのときの時刻で整理して予約する。その後で、A と同じ時刻に始まった C が整理して予約する。
- 今のコード（d34b13b0）では A=20261023-044308、B=20261023-044309、C=20261023-044309-2 で、A の予約は B の整理の後も残っていた。
- 整理だけを一時的に狭い形（整理するコマンドを始めた秒より前の ID の予約を消す）に変えると、A=20261023-044317、B=20261023-044318、C=20261023-044318-2 で、A の予約は消えていた。狭い形のまま C の予約する時刻だけを C を始めた時刻に決めると（ID の日時をコマンドを始めた時刻とした場合）、A=20261023-044318、B=20261023-044319、C=20261023-044318 で、C は A と同じ ID を受け取った。
- 狭い形のまま `cargo nextest run -p remote-merge --all-features -E 'test(/session_ids|removed_session|cleanup_leaves|reservation/)'` を動かすと 7 件すべて成功した（merges_started_in_the_same_second_use_distinct_session_ids_with_zero_retention、a_rollback_confirmed_days_after_it_started_gets_an_id_unlike_a_removed_session、cleanup_leaves_nothing_of_a_removed_session_in_the_store を含む）。測定の後で整理を元に戻した。
- 同じ前提で、ID の日時をコマンドを始めた時刻とみなしているものがほかにないかを読んだ。読んだのは、rollback --list の並びと期限切れの表示（crates/remote-merge-engine/src/backup_store.rs の list_sessions、crates/remote-merge-engine/src/treat/rollback_session.rs の with_expiry、crates/remote-merge/src/service/output.rs の format_backup_list_text）、--session と省略時の最新の選び方（rollback_session.rs の choose_session）、一回の sync が一つの ID を共有すること（crates/remote-merge/src/cli/sync_targets.rs の start_backup_session は確認の後で一度だけ）、rollback が書き戻す前の内容を退避するセッション（crates/remote-merge/src/runtime/rollback_apply.rs の restore_paths は確認の後で予約する）、何も保存しなかったセッションの予約を消す finish_session である。finish_session が消すのは、どの出力にも集約先にも ID が出ていないセッションの予約で、ID の日時には依らない。文書は、README.md、skills/remote-merge/SKILL.md、skills/remote-merge/references/json-schemas.md、crates/remote-merge/src/main.rs の rollback のヘルプ、docs/ir を、`rg -n -i "same second|command start|同じ秒|始めた時刻|開始"` とセッションの語で検索した。ID の日時を始めた時刻とする記述は見つからなかった。
