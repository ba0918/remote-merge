# Plan: CLI diff の --ref の参照先と競合のテストを要件の根拠に整理する

## Goal

CLI diff の --ref の参照先と競合の要件について、印付きの根拠テストが公開された入口から要件を十分に確かめ、純粋関数の単体テストの扱いは利用者の一括の判断で決まり、この範囲の変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/cli/reference.md#REQ-cli-062`、`#REQ-cli-063`、`#REQ-cli-064`、`#REQ-cli-066`
- `docs/ir/cli/conflicts.md#REQ-cli-065`、`#REQ-cli-016`（既存。例 EX-cli-031・EX-cli-032 には印付きの契約テストがあるが、要件そのものの印はまだない）
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`、`#REQ-testing-013`（いずれも review）

判断の出典は `docs/decision/records/2026-09-30-adopt-diff-ref.md`（取り込み、FLAG-cli-058 から 070、テストの仕分けの件数）、`docs/decision/records/2026-09-28-mutation-scope.md`、`docs/decision/records/2026-09-29-mutation-rerun-and-load.md`。要件と例の本文と印付きのテストは `kotowari query REQ-cli-062` のように読む。手本は status の整理の記録 `docs/testing/status-test-cleanup.md`（純粋関数の単体テストを入口を通すテストに置き換え、削除を利用者の一括の判断に任せた流れ）と、`tests/contract/diff_output_cli.rs` の `launch_diff`（実行ファイルを試験 SSH サーバに対して起動する組み方）、三者比較の構成の手本（`tests/common/mod.rs` の `CliEnv::new_3way` は local を参照先、develop を左、staging を右に置く構成で、`tests/cli_diff_general.rs` の `test_diff_with_ref` と `tests/contract/status_targets.rs` の `three_way` が `--left develop --right staging --ref local` で使う。サーバ名の参照先には `TestDirs::new_3way` と `gen_config` の参照先の引数が使える）、`tests/contract/cli_results.rs` の EX-cli-031・032 のテスト（execute_diff を ref_server 付きで呼ぶ組み方）。

## Approach and why

取り込みの仕分けで要件の根拠とした 39 件は次のとおり。このうち tests/contract/cli_results.rs の 2 件（`three_way_diff_reports_conflicting_edits_to_the_same_line` と `identical_changes_on_both_sides_have_no_three_way_conflict`。EX-cli-031・032 の印付き）は既に `.kotowari/config.yaml` の `tests.files`（`tests/contract/**/*.rs`、`tests/cli_diff.rs` など）の中にあり、残りの 37 件（置き換え元）はその外にある。

- src/service/diff.rs の 6 件: `test_ref_content_produces_ref_hunks`、`test_ref_content_same_as_left_produces_empty_ref_hunks`、`test_ref_content_none_produces_none_ref_hunks`、`test_no_ref_backward_compat`、`test_conflict_count_with_ref`、`test_conflict_count_without_ref`
- src/diff/conflict.rs の detect_conflicts の振る舞いの 13 件: `test_basic_conflict`、`test_one_sided_change_no_conflict`、`test_both_same_change_no_conflict`、`test_multi_line_conflict`、`test_separate_conflicts`、`test_delete_vs_modify_conflict`、`test_both_delete_same_line_no_conflict`、`test_no_ref_returns_empty`、`test_all_identical_no_conflicts`、`test_empty_files`、`test_insert_conflict_both_insert_different`、`test_ref_empty_both_add_different`、`test_overlapping_range_conflict`
- src/service/output.rs の 8 件: `test_format_diff_text_with_ref_hunks`、`test_format_diff_text_no_ref_backward_compat`、`test_format_diff_text_with_conflicts`、`test_format_diff_text_no_conflicts`、`test_format_multi_diff_text_with_conflicts`、`test_format_multi_diff_text_no_conflicts`、`test_diff_output_conflict_count_zero_omitted_in_json`、`test_diff_output_conflict_count_nonzero_in_json`
- src/cli/ref_guard.rs の 4 件: `ref_same_as_left_returns_none`、`ref_same_as_right_returns_none`、`ref_different_returns_some`、`ref_none_returns_none`（status と merge の整理でも根拠にした）
- src/service/source_pair.rs の 5 件: `test_resolve_ref_source_remote`、`test_resolve_ref_source_local`、`test_resolve_ref_source_nonexistent`、`test_resolve_ref_source_none`、`test_resolve_ref_source_same_as_left`（status の整理でも根拠にした）
- tests/cli_diff_general.rs の `test_diff_with_ref`

