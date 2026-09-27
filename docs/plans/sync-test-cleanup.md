# Plan: sync のテストを要件の根拠に整理する

## Goal

sync の取り込みで加えた各要件に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、実装詳細をなぞるだけのテストは利用者の判断で消され、その過程が変異テストの結果で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/cli/sync.md#REQ-cli-038` から `#REQ-cli-045`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`、`#REQ-testing-013`（いずれも review）

判断の出典は `docs/decision/records/2026-09-27-adopt-sync.md`（取り込み、FLAG、TUI の sync を作らない判断、テストの仕分けの件数と範囲外にしたもの）と `docs/decision/records/2026-09-27-test-method-selection.md`（手法の選び方、変異テストとメモリ上限）。要件の本文は `kotowari query REQ-cli-0nn` で読む。決定表 TBL-cli-006 と TBL-cli-007 は各要件の `- definition:` から辿れる。同じ手順を status で行った記録 `docs/testing/status-test-cleanup.md` が記録の書き方の手本になる。

## Approach and why

最初に整理前の変異テストの見逃しを記録する。見逃しは根拠テストが要件を十分に確かめていないことの機械的な手掛かりになり、削除の前後の比較の基準にもなる。変異テストは既存の `scripts/mutants.sh`（メモリ上限付き）からだけ実行し、対象は sync の規則を持つ `src/service/sync.rs`、`src/cli/sync.rs`、`src/service/source_pair.rs` の三つに絞る。`src/service/output.rs` と `src/service/types.rs` は status・diff・merge の出力が大半を占め、sync のテキスト出力は FLAG-cli-012 で未決のため対象にしない。

見逃しのうち決着の対象にするのは、sync の入口の規則を持つ関数のものに限る: `src/service/sync.rs` の `compute_target_status`・`compute_sync_summary`・`sync_exit_code`、`src/service/source_pair.rs` の `resolve_source_pairs`、`src/cli/sync.rs` の `run_sync`・`print_sync_result`・`validate_sync_args`・`build_dry_run_targets`・`print_sync_plan`・`execute_sync`。`print_sync_result` のテキストの分岐（"No files to sync." と `format_sync_text`）の見逃しは FLAG-cli-012 の範囲として記録する。`src/service/source_pair.rs` の `validate_server` は status・diff・merge と共有するため、その見逃しは sync の入口を通すテストで落とせる場合だけ決着させ、それ以外は記録だけする。ただし `execute_sync` の中でも、左右の比較・中身の読み比べ・削除計画・merge 計画の組み立て（`compute_status_from_trees` から `plan_deletions` と `skip_symlink_deletions` の呼び出しまでの区間）は merge と共通の振る舞いで、merge の取り込みで扱うため、そこに入った見逃しは記録だけする。`src/service/sync.rs` の `plan_deletions` と `skip_symlink_deletions`、`src/service/source_pair.rs` の status 用の関数の見逃しも記録だけする。

審査は要件ごとに進める。各要件について、既存テストのうち要件の文と決定表の行を十分に確かめるものを選び、`tests/contract/` の下の sync 用の新しいモジュールへ移して印を付ける。移すのは `.kotowari/config.yaml` の `tests.files` が `tests/contract/**/*.rs` を対象にしており、`src/` 内のテストは印を付けても根拠に数えられないため。`tests.files` は広げない。`tests/contract/cli_results.rs` の sync の三件（`a_failed_sync_target_is_reported_separately_with_a_nonzero_exit_code`、`every_successful_sync_target_returns_a_zero_exit_code`、`a_connection_failure_on_one_target_does_not_prevent_the_other_sync`）が既に要件を確かめている場合は、複製せずその印の行に ID を足す。

根拠テストは sync の公開された入口を通す。関数呼び出しなら `remote_merge::cli::sync::execute_sync` に `RuntimeTargets::with_local` でサーバをローカルのディレクトリに差し替えて渡す（`tests/contract/cli_results.rs` の `sync_fixture` の形）。関数呼び出しのテストは `force: true`（または `dry_run: true`）を渡す。`force: false` で書き込む予定があると `execute_sync` がテストのプロセスの標準入力を読み、結果がテストの実行環境の標準入力に左右されるため。確認のプロンプトは実行ファイルを通してだけ観測する。

