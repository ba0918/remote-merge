# パスの検査を経路と走査方式によらず揃えるときの読み方

## Context

diff・merge・sync の引数の検査と rollback の書き戻す前のパスの検査が、ローカル・SSH・エージェントの経路と走査方式によって違う結果を返していた。
[2026-10-05-quality-cli.md の A1](./2026-10-05-quality-cli.md#A1)、[2026-10-05-quality-merge.md の A1](./2026-10-05-quality-merge.md#A1)、[2026-10-05-quality-backup.md の A2](./2026-10-05-quality-backup.md#A2) で揃える方向は決まったが、実装するには IR が直接決めていない読み方がいくつか要る。
この記録は、それを揃える計画の承認（2026-10-05）で確定した読み方を残す。

## Agreements

- A1 書き戻し先のパスの祖先にリンク切れの symlink があり、その下のパスを検査するときは、[REQ-backup-003](../../ir/backup/rollback-path.md#REQ-backup-003) の「リンク先を辿れない」と読み、どの経路でもセッション全体を "cannot resolve path: " で止める。
  - why: ローカルとエージェントの経路は既にこう振る舞い、書き戻さない安全な側である。[REQ-backup-033](../../ir/backup/rollback-path.md#REQ-backup-033) の「リンク切れの symlink」は、戻す先そのもの（末尾）の symlink の例として書かれている。
  - decided_by: 利用者（計画の承認）
