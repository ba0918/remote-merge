# Plan: バックアップと rollback のテストを要件の根拠に整理する

## Goal

バックアップと rollback の各要件に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、実装詳細をなぞるだけのテストは利用者の判断で消され、その過程が変異テストの結果で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/backup/storage.md#REQ-backup-011` から `#REQ-backup-016`
- `docs/ir/backup/failure.md#REQ-backup-017` から `#REQ-backup-020`
- `docs/ir/backup/sessions.md#REQ-backup-021` から `#REQ-backup-026`（REQ-backup-022 と REQ-backup-023 は verification が property）
- `docs/ir/backup/rollback-path.md#REQ-backup-027` から `#REQ-backup-033`
- `docs/ir/backup/rollback-cli.md#REQ-backup-034` から `#REQ-backup-041`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009` から `#REQ-testing-013`（いずれも review）

判断の出典は `docs/decision/records/2026-09-27-adopt-backup.md`（取り込み、FLAG、削除候補を含むテストの仕分け）、`docs/decision/records/2026-09-27-test-method-selection.md`（手法の選び方、変異テストとメモリ上限）、`docs/decision/records/2026-09-27-backup-session-id-properties.md`（セッション ID の性質テスト）。

## Approach and why

最初に変異テストを安全に回す道具（メモリ上限付きのスクリプトと同等変異の一覧）を作り、整理前の見逃しを記録する。見逃しは「根拠テストが要件を十分に確かめていない」ことの機械的な手掛かりになるため、審査の前に取る。

審査は IR の文書単位で進める。各要件について、既存テストのうち要件を十分に確かめるものを選び、`tests/contract/` の下の backup 用モジュールへ移して印を付ける。移すのは `.kotowari/config.yaml` の `tests.files` が `tests/contract/**/*.rs` を対象にしており、`tests/` 直下と `src/` 内のテストは印を付けても根拠に数えられないため。`tests.files` は広げない。既存の `tests/contract/` のテストが既に要件を十分に確かめている場合は、複製せずそのテストの印の行に要件の ID を足す。足りない要件にはテストを書き足す。

削除は利用者が一括で判断する。削除の前後で同じ対象に変異テストを回し、見逃しが増えたら削除を戻す。最後に残った見逃しは一件ずつテストの追加・同等変異の登録・FLAG の記録のどれかで決着させ、その記録を人が確かめられる文書に残す。

変異テストは S2 と S8 で同じ条件（`--all-features`、テストの実行には cargo nextest）で回す。SSH の fixture を使うテストは `tests/contract.rs` では `test-utils` の feature でしか組み込まれないため、条件をそろえないと移動だけで見逃しの数が変わる。変異テストの対象は `src/backup/mod.rs`、`src/service/rollback.rs`、`src/runtime/backup_store.rs` の三つに絞る。復元処理のある `src/runtime/side_io.rs` は 3,000 行を超え、ほかのトピックの処理が大半を占めるため対象にしない。

## Scope of change

- `scripts/mutants.sh`（新規）
- `.kotowari/config.yaml`（`mutants.equivalents` の追加だけ）
- `.kotowari/mutants-equivalents.yaml`（新規）
- `Cargo.toml` と `Cargo.lock`（dev-dependencies に proptest を加えるだけ）
- `proptest-regressions/`（proptest が書き出した場合）
- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の backup 用の新しいモジュール
- 既存の `tests/contract/*.rs`（既存テストの `@kotowari[...]` の印の行に ID を足すことだけ）
- `tests/common/mod.rs`（移すテストが使う補助関数の追加だけ。`tests/contract.rs` では `test-utils` の feature でだけ組み込まれることに注意）
- `tests/local_backup_store.rs`、`tests/cli_rollback.rs`、`tests/cli_rollback_local.rs`（移したテストの削除と、利用者が削除を決めたテストの削除）
- `src/backup/mod.rs`、`src/service/rollback.rs`、`src/cli/rollback.rs`、`src/handler/merge_file_io.rs`（`#[cfg(test)]` のテスト部分と、性質テストのために `src/backup/mod.rs` の関数の公開範囲を広げる場合だけ。ほかの製品コードは変えない）
- `docs/testing/backup-test-cleanup.md`（新規。変異テストの結果と見逃しの決着、テストの行き先の記録）

## Step order and prerequisites