確認のプロンプト（標準入力と標準エラー）は、`tests/common/mod.rs` の `CliEnv`（SSH の fixture、`test-utils` の feature が要る）で実行ファイルを動かし、標準入力に答えを渡して確かめる（`tests/contract/backup_rollback_cli_e2e.rs` の `rollback_answering` の形）。`CliEnv::new_3way` はリモートの develop と staging を持つため、`sync <パス> --left local --right develop staging` で二つの書き込み先への確認を作れる（--right は値を複数取るため、パスは --left と --right より前に書く）。staging のディレクトリは `CliEnv` のフィールドにないため、`temp_root()` の下の "staging" から辿る。エラーで止まったときの終了コード 2 は、TBL-cli-006 のエラーが接続より前に起きるため、SSH の fixture を使わずに `remote_merge_cmd` と一時的な設定ファイルで確かめてよい。`tests/contract.rs` は `common` を `test-utils` の feature があるときだけ読み込むため、関数呼び出しのテストの補助関数は新しい contract モジュール（または feature で囲まない隣のモジュール）に置き、`CliEnv` を使うテストとその補助関数だけを `test-utils` で囲む。足りない要件にはテストを書き足す。

新しく書くテストは FLAG-cli-006 から 014 の挙動を確かめない。具体的には、接続に失敗した書き込み先を含む結果の並び（FLAG-cli-008）、確認を断ったときの標準出力（FLAG-cli-007）、テキスト出力の形（FLAG-cli-012）、dry-run の削除予定の表示（FLAG-cli-013）、読めないファイルを含む dry-run の状態と終了コード（FLAG-cli-014）、種類の違いによるスキップと状態（FLAG-cli-010）、削除の成否と状態（FLAG-cli-011）を検証の対象に含めない。

