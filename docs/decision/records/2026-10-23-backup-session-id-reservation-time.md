# セッション ID の日時を予約する時刻にすること

## Context

期限切れの整理でセッション ID の予約を消すようにした後（[backup-reservations-cleanup-consistency A1](./2026-10-23-backup-reservations-cleanup-consistency.md#A1)・[A2](./2026-10-23-backup-reservations-cleanup-consistency.md#A2)）も、ID の日時はコマンドを始めた時刻で、予約は書き込みの段で行っていた。
sync と rollback は確認の問いに答えるまで待つので、待つ間に同じ秒に始まった先の操作のセッションが予約ごと整理で消えると、後の操作が先の操作と同じ ID を受け取っていた（下の Verification の測定 1）。
これは [REQ-backup-023](../../ir/backup/sessions.md#REQ-backup-023) の「別々の操作が同じ秒に始まっても同じ ID にならない」に反する。この記録は、この食い違いをどう直したかを残す。

Position: ID の日時を、セッション ID を予約した時刻にして直した（A1）。整理で予約を残す 24 時間の但し書きは変えず、理由の書き方だけを改めた（A2）。

## Agreements

- A1 セッション ID の日時は、コマンドを始めた時刻ではなく、操作が書き込みの前にセッション ID を予約した時刻（UTC）とする。sync と rollback では確認の問いに答えた後の時刻になる。期限切れの判定と整理には、これまでどおりコマンドを始めた時刻を使う。これを [REQ-backup-022](../../ir/backup/sessions.md#REQ-backup-022) に書き、コードを直す（b7c04aa8）。
  - why: 9fd90a5b までのコードは REQ-backup-023 の後半に反していた。実行の時刻は始めたときに一度だけ決まり（crates/remote-merge/src/runtime/mod.rs の RuntimeTargets::production の now）、sync は "Proceed? [y/N]"（crates/remote-merge/src/cli/sync.rs の confirmed）、rollback は復元の確認（crates/remote-merge/src/cli/rollback.rs の confirm_prompt）に答えるまで待ってから予約する（crates/remote-merge/src/cli/plan_apply.rs の start_backup_session、crates/remote-merge/src/runtime/rollback_apply.rs の restore_paths）。待つ間に別の merge・sync の整理が同じ秒の ID の予約を消すと、後の操作がその ID を受け取る（測定 1）。REQ-backup-023 の出典（[adopt-backup A22](./2026-09-27-adopt-backup.md#A22)・[backup-session-id-properties A2](./2026-09-27-backup-session-id-properties.md#A2)）にも、IR の部分どうしにも、IR を改める理由はないので、コードを IR に合わせた。ID の日時がどの時点の時刻かは、IR（docs/ir）も README.md も [adopt-backup A21](./2026-09-27-adopt-backup.md#A21) も決めていない（測定 2）ので、予約した時刻にすることは IR と矛盾しない具体化である。配布する skills/remote-merge/SKILL.md は "-N" を同じ秒に操作が始まったときに付くと書き、ID の日時を始めた秒とみなしていたが、スキル文書は IR ではなく製品の一部なので、IR に合わせて直した（[backup-session-id-skill-text A1](./2026-10-23-backup-session-id-skill-text.md#A1)）。予約した時刻にすると、整理が予約を消す ID の日時は、整理するコマンドを始めた時刻より 24 時間以上前で、その秒はもう過ぎている。後から予約する実行はそれより後の時刻の ID を受け取るので、消した予約の ID を受け取ることはない。これで REQ-backup-023 と、利用者が受け入れた予約を消す修正（[REQ-backup-047](../../ir/backup/sessions.md#REQ-backup-047)）が両方成り立つ。利用者に見える違いは、ID の日時が、比べる間と、sync と rollback では確認に答えるまでの間だけ後になることである。
  - rejected: コマンドを始めた時刻の ID のままにし、予約してから 24 時間以上たった操作が整理で消えたセッションの ID を受け取りうることを REQ-backup-047 に書く案は、承認された REQ-backup-023 を、IR が誤っていると示されていないのに狭める（予約した時刻の ID で REQ-backup-023 は実現できる）。予約するときにコマンドを始めた時刻が 24 時間以上前なら止める案は、確認に遅れて答えた sync と rollback に新しい失敗を足す。予約を消さない案は、利用者が受け入れた修正を取り消す。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A2 [REQ-backup-047](../../ir/backup/sessions.md#REQ-backup-047) の、日時から 24 時間に満たない ID の予約を残す但し書きは変えない。残す理由は「その秒のうちにこれから予約する別の操作にその ID を割り当てないため」と書き直す。
  - why: A1 の後に整理が予約を消した ID を受け取りうるのは、その ID と同じ秒に予約する操作だけで、同じ秒に始まった操作ではない。保持期間が 0 日なら、整理するコマンドを始めた秒と同じ秒の ID も期限切れになるので、但し書きは今も要る（契約テスト merges_started_in_the_same_second_use_distinct_session_ids_with_zero_retention）。24 時間の幅は [backup-reservations-cleanup-consistency A2](./2026-10-23-backup-reservations-cleanup-consistency.md#A2) のとおり保持期間の最小の正の値にそろえたもので、A1 の後も変える理由はない。
  - decided_by: consistency phase（実装と仕様の整合確認役）
  - superseded_by: [backup-reservation-margin-consistency A1](./2026-10-23-backup-reservation-margin-consistency.md#A1)

## Verification

- 測定 1: リポジトリに入れない使い捨ての契約テストで、merge A を 2020-01-01 00:00:00 に（保持期間は既定の 7 日）、merge C を 2020-01-08 00:00:00 に同じ集約先で行い（C の整理が A のセッションと予約を消す）、その後で 2020-01-01 00:00:00 に始まった rollback（確認に遅れて答えたものの代わりに --force を付けた）で C のセッションを戻した。`cargo nextest run -p remote-merge --all-features -E 'test(/zz_probe_late_reserving/)' --no-capture` で、f7c07aa3 では rollback の pre_rollback_backup が A と同じ "20200101-000000" になった。整理だけを b8dde42a に戻すと "20200101-000000-2" だった。
- 測定 2: docs/ir・README.md・skills/remote-merge をセッション ID で検索した。docs/ir と README.md には、ID の日時がどの時点の時刻かを決めた文はなかった。skills/remote-merge/SKILL.md は "-N" を "when multiple operations start in the same second" のときに付くと書いていた（ffe5ef72 で直した）。A21 の出典の旧資料（docs/archive/spec.md の「開始時にセッション ID を 1 回だけ決める」）は形式と順序だけが A21 に取り込まれ、ID をいつ決めるかは IR に入っていない。
- 契約テスト crates/remote-merge/tests/contract/backup_sessions.rs の a_rollback_confirmed_days_after_it_started_gets_an_id_unlike_a_removed_session（測定 1 と同じ流れで、予約する時刻を 2020-01-08 にずらす）は、予約する時刻を渡すだけで使わないコードでは二つの ID がどちらも "20200101-000000" になって失敗し、b7c04aa8 で成功する。`cargo nextest run --workspace --all-features` は 2148 件すべて成功した。

## Revisions

- 2026-10-23: A2 の why の「A1 の後も変える理由はない」は、[backup-reservations-cleanup-consistency A2](./2026-10-23-backup-reservations-cleanup-consistency.md#A2) が狭い案を退けた根拠（同じ記録の測定 2）が A1 の後は成り立たないことを見ていなかった。A2 が書き直した残す理由の文も、24 時間に満たないどの ID にもその秒のうちに予約する操作がいるように読めた。どちらも [backup-reservation-margin-consistency A1](./2026-10-23-backup-reservation-margin-consistency.md#A1) で置き換えた。24 時間の但し書きは変わらない。
