# Plan: status のテストを要件の根拠に整理する

## Goal

status の取り込みで加えた各要件と例に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、実装詳細をなぞるだけのテストは利用者の判断で消され、その過程が変異テストの結果で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/cli/status.md#REQ-cli-027`、`docs/ir/cli/status.md#REQ-cli-028`、例 EX-cli-062
- `docs/ir/cli/status-output.md#REQ-cli-029` から `#REQ-cli-033`
- `docs/ir/cli/status-targets.md#REQ-cli-034` から `#REQ-cli-037`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`、`#REQ-testing-013`（いずれも review）

判断の出典は `docs/decision/records/2026-09-27-adopt-status.md`（取り込み、FLAG、hunks を作らない判断、テストの仕分けの件数）と `docs/decision/records/2026-09-27-test-method-selection.md`（手法の選び方、変異テストとメモリ上限）。要件の本文は `kotowari query REQ-cli-0nn` で読む。決定表 TBL-cli-001 から TBL-cli-005 は各要件の `- definition:` から辿れる。

## Approach and why

最初に整理前の変異テストの見逃しを記録する。見逃しは根拠テストが要件を十分に確かめていないことの機械的な手掛かりになり、削除の前後の比較の基準にもなる。変異テストは既存の `scripts/mutants.sh`（メモリ上限付き）からだけ実行し、対象は status の判定と比較対象の決め方を持つ `src/service/status.rs`、`src/cli/status.rs`、`src/cli/ref_guard.rs`、`src/service/source_pair.rs` と、サイズ・更新時刻・symlink のリンク先による判定を実際に決める `compare_metadata` を持つ `src/tree.rs` の五つに絞る。`src/service/output.rs` と `src/service/types.rs` は diff・merge・sync の出力が大半を占めるため対象にしない。`src/service/status.rs` の `verified_content_pairs`、`needs_explicit_file_compare`、`needs_merge_content_compare`、`status_from_read_results` と `src/service/source_pair.rs` の `resolve_source_pairs`、`src/tree.rs` の `compare_metadata` 以外の関数は status の判定の規則を持たない（diff・merge・sync・TUI の処理、またはツリーの構造の操作）ため、その見逃しは記録だけして決着の対象にしない。この計画で「status の入口」は実行ファイルの `status` サブコマンドを指し、`run_status` と `print_status_result` を含む。

審査は IR の文書単位で進める。各要件について、既存テストのうち要件の文と決定表の行を十分に確かめるものを選び、`tests/contract/` の下の status 用モジュールへ移して印を付ける。移すのは `.kotowari/config.yaml` の `tests.files` が `tests/contract/**/*.rs` を対象にしており、`tests/` 直下と `src/` 内のテストは印を付けても根拠に数えられないため。`tests.files` は広げない。既存の `tests/contract/status_results.rs` のテストが既に要件を確かめている場合は、複製せずその印の行に ID を足す。

根拠テストは status の公開された入口を通す。関数呼び出しなら `remote_merge::cli::status::execute_status` に `RuntimeTargets::with_local` でサーバをローカルのディレクトリに差し替えて渡し（`tests/contract/status_results.rs` の fixture の形）、標準エラー・終了コード 2・テキストの出力のように実行ファイルを通さないと観測できないものは `tests/common/mod.rs` の `CliEnv`（SSH の fixture、`test-utils` の feature が要る）で実行ファイルを動かす。`execute_status` は右がサーバで `--ref` がないとき、ハッシュの経路（`try_hash_compare` から `refine_status_with_hashes`）で比べ、右が local か `--ref` があるときは中身を読む経路（`refine_status_with_content`）で比べる。with_local で差し替えたサーバもハッシュの経路に入るため、中身で判定する行（TBL-cli-001 の最後の行と REQ-cli-008 の再比較）は、右をサーバにした構成（ハッシュの経路）と `--left` にサーバ・`--right` に "local" を指定した構成（中身を読む経路）の両方で確かめる。ローカルでは作れない入力（サイズや更新時刻が分からないノード）の行だけは、公開関数 `compute_status_from_trees`・`needs_content_compare`・`refine_status_with_content`（`remote_merge::service::status`）を組み合わせ、組み立てた `FileTree` と中身で、同じなら "equal"・違えば "modified" の両方を確かめてよい。足りない要件にはテストを書き足す。

