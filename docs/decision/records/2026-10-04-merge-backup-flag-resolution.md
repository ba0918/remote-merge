# merge とバックアップの判断のいらない FLAG を閉じる際の判断

## Context

merge とバックアップの話題には、取り込みのときに未決として残した FLAG が溜まっていた。
2026-10-04 に利用者が「仕分けして判断がいらないものを全て対処して」と指示したため、すべての FLAG を読み取りだけで仕分け、承認済みの要件か現行のコードで決まるものを選んだ。
この記録は、そのうち merge とバックアップの FLAG について、既に決まっているので閉じるもの、現行の挙動をそのまま仕様にするもの、承認済みの要件に合わせてコードを直すものを残す。
利用者の判断が要るものと、前提が確かめられなかったものは FLAG のまま残す。

## Agreements

- A1 FLAG-merge-001 を閉じる。中身まで同じファイルを明示した merge は書き込まず、skipped にも出さず、テキストでは "no files to merge in the specified path(s)" を出す今の挙動を正とし、旧資料の reason "identical" は採らない。
  - why: 承認済みの [REQ-merge-005](../../ir/merge/comparison.md#REQ-merge-005) が中身の同じファイルを更新しないと定め、承認済みの [REQ-cli-049](../../ir/cli/merge.md#REQ-cli-049) がこの場合のテキストを定めており、契約テストが確かめている。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A2 FLAG-merge-008 を IR を変えずに閉じる。種類の違いによるスキップの reason の文言は契約にせず、今の "source and destination have different file types" はそのまま残す。
  - why: [REQ-merge-001](../../ir/merge/symlink.md#REQ-merge-001) は理由付きでスキップすることだけを定めて文言を定めず、契約テストも文言に依存しない。旧資料の文言に合わせる理由は承認済みの要件にない。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A3 FLAG-merge-009 を IR を変えずに閉じる。--delete の対象が symlink のときのスキップの reason の文言は契約にせず、計画のときの "destination is a symlink" と削除の直前の "destination is a symlink; deletion skipped" はそのまま残す。
  - why: [REQ-merge-002](../../ir/merge/symlink.md#REQ-merge-002) は理由付きでスキップすることだけを定めて文言を定めず、契約テストも理由が空でないことだけを確かめる。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A4 FLAG-backup-003 を閉じる。--force のない rollback で機密ファイルを一件でもスキップしたときは、他のファイルを戻せても終了コード 2 を返す。
  - why: 承認済みの [REQ-backup-036](../../ir/backup/rollback-cli.md#REQ-backup-036) は機密ファイルの書き戻しをスキップとして報告すると定め、承認済みの [REQ-backup-038](../../ir/backup/rollback-cli.md#REQ-backup-038) はスキップがあれば 2 を返すと定めており、実装も同じである。食い違うのは旧資料だけである。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A5 FLAG-merge-020 を閉じる。利用者向けの手引き "skills/remote-merge/SKILL.md" の hunk merge の節の機密ファイルの記述を、--hunks では --force がないとエラーで止まるという内容に直す。
  - why: 承認済みの [TBL-merge-001](../../ir/merge/hunks.md#TBL-merge-001) と [REQ-merge-028](../../ir/merge/hunks.md#REQ-merge-028) が、機密ファイルを --force なしで hunk merge すると "Sensitive file 'パス' requires --force for hunk merge" で止まると定めており、手引きの「自動でスキップ」はこれと食い違う。同じ行の更新日時による確認の記述は未決の FLAG-merge-017 に関わるため変えない。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A6 FLAG-merge-005 を IR を変えずに閉じる。中身を読み比べないファイルの書き込み先を読めなかったときの failed の error の文言は IR で定めず、今の読み取りのエラーそのものを出す挙動はそのまま残す。
  - why: 実装はこの場合もそのファイルを書かずに failed に出し、他のファイルの処理を続けており、[REQ-merge-017](../../ir/merge/read-failure.md#REQ-merge-017) と同じく読めないファイルを上書きしない。[REQ-merge-019](../../ir/merge/read-failure.md#REQ-merge-019) の文言は中身の読み比べの場合に限られ、この場合の文言を決める承認済みの要件はない。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A7 FLAG-backup-004 を閉じ、[REQ-backup-035](../../ir/backup/rollback-cli.md#REQ-backup-035) の末尾に、確認に "y" と "yes" 以外で答えたときは標準エラーに "Aborted." を出し、何も書き戻さずに終了コード 0 で終わるという文を足す。
  - why: 今の rollback はそのとおりに動く。REQ-backup-035 はこの答えのときに書き戻さないことをすでに定めており、足す文はそれと矛盾しない。確認を断ったときに何も書かず 0 で終わるのは、承認済みの sync の確認の扱い [REQ-cli-040](../../ir/cli/sync.md#REQ-cli-040) とも揃う。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A8 FLAG-backup-002 を閉じ、[REQ-backup-003](../../ir/backup/rollback-path.md#REQ-backup-003) の末尾に、書き戻し対象のどれか一件でもリンク先を辿れないときもそのセッションのファイルは一件も書き戻さず、すべてを failed に error "cannot resolve path: " に続く原因で報告するという文を足す。
  - why: 今の rollback は書き戻す前の確認で一件でも辿れないと、そのセッションのすべてのファイルをこの error で failed に出し、何も書き戻さない。REQ-backup-003 はリンク先が変わったときにセッション全体を書き戻さないと定めており、足す文はそれと矛盾せず、同じくセッション単位で止める扱いを辿れない場合に広げるだけである。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A9 FLAG-merge-006 を閉じ、[REQ-merge-011](../../ir/merge/concurrency.md#REQ-merge-011) の後に [REQ-merge-033](../../ir/merge/concurrency.md#REQ-merge-033) を足す。merge と sync は、比べたときから書き込み先の中身が変わったファイルと、比べたときになかった書き込み先が現れたファイルを書かずに failed に出し、他のファイルの処理は続ける。failed の error の文言は定めない。
  - why: CLI の merge と sync は、書き込み直前の確認で検知した変更をそのファイルの失敗として failed に入れ、次のファイルへ進む。REQ-merge-011 が求めるのは古い状態を前提とした書き込みを止めて変更を報告することで、止める範囲をそのファイルに限るこの挙動はそれを満たし、読めないファイルをそのファイルだけ失敗にする [REQ-merge-017](../../ir/merge/read-failure.md#REQ-merge-017) とも揃う。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A10 FLAG-merge-014 を閉じ、[REQ-merge-034](../../ir/merge/deletion.md#REQ-merge-034) を足す。--delete の merge と sync は、削除の直前に書き込み先の種類を調べられなかったファイルと、削除に失敗したファイルを failed に出し、他のファイルの処理は続ける。failed の error の文言は定めず、旧資料の skipped の reason "cannot resolve path: <原因>" は採らない。
  - why: 今の削除の処理はこの二つの場合をそのファイルの失敗として failed に入れ、次の対象へ進む。[REQ-merge-008](../../ir/merge/deletion.md#REQ-merge-008) と [REQ-merge-016](../../ir/merge/deletion.md#REQ-merge-016) はこの場合を定めておらず、削除しなかったものを失敗として知らせるこの挙動は、バックアップに失敗した対象を削除せずに failed に出す REQ-merge-016 と [REQ-backup-017](../../ir/backup/failure.md#REQ-backup-017) の扱いと揃う。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A11 FLAG-merge-018 を閉じ、--hunks の merge も --max-entries で指定した件数の上限で走査するようにコードを直す。IR は変えない。
  - why: 承認済みの [REQ-scan-003](../../ir/scan/limits.md#REQ-scan-003) は利用者が走査の件数の上限を変更できると定め、merge の --max-entries のヘルプも設定の上限を上書きするとしているが、--hunks の merge だけが指定を受け取りながら使わず、設定の上限で走査していた。要件が一意に読め、直すのは指定を走査に渡す一か所である。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
