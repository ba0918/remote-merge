# バックアップのテスト整理で残った問題を FLAG として残す判断

## Context

バックアップと rollback のテスト整理（[計画](../../plans/backup-test-cleanup.md)）で、変異テストの見逃しは [テスト手法の選び方の A8](./2026-09-27-test-method-selection.md#A8) に従い、テストの追加・同等変異の登録・FLAG の記録のどれかで決着させる。
整理後に残った見逃し 7 件は、IR が定めていない状況でしか観測できる違いが出ず、テストを足すと IR にない扱いを固定し、同等変異とも言えない。
また性質テストの試行で、セッション ID の生成が加算のあふれで panic することが分かった。
これらを仕様として決めずに、次にこのトピックを扱うときの論点として残す。経緯は "docs/testing/backup-test-cleanup.md" にある。

## Agreements

- A1 未決の FLAG として残す。既存のセッション ID の N が u64 の最大値のとき、次の ID を作る処理（src/backup/mod.rs の next_session_id）が加算のあふれで panic する。集約先の予約ディレクトリを手で作らない限り起きないが、そのときの扱い（エラーにするか、別の ID にするか）は IR に定めがない。
  - why: 利用者の判断で FLAG として記録する。起きる条件が人の手による集約先の書き換えに限られ、今決める必要はない。
  - decided_by: user (took the recommendation)
- A2 未決の FLAG として残す。セッション ID の予約や一時的な保存場所の作成が「既に存在する」以外の入出力エラー（パス長の上限、容量不足）で失敗したときの扱いが IR に定めがない。今の実装はコマンド全体をエラーで止めるが、それが仕様かは決まっていない。変異テストの見逃し 3 件（src/runtime/backup_store.rs の reserve_session と create_temporary_entry のエラーの分岐）は、このエラーを起こすテストがなければ落とせない。
  - why: 利用者の判断。約 4,000 バイトの集約先のパスでしか再現できないテストは現実の場面を表さず、扱いが決まるまでテストを足さない。観測できる違いがあるため同等変異にもしない。
  - decided_by: user (took the recommendation)
- A3 未決の FLAG として残す。集約先の中に製品が作らない名前や中身があるとき（"-1" や "-0" の接尾辞の ID、保存場所と食い違う path を持つ record.json、一時的な保存場所に残った名前）の扱いが IR に定めがなく、定めがあるのは中身が消されたとき（REQ-backup-041）だけである。変異テストの見逃し 4 件（src/backup/mod.rs の parse_session_id、src/runtime/backup_store.rs の read_record の二件と create_temporary_entry の一件）はこの状況でしか違いを生まない。
  - why: 利用者の判断。書き換えられた集約先への振る舞いを決めずにテストを足すと、IR にない扱いと集約先の内部の配置を固定することになる。
  - decided_by: user (took the recommendation)