削除は利用者が一括で判断する。削除の前後で同じ対象に変異テストを回し、見逃しが増えたら削除を戻す。`src/service/output.rs` と `src/service/types.rs` は変異テストの対象外なので、この二ファイルのテストの削除候補には、同じ整形の分岐を確かめて残るテストの名前を一件ずつ添え、変異テストの裏付けがないことを利用者に示す一覧に書く。最後に残った見逃しは、status の入口から呼ばれる関数のものだけを一件ずつ、テストの追加、同等変異の登録、既存の FLAG（FLAG-cli-001 から 005）の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。別の文脈を立てられないときは登録せず候補として手渡す。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の status 用の新しいモジュールと `tests/contract/status_results.rs`
- 既存の `tests/contract/*.rs`（既存テストの `@kotowari[...]` の印の行に ID を足すことだけ）
- `tests/common/mod.rs`（移すテストが使う補助関数の追加だけ）
- `tests/cli_status.rs`、`tests/cli_exit_codes.rs`、`tests/cli_error_handling.rs`（移したテストの削除と、利用者が削除を決めたテストの削除。diff・merge・config のテストは触らない）
- `src/service/status.rs`、`src/cli/status.rs`、`src/cli/ref_guard.rs`、`src/service/source_pair.rs`、`src/service/output.rs`、`src/service/types.rs`、`src/tree.rs` の `#[cfg(test)]` のテスト部分（移したテストと利用者が削除を決めたテストの削除だけ。製品コードは変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/status-test-cleanup.md`（新規。変異テストの結果と見逃しの決着、テストの行き先、削除の判断の記録）

## Step order and prerequisites

S1 の整理前の結果が S6 の比較の基準になるため最初に取る。S2 から S4 は IR の文書ごとの審査で互いに独立しているが、同じ `tests/contract.rs` と移し元のファイルを書き換えるため順に行う。S5 の削除は S2 から S4 で印の行き先が決まってから行う（根拠として移したテストを誤って消さないため）。S6 は S5 の後の状態で変異テストを回す。S7 で全体を確かめる。

S5 と S6 には利用者の判断を待つ区切りがある。S5 は削除候補の一覧を `docs/testing/status-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の返答（消すもの）がその文書に書き足されてから削除を再開する。S6 は新しい FLAG の候補や verification の見直しの候補があれば、同じ文書に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/status`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test と scenario_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-testing-012（review）, REQ-testing-013（review） | — |
| S2 | REQ-cli-027, REQ-cli-028 | EX-cli-062 |
| S3 | REQ-cli-029, REQ-cli-030, REQ-cli-031, REQ-cli-032, REQ-cli-033 | — |
| S4 | REQ-cli-034, REQ-cli-035, REQ-cli-036, REQ-cli-037 | — |
| S5 | REQ-testing-012（review） | — |
| S6 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S7 | 上記すべて | EX-cli-062 |

## Left to the implementer

- `tests/contract/` の下の status 用モジュールの分け方と名前（IR の文書に合わせるかどうかを含む）
- 移すテストの名前の付け直し（振る舞いを表す名前にする場合）
- テスト用の補助関数の置き場所（新しい contract モジュールに写すか、`tests/common/mod.rs` に足すか）
- 要件ごとに関数呼び出しと実行ファイルのどちらを根拠にするか（要件の文と決定表の行を全て確かめる方を選ぶ）

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- FLAG-cli-001 から FLAG-cli-005 の挙動を確かめるテスト（`src/cli/status.rs` の `test_determine_agent_status_remote_no_agent` と `test_determine_agent_status_local`、`src/service/output.rs` の agent を扱う五件（`test_format_status_text_agent_connected`、`test_format_status_text_agent_fallback`、`test_format_status_text_no_agent`、`test_status_json_agent_field`、`test_status_json_no_agent_field_when_none`）、`src/service/types.rs` の `test_agent_status_serialize` と `test_agent_status_deserialize`、"all_equal" と "exists_only_in_ref" を扱う `src/service/status.rs` の `test_compute_ref_badges_all_equal` と `test_compute_ref_summary` と `src/service/output.rs` の `test_format_status_text_with_ref_badges`）を移すか消すか書き換える必要が生じた
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

- FLAG-cli-001 から FLAG-cli-005 の決着と、その挙動の修正
- 既存の印付きテスト（REQ-cli-006 から 008 の例 EX-cli-011 から EX-cli-016 に付いたもの）の見直し
- status 以外のトピックのテストの整理と印付け（除外フィルター、走査件数の上限、ディレクトリ symlink、バックアップ置き場、エラー時の JSON、設定の誤り、diff・merge・sync のテストは移さず残す）
- 使われなくなる `hunks` の項目を製品コードから消すこと
- 面の検査（`surface`）の導入

## Steps

### S1: 整理前の変異テストの見逃しを記録する

- Purpose: 審査と削除の比較の基準にするため、整理前の見逃しを残す
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-013
- Prerequisites: none
- May change: docs/testing/status-test-cleanup.md
- Done when: `src/service/status.rs`、`src/cli/status.rs`、`src/cli/ref_guard.rs`、`src/service/source_pair.rs`、`src/tree.rs` を一回の実行にまとめた変異テストの全体の集計（caught・survived・timeout・unviable）、ファイルごとの内訳、見逃し一件ずつの位置と変異の内容と決着の対象かどうか（Approach and why の区別）が、実行したコミットとともに `docs/testing/status-test-cleanup.md` に書かれている。検知のうち、変異と関係のないテスト（tui_merge や agent_ssh のように負荷の下で落ちるもの）だけで検知されたものが名前とともに記録されている
- Shown by: artifact — `scripts/mutants.sh src/service/status.rs src/cli/status.rs src/cli/ref_guard.rs src/service/source_pair.rs src/tree.rs` の出力を docs/testing/status-test-cleanup.md の「整理前」の節に集計と見逃しの表として書く
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 実行がメモリ上限で失敗し続ける、負荷の下で落ちるテストによる見かけの検知が多く、整理前と整理後の比較が成り立たない

### S2: 判定の要件の根拠テストを移して印を付ける

- Purpose: メタデータと中身による判定と symlink の判定の要件に十分な根拠テストを検査範囲に置く
- Specification: docs/ir/cli/status.md#REQ-cli-027, docs/ir/cli/status.md#REQ-cli-028
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の status 用モジュール, tests/contract/status_results.rs, tests/common/mod.rs, tests/cli_status.rs, src/service/status.rs と src/tree.rs のテスト部分, docs/testing/status-test-cleanup.md
- Done when: REQ-cli-027、REQ-cli-028、EX-cli-062 のそれぞれで `kotowari query` の tests が空でない。REQ-cli-027 の印付きテストは TBL-cli-001 の六行を全て確かめ、サイズと更新時刻が同じで中身が違うファイルが既定では "equal" になること（中身を読まない）を含み、最後の行はハッシュの経路と中身を読む経路の両方で確かめる。REQ-cli-028 の印付きテストは両方 symlink でリンク先が同じ・違う場合と、片方だけ symlink の場合を確かめ、リンク先の文字列が同じで左右のリンク先の中身が違う（またはリンク先がない）組が "equal" になることで中身を読まないことを確かめる。EX-cli-062 の印付きテストは通常ファイルと、同じ中身の同じサイズのファイルを指す symlink の組が status で "modified" になることを確かめる。移したテストは移し元から消え、移した先で通る。どのテストをどの要件の根拠にしたかが docs/testing/status-test-cleanup.md にある
- Shown by: test — src/service/status.rs の compute_status_from_trees を確かめる test_status_left_only・test_status_right_only・test_status_nested_files・test_status_file_vs_directory_path_conflict_is_modified、メタデータ比較の四件、refine_status_with_content の七件、symlink の三件（test_status_symlink_same_target_is_equal・test_status_symlink_different_target_is_modified・test_status_symlink_vs_file_is_modified）と tests/cli_status.rs の test_status_text_shows_modified_files・test_status_text_shows_left_only・test_status_text_shows_right_only を審査し、十分なものを入口を通す形で移して印を付け、EX-cli-062 のテストと足りない行のテストを書き足す
- Left to the implementer: 一つのテストで決定表の複数の行を確かめるか分けるか
- Stop and hand back if: ローカルの入口で更新時刻と中身を揃えた構成が作れず、TBL-cli-001 の行を公開関数の組み合わせからも確かめられない

### S3: 出力と終了コードの要件の根拠テストを移して印を付ける

- Purpose: 集計、終了コード、出力形式とテキスト、JSON、集計だけの表示の要件に十分な根拠テストを置く
- Specification: docs/ir/cli/status-output.md#REQ-cli-029, docs/ir/cli/status-output.md#REQ-cli-030, docs/ir/cli/status-output.md#REQ-cli-031, docs/ir/cli/status-output.md#REQ-cli-032, docs/ir/cli/status-output.md#REQ-cli-033
- Prerequisites: S2
- May change: tests/contract.rs, tests/contract/ の status 用モジュール, tests/contract/status_results.rs, 既存の tests/contract/*.rs の印の行, tests/common/mod.rs, tests/cli_status.rs, tests/cli_exit_codes.rs, src/service/status.rs と src/cli/status.rs と src/service/output.rs と src/service/types.rs のテスト部分, docs/testing/status-test-cleanup.md
- Done when: 五つの要件それぞれについて `kotowari query` の tests が空でない。REQ-cli-029 は --all なしと --summary ありでも "equal" が集計に数えられることを確かめる。REQ-cli-030 は TBL-cli-002 の三行（0・1・2）を実行ファイルの終了コードで確かめ、2 は SSH なしで起こせる status のエラー（設定にないサーバ名、または左右が同じ）で確かめる（tests/cli_error_handling.rs の設定の誤りのテストは config の話題なので移さない）。REQ-cli-031 は "text"・"json"・"diff" の受け付けと未知の値のエラー、見出し、TBL-cli-003 の四つの記号、機密ファイルの " [SENSITIVE]"、Summary 行を確かめる。REQ-cli-032 は "left"・"right"・"files"・"summary" の形と "status" の四つの値を確かめる。REQ-cli-033 はテキストでファイルの行が出ないことと JSON で "files" が出ないことを確かめる。行き先は docs/testing/status-test-cleanup.md に記録されている
- Shown by: test — src/cli/status.rs の test_summary_equal_count_preserved_after_filter と tests/cli_status.rs の test_status_excludes_equal_by_default（REQ-cli-029）、tests/cli_exit_codes.rs の test_status_exit_0_when_no_diff と test_status_exit_1_when_diff_found と src/service/status.rs の exit code の三件（REQ-cli-030、エラーの 2 は S4 で移す tests/cli_error_handling.rs の二件を兼ねるか書き足す）、src/service/output.rs の test_format_status_text・test_status_header_without_ref・test_status_text_left_only_symbol・test_status_text_right_only_symbol と test_output_format_parse と tests/cli_status.rs の test_status_sensitive_files_included（REQ-cli-031、" [SENSITIVE]" と "diff" は書き足す）、src/service/types.rs の test_status_output_serialize・test_status_summary_serialize・test_file_status_kind_serializes_snake_case と tests/cli_status.rs の test_status_json_format・test_status_json_special_chars_in_path（REQ-cli-032）、src/service/status.rs の test_build_status_output_summary_only と src/service/output.rs の test_format_status_text_summary_only・test_status_header_appears_in_summary_only_mode と tests/cli_status.rs の test_status_summary_shows_counts（REQ-cli-033）を審査し、十分なものを入口を通す形で移して印を付け、足りない行を書き足す
- Left to the implementer: 出力の形を実行ファイルの出力で確かめるか execute_status の結果と公開の整形関数で確かめるか
- Stop and hand back if: "Comparing: 左 ↔ 右" の見出しや Summary 行を確かめるために、IR にない文言（区切りの空行の数や行の順序）を固定する必要が生じた

### S4: 比較対象と三者比較の要件の根拠テストを移して印を付ける

- Purpose: 左右の決め方、参照先との違いの印、機密ファイルの印、左右と同じ参照先の要件に十分な根拠テストを置く
- Specification: docs/ir/cli/status-targets.md#REQ-cli-034, docs/ir/cli/status-targets.md#REQ-cli-035, docs/ir/cli/status-targets.md#REQ-cli-036, docs/ir/cli/status-targets.md#REQ-cli-037
- Prerequisites: S3
- May change: tests/contract.rs, tests/contract/ の status 用モジュール, tests/contract/status_results.rs, 既存の tests/contract/*.rs の印の行, tests/common/mod.rs, tests/cli_status.rs, tests/cli_error_handling.rs, src/service/status.rs と src/cli/ref_guard.rs と src/service/source_pair.rs と src/service/output.rs と src/service/types.rs のテスト部分, docs/testing/status-test-cleanup.md
- Done when: 四つの要件それぞれについて `kotowari query` の tests が空でない。REQ-cli-034 は TBL-cli-004 の四行と、左右が同じとき・既定サーバで補って同じになったとき（エラーにその旨がある）・設定にないサーバ名・サーバが一つもないときのエラーを status の入口で確かめる。既定サーバで補ったことは、エラーに既定サーバの名前と "default server" が含まれることだけで確かめ、ほかの文言は固定しない。REQ-cli-035 は TBL-cli-005 の三行、JSON の "ref" と "ref_differs"・"ref_only"・"ref_missing"、テキストの見出しの "(ref: 参照先の名前)" と Ref 行を確かめる（"all_equal" と "exists_only_in_ref" は FLAG-cli-002 の範囲なので確かめない）。REQ-cli-036 は機密ファイルが参照先にないときの "missing_in_ref" と、あるときに中身が違っても印が付かないことを確かめる。REQ-cli-037 は参照先が左と同じとき・右と同じときのそれぞれで警告の文言が標準エラーに出て、三者比較をせず比較が続くことを実行ファイルで確かめる。行き先は docs/testing/status-test-cleanup.md に記録されている
- Shown by: test — src/service/source_pair.rs の status に関わる十九件（resolve_source_pairs で始まる sync の四件を除く）と tests/cli_error_handling.rs の test_invalid_server_name_rejected と test_self_compare_rejected（REQ-cli-034）、src/service/status.rs の compute_ref_badges の七件（test_compute_ref_badges_all_equal を除く）と src/service/output.rs の test_status_header_with_ref と src/service/types.rs の test_status_output_with_ref_serialize と tests/cli_status.rs の test_status_with_ref_shows_badges（REQ-cli-035）、src/service/status.rs の機密ファイルの印の四件（REQ-cli-036）、src/cli/ref_guard.rs の四件（REQ-cli-037、標準エラーの文言は書き足す）を審査し、十分なものを入口を通す形で移して印を付け、足りない行を書き足す
- Left to the implementer: 三者比較の参照先を with_local で差し替えるか SSH の fixture を使うか
- Stop and hand back if: 参照先が左右と同じ構成を実行ファイルで作ると SSH の接続が先に失敗し、警告を観測できる構成が fixture でも作れない

### S5: 削除候補を利用者に一括で判断してもらい、決まったものを消す

- Purpose: 実装詳細をなぞるだけのテスト、作らないと決めた hunks のテスト、根拠にしなかった重複テストを利用者の判断で消す
- Specification: docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S4
- May change: src/service/status.rs, src/cli/status.rs, src/cli/ref_guard.rs, src/service/source_pair.rs, src/service/output.rs, src/service/types.rs のテスト部分, tests/cli_status.rs, tests/cli_exit_codes.rs, tests/cli_error_handling.rs, docs/testing/status-test-cleanup.md
- Done when: 利用者が削除候補の一覧（src/service/status.rs の test_hash_comparison で始まる五件、tests/cli_error_handling.rs の test_ref_with_left_equal_fails_on_ssh と test_ref_with_right_equal_fails_on_ssh、hunks を扱う src/service/output.rs の test_format_status_text_with_hunks と src/service/types.rs の test_status_output_with_hunks、および S2 から S4 で根拠にしなかった重複テスト）を一度に見て削除するものを決め、決まったものだけが消え、判断の結果が docs/testing/status-test-cleanup.md に記録されている
- Shown by: external — 削除候補の一覧（src/service/output.rs と src/service/types.rs の候補には同じ分岐を確かめて残るテストの名前と、変異テストの裏付けがないこと）を docs/testing/status-test-cleanup.md に書いてコミットして作業を返し、利用者の返答（消すものの一覧）が同じ文書に書き足されてから削除し、削除後に `cargo nextest run --all-features` が通ることを確認する
- Left to the implementer: 一覧の示し方
- Stop and hand back if: 利用者が一覧のうち一部の判断を保留した（保留分は消さずに残して報告する）

### S6: 整理後の変異テストで見逃しを比べ、全て決着させる

- Purpose: 削除で見逃しが増えていないことを確かめ、status の入口から呼ばれる関数に残った見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S5
- May change: tests/contract/ の status 用モジュール, tests/contract/status_results.rs, tests/contract.rs, tests/common/mod.rs, .kotowari/mutants-equivalents.yaml, S5 で挙げた各ファイルのテスト部分, docs/testing/status-test-cleanup.md
- Done when: S1 と同じ四ファイルの変異テストの見逃しが S1 の見逃しの部分集合になっている（増えた見逃しを生んだ削除は戻してある）。決着の対象の見逃しの全てに、足したテスト、同等変異の一覧への理由付きの登録（別の文脈のエージェントが落とすテストを書けなかった試みつき）、既存の FLAG-cli-00N の範囲としての記録、新しい FLAG の候補としての報告のどれかが docs/testing/status-test-cleanup.md に記録されている。決着の対象でない見逃しは一覧として記録されている。同じ文書に REQ-cli-006 から 008 と REQ-cli-027 から 037 の verification とそれが要件の性質に合う理由が一行ずつあり、合わないと判断したものは IR を直さず候補として挙がっている
- Shown by: external — `scripts/mutants.sh src/service/status.rs src/cli/status.rs src/cli/ref_guard.rs src/service/source_pair.rs src/tree.rs` の出力と S1 の記録を突き合わせた結果を docs/testing/status-test-cleanup.md の「整理後」の節に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: none
- Stop and hand back if: 見逃しが不具合の疑いを示した、または verification の見直しの候補がある（どちらも文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S6 は完了しない。実装と IR は直さない）、負荷の下で落ちるテスト（tui_merge や agent_ssh）による見かけの検知が集計に混ざり、比較が成り立たない

### S7: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件と例に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/cli/status.md#REQ-cli-027, docs/ir/cli/status-targets.md#REQ-cli-037, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S6
- May change: none
- Done when: REQ-cli-027 から REQ-cli-037 と EX-cli-062 の全てで `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in $(seq -f 'REQ-cli-%03g' 27 37) EX-cli-062; do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