S1 の道具がないと S2 以降の変異テストを安全に回せない。S2 の整理前の結果が S8 の比較の基準になる。S3 から S6 は IR の文書ごとの審査で互いに独立しているが、同じ `tests/contract.rs` と移し元のファイルを書き換えるため順に行う。S7 の削除は S3 から S6 で印の行き先が決まってから行う（根拠として移したテストを誤って消さないため）。S8 は S7 の後の状態で変異テストを回す。S9 で全体を確かめる。

作業ブランチは `adopt/backup`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-testing-013（review） | EX-testing-018, EX-testing-019 |
| S2 | REQ-testing-012（review） | — |
| S3 | REQ-backup-011, REQ-backup-012, REQ-backup-013, REQ-backup-014, REQ-backup-015, REQ-backup-016 | — |
| S4 | REQ-backup-017, REQ-backup-018, REQ-backup-019, REQ-backup-020 | — |
| S5 | REQ-backup-021, REQ-backup-022, REQ-backup-023, REQ-backup-024, REQ-backup-025, REQ-backup-026 | — |
| S6 | REQ-backup-027, REQ-backup-028, REQ-backup-029, REQ-backup-030, REQ-backup-031, REQ-backup-032, REQ-backup-033, REQ-backup-034, REQ-backup-035, REQ-backup-036, REQ-backup-037, REQ-backup-038, REQ-backup-039, REQ-backup-040, REQ-backup-041 | — |
| S7 | REQ-testing-012（review） | EX-testing-017 |
| S8 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | EX-testing-014, EX-testing-016 |
| S9 | 上記すべて | — |

## Left to the implementer

- `tests/contract/` の下の backup 用モジュールの分け方と名前（IR の文書に合わせるかどうかを含む）
- 移すテストの名前の付け直し（振る舞いを表す名前にする場合）
- テスト用の補助関数の置き場所（新しい contract モジュールに写すか、`tests/common/mod.rs` に足すか）
- `scripts/mutants.sh` が受け付ける環境変数の名前、結果の出力先ディレクトリ、メモリ上限による強制終了の検出方法（REQ-testing-013 の挙動を満たす限り）
- 性質テストで対象の関数を公開するか、公開された入口から確かめるか（`docs/decision/records/2026-09-27-test-method-selection.md#A10`）

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 既存の FLAG（`docs/ir/backup/FLAGS.md`）の挙動を確かめるテスト三件（`trailing_slash_does_not_change_remote_target_identity`、`rollback_reports_a_cyclic_symlink_as_an_unresolvable_path`、`rollback_reports_a_cyclic_parent_symlink_as_an_unresolvable_path`）を消すか書き換える必要が生じた
- 要件を確かめるために製品コードの挙動を変える、または `src/backup/mod.rs` 以外の製品コードの公開範囲を広げる必要が生じた
- 変異テストの見逃しや新しいテストが不具合の疑いを示した（FLAG として記録するかは利用者が決めるため、その見逃しは未決着のまま報告する）
- 変異テストが WSL のメモリを使い切りそうな兆候（スクリプトの上限に当たって失敗し続ける）があり、並列数を下げても解消しない

## Test command

```sh
cargo nextest run --all-features
```

変異テストは S1 で作る `scripts/mutants.sh` から実行し、cargo-mutants を直接実行しない。

## Out of scope

- FLAG-backup-001 から FLAG-backup-004 の決着と、その挙動の修正
- バックアップと rollback 以外のトピックのテストの整理と印付け（`tests/local_backup_store.rs` にある merge・symlink 側の四件は移さず残す）
- 変異テスト以外の重いコマンドのメモリ上限（`docs/decision/records/2026-09-27-test-method-selection.md` の U1）
- 既存の印付きテスト（REQ-backup-001 から 010 の例に付いたもの）の見直し

## Steps

### S1: メモリ上限付きの変異テストの実行手段を作る

