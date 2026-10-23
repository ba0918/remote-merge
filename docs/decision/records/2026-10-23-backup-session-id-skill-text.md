# 配布するスキル文書のセッション ID の説明を予約した時刻に合わせること

## Context

[backup-session-id-reservation-time A1](./2026-10-23-backup-session-id-reservation-time.md#A1) で、セッション ID の日時をコマンドを始めた時刻から予約した時刻に変え、[REQ-backup-022](../../ir/backup/sessions.md#REQ-backup-022) に書いた。
配布するスキル文書 skills/remote-merge/SKILL.md は、"-N" が「同じ秒に操作が始まったとき（when multiple operations start in the same second）」に付くと書いたままだった。
今は、同じ秒に始まっても予約する秒が違えば "-N" は付かず、違う秒に始まっても同じ秒に予約すれば付くので、この説明は REQ-backup-022 とも製品の振る舞いとも合わない。
この記録は、この食い違いをどう直したかを残す。

Position: スキル文書を REQ-backup-022 に合わせて直した（A1）。IR とコードは変えていない。

## Agreements

- A1 skills/remote-merge/SKILL.md のバックアップの説明を、"-N" は同じ秒の ID が既にあるときに付くこと、ID の日時は書き込みの前にセッションを予約した時刻（sync と rollback では確認の問いに答えた後）で、コマンドを始めた時刻ではないことに書き直す（ffe5ef72）。
  - why: スキル文書は配布する製品の一部で、利用者（とそれを読むエージェント）はこれで ID の読み方を知る。[adopt-backup A31](./2026-09-27-adopt-backup.md#A31) は、スキル文書のバックアップの説明を製品の振る舞いに一致させるとしている。IR を改める理由はない。REQ-backup-022 は [backup-session-id-reservation-time A1](./2026-10-23-backup-session-id-reservation-time.md#A1) の理由（ID の日時を始めた時刻のままにすると [REQ-backup-023](../../ir/backup/sessions.md#REQ-backup-023) に反する）で決まっていて、スキル文書の言い回しは IR ではないので、それを覆す根拠にならない。そのため、スキル文書を IR に合わせた。書き足した文は REQ-backup-022 の文を言い換えたもので、それより多くを約束しない。"-N" の条件は、REQ-backup-022 の「同じ日時の ID が既にあるときだけ "-N" を付ける」と同じにした。
  - rejected: "-N" の条件だけを直し、ID の日時がどの時点の時刻かを書かない案は、スキル文書を読む側が ID の日時をコマンドを始めた時刻と読む余地を残す。sync と rollback の ID の日時が確認に答えるまで後になることは、利用者が ID を操作と結び付けるときに見えるので、REQ-backup-022 にある以上はスキル文書にも書く。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- `rg -n -i "same second|同じ秒"` をリポジトリで実行し、配布する文書でセッション ID の "-N" を操作の始まりに結び付けていたのは skills/remote-merge/SKILL.md の 181〜182 行目（"with `-N` (`N >= 2`) when multiple operations start in the same second."）だけだった。skills/remote-merge/references/json-schemas.md と README.md には、ID の日時がどの時点の時刻かを書いた文はなかった。
- 文書の言い回しはテストで固定しない（契約として決めた言い回しではない）。直した後の `kotowari check --format json` に、範囲の IR の指摘はない。