FLAG の挙動のテスト 3 件（src/diff/conflict.rs の `test_conflict_info_serialization`〈FLAG-cli-058〉、src/service/diff.rs の `test_max_lines_applied_independently_to_ref_hunks`〈FLAG-cli-061〉と `test_conflict_count_binary_is_zero`〈FLAG-cli-059〉）と、「残す」とした 15 件（src/diff/conflict.rs の TUI 用の行の判定 9 件と `compute_conflict_if_complete` の 6 件）は置き換えず、削除候補にもしない。

status の整理と同じく、要件ごとに公開された入口（実行ファイルの `diff`、または `execute_diff` の関数呼び出し）から振る舞いを確かめる根拠テストを `tests/contract/` に置いて印を付ける。置き換え元の 37 件は移し元に残し、削除候補として利用者に一括で判断してもらう。製品コードは変えない。

根拠テストが確かめるのは IR の要件の文が定める部分だけにする。FLAG-cli-058 から 070 に当たる部分は確かめない（例: "conflict_regions" の要素の形は FLAG-cli-058、バイナリの競合と参照先との差は FLAG-cli-059・064、参照先にないファイルの競合は FLAG-cli-060、参照先との差の打ち切りは FLAG-cli-061、参照先を読めない理由は FLAG-cli-062、symlink や機密ファイルなど参照先と比べない項目は FLAG-cli-063、左右が同じファイルは FLAG-cli-065、片側にないファイルの競合は FLAG-cli-067、参照先にだけあるファイルは FLAG-cli-066、参照先に接続できないときは FLAG-cli-068、参照先の機密ファイルと root_dir の外を指す symlink は FLAG-cli-070）。根拠テストの構成では、参照先に機密ファイルや symlink を置かない。REQ-cli-063 の「参照先のファイルを読めないときは "ref" だけ」は、参照先にそのパスがない場合で確かめる（読めない理由の違いは FLAG-cli-062）。REQ-cli-065 の競合は、用語「競合」のうち FLAG に当たらないテキストのファイルの場合で確かめる。

要件ごとの入口の目安は次のとおり。標準エラーの警告、main.rs が出すエラーと終了コード、テキストの出力は関数呼び出しでは観測できないため、実行ファイルを起動して確かめる。

- REQ-cli-062: 実行ファイルで、設定のサーバ名と "local" の --ref が三者比較になること（JSON に "ref" が出る）と、設定にないサーバ名が "Server '<名前>' not found in config" と終了コード 2 になること。サーバ名の参照先は左 local・右 develop・参照先 staging の構成で、"local" の参照先は左 develop・右 staging・参照先 local の構成で確かめる（`launch_diff` は左 local・右 develop に固定されており、そのまま `--ref local` を渡すと左と同じ参照先として REQ-cli-066 の警告の経路に入る）
- REQ-cli-063: JSON の "ref"（"label" と "root"）、左から参照先への差の "ref_hunks"、左と参照先が同じときの空の配列、参照先にそのパスがないときに "ref" だけで "ref_hunks" がないこと、--ref がないときにどちらもないこと
- REQ-cli-064: テキストで、参照先との差が空でないとき左右の差の後に "--- ref:<参照先>:<パス> (reference diff vs left)" と hunk が出て、空のときは出ないこと
- REQ-cli-065 と REQ-cli-016: 競合があるテキストのファイルで JSON の "conflict_count" と "conflict_regions" が出て、テキストに "Conflicts: N region(s) where both sides changed the same lines differently" が出ること、複数のファイルのテキストの末尾に "N conflict(s) detected across files" が出ること、競合がなければどれも出ないこと。「競合がない」場合は、左右に差があって出力に出るファイルで確かめる。一つは左右が参照先に対して別々の行を変えた場合、もう一つは左右が同じ行を同じ内容に変え、ほかの行で左右に差がある場合（競合の意味は `docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A4`）。EX-cli-032 のテストは左右が同じ内容でファイルが出力に出ない（FLAG-cli-065）ため、REQ-cli-016 の根拠に数えない。"conflict_regions" の要素の中身は FLAG-cli-058 のため、ファイルが一つのときに "N conflict(s) detected across files" が出るかは IR が定めないため、どちらも確かめない
- REQ-cli-066: 参照先が左と同じとき・右と同じときのそれぞれで警告の文言が標準エラーに出て、参照先なしで比較が続く（JSON に "ref" が出ない）こと

変異テストは既存の `scripts/mutants.sh` からだけ実行し、対象は参照先と競合の規則を持つ関数に絞る。整理の前後で同じコマンドを使う。