- Purpose: 変異テストを WSL を落とさずに実行し、毎回新しい結果を kotowari mutants で読めるようにする
- Specification: docs/ir/testing/methods.md#REQ-testing-013, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: none
- May change: scripts/mutants.sh, .kotowari/config.yaml, .kotowari/mutants-equivalents.yaml
- Done when: `scripts/mutants.sh src/backup/mod.rs` が cargo-mutants を systemd-run --user のサービスとして MemoryHigh 35%・MemoryMax 40%・MemorySwapMax 0・並列数 2、`--all-features` と cargo nextest で起動し、その実行で作った結果を `kotowari mutants --tool cargo-mutants --format text` で読んだ出力を出す。メモリ上限を小さく上書きした実行は 0 以外で終了し、kotowari mutants の出力を出さない。設定の mutants.equivalents が空の同等変異の一覧を指し、kotowari mutants がそれを読んでもエラーにならない
- Shown by: external — 既定の設定で `scripts/mutants.sh src/backup/mod.rs` を実行して、最終行に `mutants: caught=... survived=...` が出ることと実行ログにメモリ上限の値が出ることを確認し、MemoryMax を 200M 程度に上書きして同じ対象で実行して 0 以外の終了コードと kotowari mutants の出力がないことを確認する
- Left to the implementer: 環境変数の名前、結果の出力先、強制終了の検出方法
- Stop and hand back if: systemd-run --user のサービスでメモリ上限による強制終了を検出できない（実行が 0 で終わってしまう）、または WSL で systemd のユーザーインスタンスが使えない

### S2: 整理前の変異テストの見逃しを記録する

- Purpose: 審査と削除の比較の基準にするため、整理前の見逃しを残す
- Specification: docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S1
- May change: docs/testing/backup-test-cleanup.md
- Done when: `src/backup/mod.rs`、`src/service/rollback.rs`、`src/runtime/backup_store.rs` の変異テストの集計（caught・survived・timeout・unviable）と、見逃し一件ずつの位置と変異の内容が `docs/testing/backup-test-cleanup.md` に書かれている
- Shown by: artifact — docs/testing/backup-test-cleanup.md の「整理前」の節に三ファイル分の集計行と見逃しの一覧がある
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 実行がメモリ上限で失敗し続ける

### S3: 保存の要件の根拠テストを移して印を付ける

- Purpose: 集約先への保存に関する要件ごとに十分な根拠テストを検査範囲に置く
- Specification: docs/ir/backup/storage.md#REQ-backup-011, docs/ir/backup/storage.md#REQ-backup-012, docs/ir/backup/storage.md#REQ-backup-013, docs/ir/backup/storage.md#REQ-backup-014, docs/ir/backup/storage.md#REQ-backup-015, docs/ir/backup/storage.md#REQ-backup-016
- Prerequisites: S2
- May change: tests/contract.rs, tests/contract/ の backup 用モジュール, tests/local_backup_store.rs, docs/testing/backup-test-cleanup.md
- Done when: 六つの要件それぞれについて `kotowari query` の tests が空でなく、印の付いたテストが要件の文を全て確かめており、移したテストは移し元から消え、移した先で通る。どのテストをどの要件の根拠にしたかが docs/testing/backup-test-cleanup.md にある
- Shown by: test — tests/local_backup_store.rs の aggregate_store_entries_are_owner_only_and_describe_target（REQ-backup-011, REQ-backup-012）、rollback_restores_the_content_seen_immediately_before_merge_writes（REQ-backup-013）、read_only_side_has_no_session_after_one_way_merge（REQ-backup-014）、new_file_merge_records_no_backup と new_file_in_a_missing_directory_records_no_backup と same_second_sessions_are_listed_newest_first_and_empty_operations_are_absent（REQ-backup-015）、legacy_backup_directory_is_ignored_by_list_and_status（REQ-backup-016）を審査し、十分なものを移して印を付け、不十分なら要件の文を満たすまで書き足す
- Left to the implementer: 一つのテストが複数の要件を確かめる場合に分けるかどうか
- Stop and hand back if: 候補のテストが要件の一部しか確かめられず、残りを確かめるテストに製品コードの変更が必要になる

### S4: 失敗時の扱いの要件の根拠テストを移して印を付ける

