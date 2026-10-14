# rollback --dry-run のテキストに出すものを IR に書き足す

## Context

[2026-10-11-permission-copy](./2026-10-11-permission-copy.md) の [A4](./2026-10-11-permission-copy.md#A4) で、`rollback --dry-run` は failed があれば終了コード 2 を返すようになった。その直しで、dry-run のテキストにも戻せないファイルを `✗` と理由で出すようにした。
ところが IR には dry-run のテキストに何を出すかを書いた文がない。[REQ-backup-008](../../ir/backup/rollback-path.md#REQ-backup-008) は「復元予定とブロック理由を報告する」とだけ書き、[REQ-backup-037](../../ir/backup/rollback-cli.md#REQ-backup-037) は実行時の結果の形（集計行 "Restored N file(s)" を含む）を書いている。dry-run のテキストは集計行を出さないので、REQ-backup-037 を dry-run にそのまま当てると実装と食い違う。
IR と実装をどちらに合わせるかを、ここで決める。

Position: 決定は A1 で出揃った。

## Agreements

- A1 `rollback --dry-run` のテキストは、戻す予定のファイルを "✓"、スキップを "-" と理由、失敗を "✗" と理由で、実行時の結果と同じ記号で示し、集計行は出さない。[REQ-backup-037](../../ir/backup/rollback-cli.md#REQ-backup-037) にこの文を足し、[EX-backup-031](../../ir/backup/rollback-cli.md#EX-backup-031) の Then にテキストでそのファイルを "✗" と理由で示すことを足す。見出しの文言と dry-run の JSON の形は、この範囲で変えていないので決めない。
  - why: [permission-copy A4](./2026-10-11-permission-copy.md#A4) は dry-run を前段の検査に使ったときに失敗を見落とさないために終了コード 2 を返すと決めた。テキストに失敗したファイルと理由がなければ、2 の原因が分からず同じ目的を果たせない。REQ-backup-008 の「ブロック理由を報告する」もこれに沿う。集計行を出さないことは直しの前から変わらない実装の振る舞いで（`crates/remote-merge/src/service/output.rs` の `format_rollback_dry_run_text`、直し前は `crates/remote-merge/src/cli/rollback.rs` の DryRun の分岐）、IR のどの文とも矛盾しない。どの経路（ローカル・SSH・エージェント）でも dry-run の結果は `crates/remote-merge/src/cli/rollback.rs` の同じ `restore_backup` の呼び出しから作られ、同じ関数で書き出される。測ったこと: 直しの前は `rg -n "dry-run" docs/ir/backup/` に dry-run のテキストの形を書いた文がなかった。足したテストの `dry_run_text_marks_the_planned_and_skipped_files_without_a_summary` は通り、整形の関数に集計行を足す使い捨ての変更（コミットしていない）では失敗した。
  - decided_by: 一貫性の段（consistency phase）