```sh
scripts/mutants.sh \
  --re ' in (build_diff_output|detect_conflicts|extract_changes|merge_overlapping_regions|merge_ranges|validate_ref_side|resolve_ref_source)$' \
  --re 'replace (build_diff_output|detect_conflicts|extract_changes|merge_overlapping_regions|merge_ranges|validate_ref_side|resolve_ref_source) -> ' \
  src/service/diff.rs src/diff/conflict.rs src/cli/ref_guard.rs src/service/source_pair.rs
```

計画を書いた時点で、同じ `--re` を付けた `cargo mutants --list --all-features --file src/service/diff.rs --file src/diff/conflict.rs --file src/cli/ref_guard.rs --file src/service/source_pair.rs` は 93 件（build_diff_output 8、detect_conflicts 30、extract_changes 36、merge_overlapping_regions 4、merge_ranges 8、validate_ref_side 4、resolve_ref_source 3）だった。`resolve_ref_source` は status の整理で見逃しがなかったが、その根拠にした `test_resolve_ref_source_` 系を削除候補にするため、削除の前後の比較に含める。`src/cli/diff.rs` の参照先に固有の行（参照先の解決、左右がバイナリのとき "ref_hunks" を出さない所、参照先の読み込み）には変異が作られないため対象にしない。バイナリの分岐の条件（`is_binary(&left_bytes) || is_binary(&right_bytes)` など）の変異は左右がバイナリのときの扱いで、FLAG-cli-059・064 と diff の差分の出し方の範囲のため対象にしない。`src/service/output.rs` は status の整理と同じく diff・merge・sync の出力が大半を占めるため変異テストの対象にしない。`tests/cli_diff_general.rs` は `scripts/mutants.sh` が変異ごとに流すテスト（`--lib`、`--test contract`、`--test cli_diff`）に入らないため、`test_diff_with_ref` の削除は変異テストに現れない。構造体のフィールドを消す変異はこの絞り込みでは混ざらなかったが、混ざったら件数と見逃しから分けて記録する（`docs/decision/records/2026-09-28-mutation-scope.md#A3`）。回す前に同じ `cargo mutants --list` を実行して件数を確かめ、記録に残す。

見逃しのうち、`build_diff_output` の参照先と競合を扱う部分、`detect_conflicts`・`extract_changes`・`merge_overlapping_regions`・`merge_ranges`・`validate_ref_side`・`resolve_ref_source` のものを決着の対象にする。FLAG-cli-058 から 070 や既存の FLAG に当たる見逃しはその FLAG の範囲として、`build_diff_output` の参照先と競合以外の部分（左右の差の組み立てと打ち切り）は diff の差分の出し方の整理の範囲として、引き継ぎ先を書いて記録だけにする（`docs/decision/records/2026-09-29-mutation-rerun-and-load.md#A3`）。決着の対象は、テストの追加、同等変異の登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させる。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。見逃しを落とすためにテストを足したら、同じ決定記録の A2 に従い、前の実行で caught にならなかった変異に絞って回し直してから記録する。変異の一時的な書き換えで確かめるときは本体の作業ツリーで行い、確かめた後に `git diff --stat src/` が空に戻ることを確かめる。

## Scope of change