- Purpose: バックアップできないときと集約先が決まらないときの要件に十分な根拠テストを置く
- Specification: docs/ir/backup/failure.md#REQ-backup-017, docs/ir/backup/failure.md#REQ-backup-018, docs/ir/backup/failure.md#REQ-backup-019, docs/ir/backup/failure.md#REQ-backup-020
- Prerequisites: S3
- May change: tests/contract.rs, tests/contract/ の backup 用モジュール, 既存の tests/contract/*.rs の印の行, tests/common/mod.rs, tests/local_backup_store.rs, src/handler/merge_file_io.rs のテスト部分, docs/testing/backup-test-cleanup.md
- Done when: 四つの要件それぞれについて `kotowari query` の tests が空でない。REQ-backup-017 は merge・--delete・sync の三経路、REQ-backup-018 は merge と sync のそれぞれでバックアップ有効時に書き込む前に止まることと無効時にバックアップも期限切れの整理もせずに書き込むこと、REQ-backup-020 は TUI の公開された入口（handler の execute_merge と execute_write_changes、runtime の bootstrap_tui_with_targets）を通して、バックアップ失敗時に対象ファイルが変わらずステータス行にメッセージが出ること、w で両側がバックアップされ片側の失敗で両側とも書かれないこと、集約先がなくても起動して差分が取れることを確かめるテストに印がある。行き先は docs/testing/backup-test-cleanup.md に記録されている
- Shown by: test — tests/local_backup_store.rs の backup_store_failure_leaves_target_unchanged_and_reports_file_failure（REQ-backup-017 の merge）、tests/contract/merge_paths.rs の deletion_fails_without_removing_a_file_when_backup_cannot_be_saved の印の行に REQ-backup-017 を足す（--delete）、sync は書き足す。enabled_backup_without_store_location_stops_merge と disabled_backup_without_store_location_allows_merge と disabled_backup_merge_without_store_location_proceeds_without_cleanup（REQ-backup-018 の merge、sync の二件は書き足す）、rollback の集約先なしの六件（REQ-backup-019）、公開された TUI の入口を通して新しく書く REQ-backup-020 のテスト（src/handler/merge_file_io.rs の decide_backup_write の三件は純粋関数の比較だけなので根拠にせず S7 の削除候補に挙げる）
- Left to the implementer: TUI の入口を関数呼び出しで使うか PTY を使うか
- Stop and hand back if: TUI の書き込み拒否や起動を公開された入口から確かめられず、製品コードの変更が要る

### S5: セッションと期限の要件の根拠テストを整え、性質テストを書く

- Purpose: 書き込み先の区別、セッション ID、期限切れの整理の要件に十分な根拠テストを置き、ID の順序と一意性を性質として確かめる
- Specification: docs/ir/backup/sessions.md#REQ-backup-021, docs/ir/backup/sessions.md#REQ-backup-022, docs/ir/backup/sessions.md#REQ-backup-023, docs/ir/backup/sessions.md#REQ-backup-024, docs/ir/backup/sessions.md#REQ-backup-025, docs/ir/backup/sessions.md#REQ-backup-026
- Prerequisites: S4
- May change: Cargo.toml, Cargo.lock, proptest-regressions/, tests/contract.rs, tests/contract/ の backup 用モジュール, 既存の tests/contract/*.rs の印の行, tests/common/mod.rs, tests/local_backup_store.rs, src/backup/mod.rs, src/service/rollback.rs のテスト部分, docs/testing/backup-test-cleanup.md
- Done when: 六つの要件それぞれについて `kotowari query` の tests が空でない。REQ-backup-021 にはポートだけが違う二つのリモート書き込み先とホストだけが違う二つのリモート書き込み先のそれぞれで、一方の一覧に他方のセッションが出ないことを確かめるテストがある。REQ-backup-022 と REQ-backup-023 には proptest の性質テストが印付きであり、REQ-backup-023 には集約先を通した同じ秒・並行の作成のテスト（merges_started_in_the_same_second_use_distinct_session_ids と concurrent_merges_use_distinct_session_ids）にも印がある。既存の具体例のテスト（十番目が九番目の後ろに並ぶ、同じ秒の二回の作成で ID が異なる）も残っている。proptest は dev-dependencies にだけあり、性質テストは試す入力の数を上書きしていない
- Shown by: test — 新しく書く「ポートだけ違う書き込み先のセッションが分かれる」「ホストだけ違う書き込み先のセッションが分かれる」テストは現状の実装で RED にならないことを先に確かめる（RED になったら実装の欠陥として止まる）、REQ-backup-022 の「作られた ID は形式に合い、任意の二つの ID の並びが日時の順、同じ日時なら N の数値の順（-N なしが最初）に一致する」性質テスト（-N 以外の接尾辞の受け付けや拒否は IR が定めていないので性質にしない）と REQ-backup-023 の「任意の既存 ID の集合に対して次に作る ID がどれとも重ならない」性質テスト、tests/local_backup_store.rs の sessions_for_two_write_targets_remain_separate と local_root_symlink_retargeting_keeps_existing_sessions_visible（REQ-backup-021）、sync_uses_one_session_id_for_all_targets（REQ-backup-024）、rollback_accepts_a_same_second_session_id_with_numeric_suffix と rollback_without_session_uses_the_newest_numeric_suffix（REQ-backup-022 の --session の受け付けと最新の選択）、期限切れの整理の七件（REQ-backup-025）、expired_session_is_marked_in_text_and_json_at_the_injected_boundary と src/backup/mod.rs の session_expires_at_retention_boundary と src/service/rollback.rs の mark_expired の四件を公開された入口から確かめる形にしたもの（REQ-backup-026）
- Left to the implementer: 性質テストの入力の生成器の作り方、src/backup/mod.rs の単体テストを移すか残すか
- Stop and hand back if: ポートやホストだけが違う書き込み先でセッションが混ざる（実装が REQ-backup-021 と食い違う。FLAG 候補）、性質テストが実装の反例を見つけた、proptest を加えられない

### S6: 書き戻しと rollback コマンドの要件の根拠テストを移して印を付ける

- Purpose: rollback の書き戻し方とコマンドの振る舞いの要件に十分な根拠テストを置く
- Specification: docs/ir/backup/rollback-path.md#REQ-backup-027, docs/ir/backup/rollback-path.md#REQ-backup-028, docs/ir/backup/rollback-path.md#REQ-backup-029, docs/ir/backup/rollback-path.md#REQ-backup-030, docs/ir/backup/rollback-path.md#REQ-backup-031, docs/ir/backup/rollback-path.md#REQ-backup-032, docs/ir/backup/rollback-path.md#REQ-backup-033, docs/ir/backup/rollback-cli.md#REQ-backup-034, docs/ir/backup/rollback-cli.md#REQ-backup-035, docs/ir/backup/rollback-cli.md#REQ-backup-036, docs/ir/backup/rollback-cli.md#REQ-backup-037, docs/ir/backup/rollback-cli.md#REQ-backup-038, docs/ir/backup/rollback-cli.md#REQ-backup-039, docs/ir/backup/rollback-cli.md#REQ-backup-040, docs/ir/backup/rollback-cli.md#REQ-backup-041
- Prerequisites: S5
- May change: tests/contract.rs, tests/contract/ の backup 用モジュール, 既存の tests/contract/*.rs の印の行, tests/common/mod.rs, tests/local_backup_store.rs, tests/cli_rollback.rs, tests/cli_rollback_local.rs, src/service/rollback.rs と src/cli/rollback.rs のテスト部分, docs/testing/backup-test-cleanup.md
- Done when: 十五の要件それぞれについて `kotowari query` の tests が空でない。REQ-backup-035 には "y" と "yes" では書き戻し、"n" と空の応答では書き戻さないことを確かめるテストがある（断ったときの終了コードは FLAG-backup-004 の範囲なので確かめない）。REQ-backup-028 にはバックアップ無効時に rollback が新しいセッションを作らず pre_rollback_backup を出さないことを確かめるテストがある。重複して同じ振る舞いを確かめるテストは、十分な方にだけ印があり、もう一方は S7 の削除候補として docs/testing/backup-test-cleanup.md に挙がっている
- Shown by: test — tests/local_backup_store.rs の enabled_rollback_reports_the_backup_taken_before_restore と rollback_does_not_restore_a_file_when_its_current_content_cannot_be_backed_up（REQ-backup-027）、新しく書くバックアップ無効時の rollback のテスト（REQ-backup-028）、rollback_restores_file_content_without_changing_existing_permissions（REQ-backup-029）、rollback_restores_a_file_removed_by_merge_delete（REQ-backup-030）、rollback_skips_deleted_file_when_its_parent_no_longer_exists（REQ-backup-031）、src/service/rollback.rs の replaced_symlink_takes_priority_over_a_changed_destination を公開された入口から確かめる形にしたもの（REQ-backup-032）、パスの変化のスキップ五件（REQ-backup-033）、tests/cli_rollback.rs と tests/cli_rollback_local.rs の各テスト（REQ-backup-034, REQ-backup-036, REQ-backup-037, REQ-backup-039, REQ-backup-040）、src/service/rollback.rs の exit_code で始まる五件と src/cli/rollback.rs の test_rollback_exit_code で始まる三件を公開された入口から確かめる形にしたものと tests/cli_rollback.rs の test_rollback_exit_code_no_sessions（REQ-backup-038）、tests/local_backup_store.rs の rollback_treats_a_session_with_missing_content_as_not_found と session_with_missing_content_is_omitted_without_failing_the_list（REQ-backup-041）、新しく書く確認プロンプトのテスト（REQ-backup-035）
- Left to the implementer: CLI の E2E テストと関数呼び出しのテストのどちらを根拠にするか（要件の文を全て確かめる方を選ぶ）
- Stop and hand back if: 確認プロンプトを標準入力から与えるテストが書けない、または断ったときの終了コードの確認が必要になる（FLAG-backup-004 の範囲）

### S7: 削除候補を利用者に一括で判断してもらい、決まったものを消す

- Purpose: 実装詳細をなぞるだけのテストと、根拠にしなかった重複テストを利用者の判断で消す
- Specification: docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S6
- May change: src/backup/mod.rs, src/service/rollback.rs, src/cli/rollback.rs, src/handler/merge_file_io.rs のテスト部分, tests/local_backup_store.rs, tests/cli_rollback.rs, tests/cli_rollback_local.rs, docs/testing/backup-test-cleanup.md
- Done when: 利用者が削除候補の一覧（取り込みの決定記録の Context で数えた実装詳細の十一件：src/service/rollback.rs の parse_batch_output で始まる六件、src/backup/mod.rs の test_extract_timestamp_valid・test_extract_timestamp_invalid・test_parse_backup_timestamp、src/cli/rollback.rs の test_resolve_target_with_name・test_resolve_target_local、および S3 から S6 で根拠にしなかった重複テストと純粋関数の比較だけのテスト）を一度に見て削除するものを決め、決まったものだけが消え、判断の結果が docs/testing/backup-test-cleanup.md に記録されている
- Shown by: external — 削除候補の一覧を利用者に一度に示し、利用者の返答（消すものの一覧）を受けてから削除し、削除後に `cargo nextest run --all-features` が通ることを確認する
- Left to the implementer: 一覧の示し方
- Stop and hand back if: 利用者が一覧のうち一部の判断を保留した（保留分は消さずに残して報告する）

### S8: 整理後の変異テストで見逃しを比べ、全て決着させる

- Purpose: 削除で見逃しが増えていないことを確かめ、残った見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S7
- May change: tests/contract/ の backup 用モジュール, tests/contract.rs, tests/common/mod.rs, .kotowari/mutants-equivalents.yaml, src/backup/mod.rs, src/service/rollback.rs, src/cli/rollback.rs, src/handler/merge_file_io.rs のテスト部分, tests/local_backup_store.rs, tests/cli_rollback.rs, tests/cli_rollback_local.rs, docs/testing/backup-test-cleanup.md
- Done when: S2 と同じ三ファイルの変異テストの見逃しが S2 の見逃しの部分集合になっている（増えた見逃しを生んだ削除は戻してある）。残った見逃しの全てに、足したテストか、同等変異の一覧への理由付きの登録（別の文脈でその変異を落とすテストを書こうとして書けなかった記録つき）が docs/testing/backup-test-cleanup.md に記録されている。さらに同じ文書に、REQ-backup-011 から 041 の verification とそれが要件の性質に合う理由の一行ずつと、性質テストが tests/contract/ の下にあり試す入力の数を上書きしておらず proptest-regressions/ のファイルがあればコミットされていることの確認がある
- Shown by: external — `scripts/mutants.sh src/backup/mod.rs src/service/rollback.rs src/runtime/backup_store.rs` の出力と S2 の記録を突き合わせた結果を docs/testing/backup-test-cleanup.md の「整理後」の節に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: none
- Stop and hand back if: 見逃しが不具合の疑いを示した（FLAG として記録するかを利用者が決めるまで S8 は完了しない。実装は直さない）、同等変異の判断のために別の文脈の確認が得られない

### S9: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/backup/storage.md#REQ-backup-011, docs/ir/backup/rollback-cli.md#REQ-backup-041, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S8
- May change: none
- Done when: REQ-backup-011 から REQ-backup-041 の全てで `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for i in $(seq 11 41); do kotowari query REQ-backup-0$i | jq -e '.items[0].tests != []' > /dev/null || echo "missing REQ-backup-0$i"; done`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
