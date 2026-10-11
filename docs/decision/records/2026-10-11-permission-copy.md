# 権限の写しと作ったディレクトリの権限の失敗を知らせ、rollback --dry-run の失敗を終了コードに出す

## Context

品質の台帳の ao-5・ao-6・ao-7 と、T10 の merge_sync-7 を直す単位である。

- ao-5・merge_sync-7: `--with-permissions` で、読み込み元の権限が 0（mode 000）のとき、または読み込み元がリモートで権限を読めないとき、`copy_permissions`（`crates/remote-merge/src/service/merge_flow.rs`）は権限を写さずに成功扱いにする。[REQ-merge-036](../../ir/merge/permissions.md#REQ-merge-036) は読み込み元の権限にローカルとリモートのどちらでも走査したツリーの値を使うと決めているが、リモートの読み込み元は実装されていない（[FLAG-merge-002](../../ir/merge/FLAGS.md)）。0 の扱いは [FLAG-merge-007](../../ir/merge/FLAGS.md) に未決のまま残っている。
- ao-6: エージェントが書き込みで作ったディレクトリの権限の変更に失敗しても、`apply_dir_permissions`（`crates/remote-merge-agent/src/agent/file_io.rs`）は debug のログだけ出して成功扱いにする。
- ao-7: `rollback --dry-run` は、failed があっても終了コード 0 を返す（接続断のときだけ 2）。

利用者は 2026-10-11 に、品質の台帳の単位を順に自走して進めてよいと指示した。ここに書く決定は、どれも承認済みの要件と決定から導いたもので、新しい方針は足していない。

Position: 決定は A1〜A4 で出揃った。

## Agreements

- A1 `--with-permissions` の merge と sync は、読み込み元の権限が 0 でも、その 0 を書き込み先に写す。[FLAG-merge-007](../../ir/merge/FLAGS.md) を閉じて消し、[REQ-merge-014](../../ir/merge/permissions.md#REQ-merge-014) の例 EX-merge-056 を足す。
  - why: REQ-merge-014 は読み込み元のファイル権限を書き込み先に反映すると決めていて、0 を除いていない。0 を写さずに成功扱いにすると、利用者は権限が揃ったと誤解する。
  - decided_by: 利用者（[spec-migration A40](./2026-09-25-spec-migration.md#A40) の REQ-merge-014 の当てはめ。2026-10-11 の自走の指示により、この単位で書き下ろした）
- A2 `--with-permissions` で読み込み元の権限が分からないとき（走査したツリーに権限の値がない）は、権限を写さず、中身は書いたことと権限を写せなかったことが分かる error で failed に出し、終了コード 2 を返す。リモートの読み込み元は、REQ-merge-036 のとおり走査したツリーの値を使って写し、[FLAG-merge-002](../../ir/merge/FLAGS.md) を閉じて消す。[REQ-merge-037](../../ir/merge/permissions.md#REQ-merge-037) に書き足す。
  - why: 権限の変更に失敗したときの扱いは [quality-merge A5](./2026-10-05-quality-merge.md#A5) で REQ-merge-037 に決めてある。写せないことは写すのに失敗したことと同じで、黙って成功にすると利用者は気づけない。リモートの読み込み元は REQ-merge-036 が既に求めている。
  - decided_by: 利用者（[quality-merge A4](./2026-10-05-quality-merge.md#A4)・[A5](./2026-10-05-quality-merge.md#A5) の当てはめ。2026-10-11 の自走の指示により、この単位で書き下ろした）
- A3 エージェントの経路でも、書き込みで作ったディレクトリの権限の変更に失敗したら、ローカルと SSH の経路と同じく、そのファイルを中身は書いたことが分かる error で failed に出す。IR は変えない。
  - why: [REQ-merge-012](../../ir/merge/permissions.md#REQ-merge-012) は新しいディレクトリを設定された権限で作ると決めていて、ローカルと SSH の経路は失敗を failed に出している。エージェントだけ失敗を捨てるのは経路差である。
  - decided_by: 利用者（REQ-merge-012 の当てはめ。2026-10-11 の自走の指示により、この単位で書き下ろした）
- A4 `rollback --dry-run` も、failed があれば実行時と同じく終了コード 2 を返す。[REQ-backup-038](../../ir/backup/rollback-cli.md#REQ-backup-038) を書き直し、例 EX-backup-031 を足す。
  - why: merge と sync の --dry-run は、failed があれば実行時と同じく 2 を返すと決めてある（[quality-cli A7](./2026-10-05-quality-cli.md#A7)、[REQ-cli-091](../../ir/cli/safety.md#REQ-cli-091)）。理由は、dry-run を前段の検査に使ったときに失敗を見落とさないためで、rollback にも同じく当てはまる。REQ-backup-038 の「--dry-run は 0」は旧資料と実装の一致をそのまま仕様にしたもので（[adopt-backup A5](./2026-09-27-adopt-backup.md#A5)）、失敗の場合を決めたものではない。
  - decided_by: 利用者（quality-cli A7 の当てはめ。2026-10-11 の自走の指示により、この単位で書き下ろした）

## Revisions

- 2026-10-11: [FLAG-merge-002](../../ir/merge/FLAGS.md) を A2 で、[FLAG-merge-007](../../ir/merge/FLAGS.md) を A1 で閉じ、IR から消した。
- 2026-10-11: [REQ-backup-038](../../ir/backup/rollback-cli.md#REQ-backup-038) の「--dry-run は 0 を返す」を A4 で改めた。