- `tests/contract.rs`（新しいモジュールの登録だけ）
- `tests/contract/` の diff の参照先用の新しいモジュールと、`tests/contract/cli_results.rs` の EX-cli-031・032 のテストの印の行（REQ-cli-016 の印を足すときだけ）
- `src/service/diff.rs`、`src/diff/conflict.rs`、`src/service/output.rs`、`src/cli/ref_guard.rs`、`src/service/source_pair.rs` の `#[cfg(test)]` のテスト部分と `tests/cli_diff_general.rs`（利用者が削除を決めたテストの削除だけ。製品コードは変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/diff-ref-test-cleanup.md`（新規。整理前後の変異テスト、要件ごとの根拠テスト、削除候補と利用者の判断、見逃しの決着、verification の見直しの記録）

## Step order and prerequisites

S1 で整理前の変異テストを記録し、S2 で根拠テストを置き、S3 で削除を利用者に判断してもらい、S4 で整理後の変異テストを比べて見逃しを決着させ、S5 で全体を確かめる。

S3 と S4 には利用者の判断を待つ区切りがある。S3 は削除候補の一覧を、S4 は新しい FLAG の候補や verification の見直しの候補があるときだけ、`docs/testing/diff-ref-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/diff-ref`。取り込みの記録は同じブランチにコミット済み（d90b185）で、この計画も承認の後に同じブランチにコミットする。main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-testing-012（review）, REQ-testing-013（review） | — |
| S2 | REQ-cli-062, REQ-cli-063, REQ-cli-064, REQ-cli-065, REQ-cli-066, REQ-cli-016 | — |
| S3 | REQ-testing-012（review） | — |
| S4 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S5 | REQ-cli-062, REQ-cli-063, REQ-cli-064, REQ-cli-065, REQ-cli-066, REQ-cli-016 | — |

## Left to the implementer

- 新しいモジュールの名前と分け方、補助の組み立て方（`launch_diff` と同じ形で参照先の "staging" を持つ構成を書くか、既存の補助を広げるか）
- 一つのテストで複数の場合を確かめるか分けるか
- 記録の見出しと表の形

## Stop conditions

- 根拠テストを書くために、IR が定めていない文言・順序・終了コードの値や、FLAG-cli-058 から 070 に当たる挙動を固定する必要が生じた
- 参照先を加えた三者比較の構成（参照先が試験 SSH サーバか "local"）が、実行ファイルの起動でも関数呼び出しでも作れない
- 既存のテストの中身や期待を書き換える必要が生じた
- 変異テストがメモリ上限で失敗し続ける

## Test command

```sh
cargo nextest run --all-features
```

cargo のテストとビルドと `cargo mutants --list` は systemd-run のメモリ上限（MemoryMax=40%、MemorySwapMax=0）と CPUWeight=idle・Nice=19 の中で実行する。形は `scripts/mutants.sh` の systemd-run の呼び出しにそろえる（`systemd-run --user --wait --pipe --quiet --same-dir --setenv=PATH="$PATH"` に RUSTUP_TOOLCHAIN・RUSTUP_HOME・CARGO_HOME・MISE_GLOBAL_CONFIG_FILE のうち値のあるものを `--setenv` で渡し、`-p MemoryMax=40% -p MemorySwapMax=0 -p CPUWeight=idle -p Nice=19` を付ける。認証情報を含む変数は渡さない）。変異テストは `scripts/mutants.sh` から実行し、既定の並列数（2）より上げない。コミットは `nice -n 19 git commit` で行う（pre-commit のフックが fmt・clippy・全テストを流す）。`--no-verify` は使わない。

## Out of scope

- FLAG-cli-058 から 070 の決着と修正
- status・merge・sync の --ref、TUI の三者比較
- 製品コードの変更、既存のテストの中身と期待の書き換え
- 面の検査（`surface`）の導入

## Steps

### S1: 整理前の変異テストの見逃しを記録する

- Purpose: 根拠テストの審査と削除の前後の比較の基準にするため、整理前の見逃しを残す
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-013
- Prerequisites: この計画が承認されてブランチ adopt/diff-ref にコミットされ（取り込みの d90b185 の後）、作業ツリーに変更がない
- May change: docs/testing/diff-ref-test-cleanup.md
- Done when: Approach and why の `cargo mutants --list` の件数と、同じ絞り込みの `scripts/mutants.sh` の全体の集計、関数ごとの内訳、見逃し一件ずつの位置と変異の内容と決着の対象かどうかが、実行したコミットとともに docs/testing/diff-ref-test-cleanup.md に書かれている
- Shown by: artifact — Approach and why の `scripts/mutants.sh` のコマンドの出力を docs/testing/diff-ref-test-cleanup.md の「整理前」の節に集計と見逃しの表として書く
- Left to the implementer: 記録の見出しと表の形
- Stop and hand back if: 実行がメモリ上限で失敗し続ける、`cargo mutants --list` の件数が 93 件と違う

### S2: 参照先と競合の要件の根拠テストを入口を通す形で置いて印を付ける

- Purpose: REQ-cli-062 から 066 と REQ-cli-016 に、公開された入口から要件を十分に確かめる印付きの根拠テストを置く
- Specification: docs/ir/cli/reference.md#REQ-cli-062, docs/ir/cli/reference.md#REQ-cli-063, docs/ir/cli/reference.md#REQ-cli-064, docs/ir/cli/conflicts.md#REQ-cli-065, docs/ir/cli/reference.md#REQ-cli-066, docs/ir/cli/conflicts.md#REQ-cli-016
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の diff の参照先用の新しいモジュール, tests/contract/cli_results.rs の印の行, docs/testing/diff-ref-test-cleanup.md
- Done when: 六つの要件それぞれで `kotowari query` の tests が空でなく、印付きのテストを合わせて Approach and why の要件ごとの目安を全て確かめる。新しく書いたテストは書いた時点の実装に対して通る。どのテストをどの要件の根拠にしたか、Approach and why に名前を挙げた置き換え元の 37 件のそれぞれを、どの根拠テストで置き換えたかが docs/testing/diff-ref-test-cleanup.md にある
- Shown by: test — Approach and why に名前を挙げた要件の根拠の 39 件を審査し、要件ごとに入口を通す根拠テストを書いて印を付け、`cargo nextest run --all-features` で通ることを確かめる
- Left to the implementer: モジュールの名前と分け方、補助の組み立て方、一つのテストで複数の場合を確かめるか
- Stop and hand back if: 要件を確かめるのに FLAG-cli-058 から 070 に当たる挙動を固定する必要が生じた、参照先を加えた三者比較の構成が作れない

### S3: 削除候補を利用者に一括で判断してもらい、決まったものを消す

- Purpose: 入口を通す根拠テストに置き換えた単体テストと重複するテストを、利用者の判断で消すか残す
- Specification: docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S2
- May change: src/service/diff.rs, src/diff/conflict.rs, src/service/output.rs, src/cli/ref_guard.rs, src/service/source_pair.rs のテスト部分, tests/cli_diff_general.rs, docs/testing/diff-ref-test-cleanup.md
- Done when: 利用者が削除候補の一覧（S2 で置き換えた元のテストと、代わりの根拠テストの名前）を一度に見て削除するものを決め、決まったものだけが消え、判断の結果が docs/testing/diff-ref-test-cleanup.md に記録されている。src/cli/ref_guard.rs の候補は status と merge の整理で、src/service/source_pair.rs の候補は status の整理でも根拠にしたテストであることが一覧に書かれている。src/service/output.rs と tests/cli_diff_general.rs の候補には変異テストの裏付けがないことが書かれている
- Shown by: external — 削除候補の一覧を docs/testing/diff-ref-test-cleanup.md に書いてコミットして作業を返し、利用者の返答（消すものの一覧）が同じ文書に書き足されてから削除し、削除後に `cargo nextest run --all-features` が通ることを確かめる
- Left to the implementer: 一覧の示し方
- Stop and hand back if: 利用者が一覧のうち一部の判断を保留した（保留分は消さずに残して報告する）

### S4: 整理後の変異テストで見逃しを比べ、全て決着させる

- Purpose: 削除で見逃しが増えていないことを確かめ、決着の対象の見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S3
- May change: tests/contract/ の diff の参照先用のモジュール, .kotowari/mutants-equivalents.yaml, docs/testing/diff-ref-test-cleanup.md, S3 で消したテストのあったファイルのテスト部分（見逃しを増やした削除を戻すことだけ）
- Done when: S1 と同じコマンドの見逃しが S1 の見逃しの部分集合になっている（増えた見逃しを生んだ削除は戻してある。どの削除が効いたかは、増えた見逃しの変異を一時的に書き入れて、消したテストを戻した状態で落ちるかで確かめる。cargo-mutants のログには最初に落ちたテストしか残らないため。戻したテストとその理由は記録に書き、利用者への報告に含める）。決着の対象の見逃しの全てに、足したテスト、同等変異の理由付きの登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれかが記録されている。決着の対象でない見逃しは引き継ぎ先とともに一覧として記録されている。REQ-cli-062 から 066 と REQ-cli-016 の verification とそれが要件の性質に合う理由が一行ずつあり、合わないと判断したものは IR を直さず候補として挙がっている
- Shown by: external — Approach and why の `scripts/mutants.sh` のコマンドの出力と S1 の記録を突き合わせた結果を docs/testing/diff-ref-test-cleanup.md の「整理後」の節に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: none
- Stop and hand back if: 見逃しが不具合の疑いを示した、または verification の見直しの候補がある（どちらも文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S4 は完了しない。実装と IR は直さない）

### S5: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/cli/reference.md#REQ-cli-062, docs/ir/cli/reference.md#REQ-cli-063, docs/ir/cli/reference.md#REQ-cli-064, docs/ir/cli/conflicts.md#REQ-cli-065, docs/ir/cli/reference.md#REQ-cli-066, docs/ir/cli/conflicts.md#REQ-cli-016
- Prerequisites: S4
- May change: none
- Done when: REQ-cli-062 から 066 と REQ-cli-016 の全てで `kotowari query` の tests が空でなく、`kotowari check --format json` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in REQ-cli-062 REQ-cli-063 REQ-cli-064 REQ-cli-065 REQ-cli-066 REQ-cli-016; do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari check --format json`、Test command の形での `cargo fmt --all --check`・`cargo clippy --all-targets --all-features -- -D warnings`・`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