削除は利用者が一括で判断する。削除の前後で同じ対象に変異テストを回し、見逃しが増えたら削除を戻す。最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG（FLAG-cli-006 から 014）の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。別の文脈を立てられないときは登録せず候補として手渡す。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の sync 用の新しいモジュール
- `tests/contract/cli_results.rs`（sync の三件の印の行に ID を足すことだけ）
- `tests/common/mod.rs`（移すテストが使う補助関数の追加だけ）
- `src/cli/sync.rs`、`src/service/sync.rs`、`src/service/source_pair.rs`、`src/service/types.rs` の `#[cfg(test)]` のテスト部分（移したテストと利用者が削除を決めたテストの削除だけ。製品コードは変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/sync-test-cleanup.md`（新規。変異テストの結果と見逃しの決着、テストの行き先、削除の判断の記録）

## Step order and prerequisites

S1 の整理前の結果が S6 の比較の基準になるため最初に取る。S2 から S4 は要件ごとの審査で互いに独立しているが、同じ `tests/contract.rs` と移し元のファイルを書き換えるため順に行う。S5 の削除は S2 から S4 で印の行き先が決まってから行う（根拠として移したテストを誤って消さないため）。S6 は S5 の後の状態で変異テストを回す。S7 で全体を確かめる。

S5 と S6 には利用者の判断を待つ区切りがある。S5 は削除候補の一覧を `docs/testing/sync-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の返答（消すもの）がその文書に書き足されてから削除を再開する。S6 は新しい FLAG の候補や verification の見直しの候補があれば、同じ文書に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/sync`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-testing-012（review）, REQ-testing-013（review） | — |
| S2 | REQ-cli-038 | — |
| S3 | REQ-cli-039, REQ-cli-040 | — |
| S4 | REQ-cli-041, REQ-cli-042, REQ-cli-043, REQ-cli-044, REQ-cli-045 | — |
| S5 | REQ-testing-012（review） | — |
| S6 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S7 | 上記すべて | — |

## Left to the implementer

- `tests/contract/` の下の sync 用モジュールの分け方と名前
- 移すテストの名前の付け直し（振る舞いを表す名前にする場合）
- テスト用の補助関数の置き場所（新しい contract モジュールに写すか、`tests/common/mod.rs` に足すか）
- 要件ごとに関数呼び出しと実行ファイルのどちらを根拠にするか（要件の文と決定表の行を全て確かめる方を選ぶ）

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- FLAG-cli-006 から FLAG-cli-014 の挙動を確かめるテスト（`src/service/output.rs` の `format_sync_text_basic`・`format_sync_text_with_delete`・`format_sync_text_multiple_servers`・`format_sync_text_with_skipped`・`format_sync_text_dry_run`・`format_sync_text_with_backup`・`format_sync_text_delete_failed`、`src/cli/sync.rs` の `build_dry_run_targets_includes_deletions`）を移すか消すか書き換える必要が生じた
- 要件の verification が要件の性質に合わないと判断した（IR は直さず、S6 で候補として手渡す）
- 要件を確かめるために製品コードの挙動や公開範囲を変える必要が生じた
- 変異テストの見逃しや新しいテストが不具合の疑いを示した（FLAG として記録するかは利用者が決めるため、その見逃しは未決着のまま報告する）
- 変異テストが WSL のメモリを使い切りそうな兆候（スクリプトの上限に当たって失敗し続ける）があり、並列数を下げても解消しない

## Test command

```sh
cargo nextest run --all-features
```

変異テストは `scripts/mutants.sh` から実行し、cargo-mutants を直接実行しない。並列数を 2 より上げない。

## Out of scope

- FLAG-cli-006 から FLAG-cli-014 の決着と、その挙動の修正
- 既存の印付きテスト（REQ-merge-015 と REQ-cli-018・019 の例に付いたもの）の見直し
- merge と共通の要件のテスト（`tests/contract/merge_paths.rs`・`tests/contract/backup_*.rs`・`tests/contract/scan_limits.rs`・`tests/contract/filters.rs`・`tests/contract/rollback_paths.rs` にある sync のテスト）の移動と印付け
- `src/service/sync.rs` の `plan_deletions` の四件（削除は merge の取り込みで扱う）
- 面の検査（`surface`）の導入

## Steps

### S1: 整理前の変異テストの見逃しを記録する

- Purpose: 審査と削除の比較の基準にするため、整理前の見逃しを残す
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-013
- Prerequisites: none
- May change: docs/testing/sync-test-cleanup.md
- Done when: `src/service/sync.rs`、`src/cli/sync.rs`、`src/service/source_pair.rs` を一回の実行にまとめた変異テストの全体の集計（caught・survived・timeout・unviable）、ファイルごとの内訳、見逃し一件ずつの位置と変異の内容と決着の対象かどうか（Approach and why の区別）が、実行したコミットとともに `docs/testing/sync-test-cleanup.md` に書かれている。検知のうち、変異と関係のないテスト（tui_merge や agent_ssh のように負荷の下で落ちるもの）だけで検知されたものが名前とともに記録されている
- Shown by: artifact — `scripts/mutants.sh src/service/sync.rs src/cli/sync.rs src/service/source_pair.rs` の出力を docs/testing/sync-test-cleanup.md の「整理前」の節に集計と見逃しの表として書く
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 実行がメモリ上限で失敗し続ける、負荷の下で落ちるテストによる見かけの検知が多く、整理前と整理後の比較が成り立たない

### S2: 読み込み元と書き込み先の指定の根拠テストを移して印を付ける

- Purpose: sync が受け付ける指定と TBL-cli-006 のエラーに十分な根拠テストを検査範囲に置く
- Specification: docs/ir/cli/sync.md#REQ-cli-038
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の sync 用モジュール, tests/common/mod.rs, src/cli/sync.rs と src/service/source_pair.rs のテスト部分, docs/testing/sync-test-cleanup.md
- Done when: REQ-cli-038 の `kotowari query` の tests が空でない。印付きテストは sync の入口で、--left 一つと --right 二つの指定が受け付けられることと、TBL-cli-006 の五行（--left がない、--right がない、--right の名前が重なる、--right の一つが --left と同じ、設定にないサーバ名を --left と --right のそれぞれに指定する）でそれぞれの文言のエラーになり書き込み先が変わらないことを確かめる。移したテストは移し元から消え、移した先で通る。どのテストを根拠にしたかが docs/testing/sync-test-cleanup.md にある
- Shown by: test — src/service/source_pair.rs の resolve_source_pairs_two_servers・resolve_source_pairs_duplicate_server_error・resolve_source_pairs_unknown_server_error・resolve_source_pairs_left_equals_right_error と src/cli/sync.rs の validate_missing_left・validate_empty_right・validate_valid_args_passes・validate_multiple_right_servers を審査し、十分なものを execute_sync を通す形で移して印を付け、足りない行（設定にない --left など）を書き足す
- Left to the implementer: 一つのテストで決定表の複数の行を確かめるか分けるか
- Stop and hand back if: 指定のエラーが接続より前に起きず、ローカルの差し替えだけでは TBL-cli-006 の行を観測できない

### S3: 処理の順と確認の根拠テストを書いて印を付ける

- Purpose: 書き込み先を指定順に処理することと、書き込む前の一度だけの確認に根拠テストを置く
- Specification: docs/ir/cli/sync.md#REQ-cli-039, docs/ir/cli/sync.md#REQ-cli-040
- Prerequisites: S2
- May change: tests/contract.rs, tests/contract/ の sync 用モジュール, tests/common/mod.rs, docs/testing/sync-test-cleanup.md
- Done when: REQ-cli-039 と REQ-cli-040 の `kotowari query` の tests が空でない。REQ-cli-039 の印付きテストは、接続できる二つの書き込み先を二通りの順で指定し、結果の targets がそれぞれ指定順に並ぶことを確かめる。REQ-cli-040 の印付きテストは実行ファイルで、書き込むファイルと --delete による削除予定の両方がある二つの書き込み先に対し、標準エラーに "Sync: 元 -> 先1, 先2" と書き込み先ごとの "[先] N files to merge, M files to delete" が含まれ "Proceed? [y/N] " が一度だけ出ること、"y" と "Y" で書き込まれること、それ以外の答えで "Sync cancelled." が出て書き込み先が変わらず終了コード 0 になること、--force・--dry-run・書き込む予定がないときには尋ねないことを確かめる。どのテストを根拠にしたかが docs/testing/sync-test-cleanup.md にある
- Shown by: test — 既存の根拠テストはないため、execute_sync を二つの with_local の書き込み先で呼ぶ順のテストと、CliEnv::new_3way で --left local --right develop staging を実行して標準入力に答えを渡す確認のテストを書く
- Left to the implementer: 答えごとにテストを分けるか一つのテストで回すか
- Stop and hand back if: 件数が 0 の部分を省いた行（例: 削除予定のない書き込み先の "[先] N files to merge"）を IR と一致するとみなすかの判断が要る確かめ方しか作れない、SSH の fixture で実行ファイルの sync が書き込み先に接続できず、確認のプロンプトを観測できる構成が作れない

### S4: 状態・集計・JSON・終了コード・dry-run の根拠テストを移して印を付ける

- Purpose: 書き込み先ごとの状態、全体の集計、JSON の形、終了コード、dry-run の要件に十分な根拠テストを置く
- Specification: docs/ir/cli/sync.md#REQ-cli-041, docs/ir/cli/sync.md#REQ-cli-042, docs/ir/cli/sync.md#REQ-cli-043, docs/ir/cli/sync.md#REQ-cli-044, docs/ir/cli/sync.md#REQ-cli-045
- Prerequisites: S3
- May change: tests/contract.rs, tests/contract/ の sync 用モジュール, tests/contract/cli_results.rs の印の行, tests/common/mod.rs, src/cli/sync.rs と src/service/sync.rs と src/service/types.rs のテスト部分, docs/testing/sync-test-cleanup.md
- Done when: 五つの要件それぞれについて `kotowari query` の tests が空でない。REQ-cli-041 は TBL-cli-007 の四行を、dry-run でなく force を指定して書き込みまで進む経路で確かめる（書き込む予定のある書き込み先が一つもないと状態は別の経路で決まるため、どの行も、書き込む予定のある別の書き込み先を同じ実行に含める。"partial" は同じ書き込み先に書けるファイルと書き込み先を読めないファイルを置いて作り、両方ないときの "success" は一つ目の書き込み先に差分があり二つ目の書き込み先が同じ中身の構成で作る）。REQ-cli-042 は二つ以上の書き込み先で summary の五項目がそれぞれの定義どおりに数えられ、successful_servers が "success" の書き込み先だけを数えることを確かめる。REQ-cli-043 は JSON の left・targets（target・merged・skipped・deleted・failed・status）・summary の形と、deleted が空でも出ることと status の小文字の値を確かめる。REQ-cli-044 は全て "success" で 0、"partial" か "failed" が一つでもあれば 2、エラーで止まったとき 2 を確かめ、エラーの 2 は実行ファイルの終了コードで確かめる。REQ-cli-045 は --dry-run で書き込む予定のファイルが merged に status "would merge" で並び、書き込み先が変わらないことを確かめる。行き先は docs/testing/sync-test-cleanup.md に記録されている
- Shown by: test — src/service/sync.rs の compute_target_status の四件（REQ-cli-041）、compute_sync_summary_multiple_servers（REQ-cli-042）、src/service/types.rs の sync_target_result_deleted_empty_included と sync_target_status_serializes_lowercase（REQ-cli-043）、src/service/sync.rs の sync_exit_code の二件（REQ-cli-044）、tests/contract/cli_results.rs の a_failed_sync_target_is_reported_separately_with_a_nonzero_exit_code（REQ-cli-041 の "failed" と "success" の行、REQ-cli-044）と every_successful_sync_target_returns_a_zero_exit_code と a_connection_failure_on_one_target_does_not_prevent_the_other_sync（REQ-cli-044）、src/cli/sync.rs の build_dry_run_targets_includes_would_merge（REQ-cli-045）を審査し、十分なものを execute_sync を通す形で移して印を付け、cli_results.rs の三件は印の行に ID を足し（summary を確かめていないため REQ-cli-042 は足さない）、足りない行を書き足す。src/cli/sync.rs の build_dry_run_targets_includes_connection_failures は接続に失敗した書き込み先の dry-run での状態だけを確かめ REQ-cli-045 の根拠にならないため移さず、S5 の削除候補に挙げる
- Left to the implementer: JSON の形を format_json の出力で確かめるか SyncOutput を serde_json の値にして確かめるか
- Stop and hand back if: "partial" や読めないファイルを作るのに権限を落とす構成が要り、root で実行される環境でその前提が成り立たない

### S5: 削除候補を利用者に一括で判断してもらい、決まったものを消す

- Purpose: 実装詳細をなぞるだけのテストと、根拠にしなかった重複テストを利用者の判断で消す
- Specification: docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S4
- May change: src/cli/sync.rs, src/service/sync.rs, src/service/source_pair.rs, src/service/types.rs のテスト部分, docs/testing/sync-test-cleanup.md
- Done when: 利用者が削除候補の一覧（src/cli/sync.rs の validate_empty_paths（パスの必須は `src/main.rs` の clap の `required = true` が execute_sync より前に止めるため、この分岐は実行ファイルからは届かない。FLAG-cli-006 の挙動は clap の側にあることを一覧に添える）と validate_rejects_invalid_format と build_dry_run_targets_includes_connection_failures、および S2 から S4 で根拠にしなかった重複テスト）を一度に見て削除するものを決め、決まったものだけが消え、判断の結果が docs/testing/sync-test-cleanup.md に記録されている
- Shown by: external — 削除候補の一覧（src/service/types.rs の候補には同じ分岐を確かめて残るテストの名前と、変異テストの裏付けがないこと）を docs/testing/sync-test-cleanup.md に書いてコミットして作業を返し、利用者の返答（消すものの一覧）が同じ文書に書き足されてから削除し、削除後に `cargo nextest run --all-features` が通ることを確認する
- Left to the implementer: 一覧の示し方
- Stop and hand back if: 利用者が一覧のうち一部の判断を保留した（保留分は消さずに残して報告する）

### S6: 整理後の変異テストで見逃しを比べ、全て決着させる

- Purpose: 削除で見逃しが増えていないことを確かめ、決着の対象に残った見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S5
- May change: tests/contract/ の sync 用モジュール, tests/contract.rs, tests/common/mod.rs, .kotowari/mutants-equivalents.yaml, S5 で挙げた各ファイルのテスト部分, docs/testing/sync-test-cleanup.md
- Done when: S1 と同じ三ファイルの変異テストの見逃しが、S1 の見逃しと、S1 で負荷の下で落ちるテストだけに検知された変異とを合わせたものの部分集合になっている（削除で増えた見逃しを生んだ削除は戻してある。S1 で負荷の下で落ちるテストだけに検知され S6 で見逃しになった変異は、決着の対象なら他の見逃しと同じく決着させる）。決着の対象の見逃しの全てに、足したテスト、同等変異の一覧への理由付きの登録（別の文脈のエージェントが落とすテストを書けなかった試みつき）、既存の FLAG-cli-0NN の範囲としての記録、新しい FLAG の候補としての報告のどれかが docs/testing/sync-test-cleanup.md に記録されている。決着の対象でない見逃しは一覧として記録されている。同じ文書に REQ-cli-038 から 045 の verification とそれが要件の性質に合う理由が一行ずつあり、合わないと判断したものは IR を直さず候補として挙がっている
- Shown by: external — `scripts/mutants.sh src/service/sync.rs src/cli/sync.rs src/service/source_pair.rs` の出力と S1 の記録を突き合わせた結果を docs/testing/sync-test-cleanup.md の「整理後」の節に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: none
- Stop and hand back if: 見逃しが不具合の疑いを示した、または verification の見直しの候補がある（どちらも文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S6 は完了しない。実装と IR は直さない）、負荷の下で落ちるテスト（tui_merge や agent_ssh）による見かけの検知が集計に混ざり、比較が成り立たない

### S7: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/cli/sync.md#REQ-cli-038, docs/ir/cli/sync.md#REQ-cli-039, docs/ir/cli/sync.md#REQ-cli-040, docs/ir/cli/sync.md#REQ-cli-041, docs/ir/cli/sync.md#REQ-cli-042, docs/ir/cli/sync.md#REQ-cli-043, docs/ir/cli/sync.md#REQ-cli-044, docs/ir/cli/sync.md#REQ-cli-045, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S6
- May change: none
- Done when: REQ-cli-038 から REQ-cli-045 の全てで `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in $(seq -f 'REQ-cli-%03g' 38 45); do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
