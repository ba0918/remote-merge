# Plan: merge の指定・確認・出力のテストを要件の根拠に整理する

## Goal

merge の指定・確認・出力の取り込みで加えた各要件に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、実装詳細をなぞるだけのテストは利用者の判断で消され、その過程が変異テストの結果で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/cli/merge.md#REQ-cli-046` から `#REQ-cli-051`（REQ-cli-046 の `- definition:` から決定表 TBL-cli-008 を辿れる）
- merge 側の根拠を足す既存要件: `docs/ir/cli/results.md#REQ-cli-018`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`、`#REQ-testing-013`（いずれも review）

判断の出典は `docs/decision/records/2026-09-28-adopt-merge-cli.md`（取り込み、FLAG、merge を 4 回に分けて取り込むこと、テストの仕分けの件数と範囲外にしたもの）と `docs/decision/records/2026-09-27-test-method-selection.md`（手法の選び方、変異テストとメモリ上限）。要件の本文は `kotowari query REQ-cli-0nn` で読む。同じ手順を sync で行った記録 `docs/testing/sync-test-cleanup.md` が記録の書き方の手本になる。

## Approach and why

最初に整理前の変異テストの見逃しを記録する。見逃しは根拠テストが要件を十分に確かめていないことの機械的な手掛かりになり、削除の前後の比較の基準にもなる。変異テストは既存の `scripts/mutants.sh`（メモリ上限付き）からだけ実行し、対象は merge の入口の規則を持つ `src/cli/merge.rs`、`src/service/merge.rs`、`src/cli/ref_guard.rs` の三つに絞る。`src/service/output.rs` は status・diff・sync・rollback の出力が大半を占め、ファイル単位でしか対象にできない変異テストの実行時間が大きく増えるため対象にしない。merge のテキストと JSON の整形は S4 と S3 の入口を通すテストで確かめる。TBL-cli-008 の行のうち、--left と --right が同じ行と設定にないサーバ名の行は `src/service/source_pair.rs`（sync の整理で変異テストの対象にした）、--format の行は `src/service/output.rs` の `OutputFormat::parse` が決めるため、この計画の変異テストの結果はこれらの行を裏付けない。S1 と S7 の記録にその旨を書く。

見逃しのうち決着の対象にするのは、merge の入口の規則を持つ関数のものに限る: `src/cli/merge.rs` の `validate_merge_args`（--hunks の分岐を除く）・`run_merge`・`execute_merge`・`print_merge_result`、`src/service/merge.rs` の `plan_merge`・`build_merge_output`・`merge_exit_code`・`has_three_way_conflict`・`check_r2r_guard`、`src/cli/ref_guard.rs` の `validate_ref_side`。ただし `execute_merge` の中でも、左右の比較・中身の読み比べ・対象の絞り込み・削除計画（`fetch_trees_and_statuses_for_merge` の呼び出しから `skip_symlink_deletions` の呼び出しまでの区間）と、ファイルごとの書き込みと削除の実行（`execute_single_merge` と `execute_deletions` の呼び出し）は merge の後の回（書き込みの中身、symlink と削除）で扱うため、そこに入った見逃しは記録だけする。`execute_merge` の機密ファイルの件数の通知（FLAG-cli-022）、三者の中身がそろわないときの分岐（FLAG-cli-023）、dry-run で競合を確かめない条件（FLAG-cli-024）に入った見逃しは、その FLAG の範囲として記録する。`validate_merge_args` の --hunks の分岐、`run_hunk_merge`、`fetch_trees_and_statuses_for_merge`、`fetch_partial_trees`、`merge_partial_nodes`・`merge_partial_node`・`merge_file_node`、`find_symlink_target`、`determine_merge_action` の見逃しも記録だけする。

審査は要件ごとに進める。各要件について、既存テストのうち要件の文と決定表の行を十分に確かめるものを探し、見つかれば同じ確かめ方で入口を通す根拠テストを `tests/contract/` の下の merge 用の新しいモジュールに新しく書いて印を付ける。**元のテスト（`src/` の単体テスト、`tests/cli_merge.rs`、`tests/cli_error_handling.rs`）は消さずに残す。** 新しく書くのは、`.kotowari/config.yaml` の `tests.files` が `tests/contract/**/*.rs` を対象にしており、その外のテストは印を付けても根拠に数えられないため。`tests.files` は広げない。既存テストが要件を十分に確かめない場合は、根拠テストを新しく書く。`tests/contract/cli_results.rs` の merge のテストは印の行に ID を足してよいが、テストの本体は書き換えない（本体を変えると既存の例の根拠が変わるため）。

根拠テストは merge の公開された入口を通す。関数呼び出しなら `remote_merge::cli::merge::execute_merge` に `RuntimeTargets::with_local` でサーバをローカルのディレクトリに差し替えて渡す（`tests/contract/cli_results.rs` の `sync_fixture` の形。`first` を書き込み先、`second` を参照先に使える）。merge は確認のプロンプトを出さず標準入力を読まないため、関数呼び出しで `force: false` を渡してよい。標準エラーへの出力（REQ-cli-050 の警告）は関数呼び出しでは観測できない。テキスト出力（REQ-cli-049）と終了コード 2 のエラー（REQ-cli-046、REQ-cli-018）は、非公開の `print_merge_result` による出力の振り分けと `src/main.rs` の終了コードの決め方を含めて確かめるため、これらも実行ファイルで確かめる。書き込み先に SSH が要る実行ファイルのテストは、`tests/common/mod.rs` の `CliEnv`（SSH の fixture、`test-utils` の feature が要る）で動かす。TBL-cli-008 のエラーと REQ-cli-018 の JSON のエラーは接続より前に起きるため SSH の fixture を使わない。このとき `tests/common/mod.rs` の `remote_merge_cmd` は使わない（`common` は `test-utils` の feature があるときだけ読み込まれ、`remote_merge_cmd` は実際の HOME を渡すため、利用者の `~/.config/remote-merge/config.toml` が `--config` の設定に合わさって結果が開発者の環境に左右される）。代わりに新しい contract モジュールの中で `Command::new(env!("CARGO_BIN_EXE_remote-merge"))` に `env_clear` をかけ、一時的なディレクトリを HOME・XDG_CONFIG_HOME・XDG_DATA_HOME に渡し、一時的な設定ファイルを `--config` で渡す（`tests/contract/diagnostics.rs` が HOME を一時的なディレクトリにする形）。`tests/contract.rs` は `common` を `test-utils` の feature があるときだけ読み込むため、関数呼び出しのテストとこの実行ファイルのテストの補助関数は新しい contract モジュール（または feature で囲まない隣のモジュール）に置き、`CliEnv` を使うテストとその補助関数だけを `test-utils` で囲む。

`CliEnv` の設定ファイルには `[backup]` の節がなく、バックアップは既定で有効になり、fixture の XDG_DATA_HOME の下に書かれる。そのため書き込みの行は既定で " (backup: …)" を伴う。バックアップのない行を確かめるときは `env.config_path` の設定ファイルに `[backup]` と `enabled = false` を書き足す。`CliEnv::new_3way` の説明のコメントは local を参照先、develop を左、staging を右とするが、その割り当てはリモート間の merge になり --force か --dry-run なしでは止められ（FLAG-cli-020）、--force か --dry-run を付けると三者の競合を確かめない（FLAG-cli-021、FLAG-cli-024）。参照先を使う実行ファイルのテストは `--left local --right develop --ref staging` の割り当てを使い、--force も --dry-run も付けない。

新しく書くテストは FLAG-cli-016 から 024 の挙動を確かめない。具体的には、種類の違いなどのスキップだけのときの終了コード（FLAG-cli-016）、読めないファイルを含む dry-run の終了コード（FLAG-cli-017）、機密ファイルのスキップの reason の文言（FLAG-cli-018）、スキップのテキストの行の形（FLAG-cli-019）、リモート間の merge を止めたときの出力と終了コード（FLAG-cli-020）、確認のプロンプトの有無と --force の働き（FLAG-cli-021）、機密ファイルの件数の通知（FLAG-cli-022）、三者の中身がそろわないファイルの扱い（FLAG-cli-023）、dry-run での競合（FLAG-cli-024）を検証の対象に含めない。スキップが出る構成で JSON の skipped を確かめるときは、path と reason の項目があることだけを確かめ、reason の値は確かめない。テキスト出力の根拠テストはスキップの出ない構成で作る。

削除は利用者が一括で判断する。削除の前後で同じ対象に変異テストを回し、見逃しが増えたら削除を戻す。最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG（FLAG-cli-016 から 024）の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。別の文脈を立てられないときは登録せず候補として手渡す。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の merge 用の新しいモジュール
- `tests/contract/cli_results.rs`（merge のテストの印の行に ID を足すことだけ）
- `tests/common/mod.rs`（新しいテストが使う補助関数の追加だけ）
- `src/cli/merge.rs`、`src/service/merge.rs`、`src/cli/ref_guard.rs`、`src/service/output.rs` の `#[cfg(test)]` のテスト部分と `tests/cli_error_handling.rs`（利用者が削除を決めたテストの削除だけ。製品コードは変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/merge-cli-test-cleanup.md`（新規。変異テストの結果と見逃しの決着、根拠にしたテスト、削除の判断の記録）

## Step order and prerequisites

S1 の整理前の結果が S7 の比較の基準になるため最初に取る。S2 から S5 は要件ごとの審査で互いに独立しているが、同じ `tests/contract.rs` と新しいモジュールを書き換えるため順に行う。S6 の削除は S2 から S5 で根拠が決まってから行う（根拠の確かめ方の手本にしたテストを誤って消さないため）。S7 は S6 の後の状態で変異テストを回す。S8 で全体を確かめる。

S6 と S7 には利用者の判断を待つ区切りがある。S6 は削除候補の一覧を `docs/testing/merge-cli-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の返答（消すもの）がその文書に書き足されてから削除を再開する。S7 は新しい FLAG の候補や verification の見直しの候補があれば、同じ文書に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/merge`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-testing-012（review）, REQ-testing-013（review） | — |
| S2 | REQ-cli-046, REQ-cli-018 | — |
| S3 | REQ-cli-047, REQ-cli-048 | — |
| S4 | REQ-cli-049, REQ-cli-050 | — |
| S5 | REQ-cli-051 | — |
| S6 | REQ-testing-012（review） | — |
| S7 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S8 | 上記すべて | — |

## Left to the implementer

- `tests/contract/` の下の merge 用モジュールの分け方と名前
- テスト用の補助関数の置き場所（新しい contract モジュールに書くか、`tests/common/mod.rs` に足すか）
- REQ-cli-047・048・051 の根拠を関数呼び出しと実行ファイルのどちらにするか（要件の文を全て確かめる方を選ぶ。REQ-cli-046・018・049・050 は Approach and why のとおり実行ファイルで確かめる）

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- FLAG-cli-016 から FLAG-cli-024 の挙動を確かめるテスト（`tests/cli_merge.rs` の `test_merge_sensitive_file_requires_force`・`test_merge_remote_to_remote_requires_force`、`src/service/output.rs` の `test_format_merge_outcome_text_r2r_blocked`・`test_format_merge_outcome_json_r2r_blocked`・`test_format_merge_outcome_json_r2r_blocked_no_root_path`・`test_format_merge_text_sensitive_skip_reason`）を移すか消すか書き換える必要が生じた
- 要件の verification が要件の性質に合わないと判断した（IR は直さず、S7 で候補として手渡す）
- 要件を確かめるために製品コードの挙動や公開範囲を変える必要が生じた
- 変異テストの見逃しや新しいテストが不具合の疑いを示した（FLAG として記録するかは利用者が決めるため、その見逃しは未決着のまま報告する）
- 変異テストが WSL のメモリを使い切りそうな兆候（スクリプトの上限に当たって失敗し続ける）があり、並列数を下げても解消しない

## Test command

```sh
cargo nextest run --all-features
```

変異テストは `scripts/mutants.sh` から実行し、cargo-mutants を直接実行しない。並列数を 2 より上げない。

## Out of scope

- FLAG-cli-016 から FLAG-cli-024 の決着と、その挙動の修正
- 既存の印付きテスト（REQ-cli-002・003・004・011・017・019 の例に付いたもの）の本体の見直し
- merge の後の回で扱うテスト（複数ファイル・ディレクトリ・バイナリ・重複パス・hunk・symlink・削除・権限・バックアップのテスト。`tests/cli_merge.rs` と `tests/contract/merge_paths.rs` と `tests/contract/cli_results.rs` にあるもの）の移動と印付け
- `src/service/merge_flow.rs` の `check_source_exists` のテスト 12 件（merge の入口から届かない内部の振る舞いとして残す）
- 面の検査（`surface`）の導入

## Steps

### S1: 整理前の変異テストの見逃しを記録する

- Purpose: 審査と削除の比較の基準にするため、整理前の見逃しを残す
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-013
- Prerequisites: none
- May change: docs/testing/merge-cli-test-cleanup.md
- Done when: `src/cli/merge.rs`、`src/service/merge.rs`、`src/cli/ref_guard.rs` を一回の実行にまとめた変異テストの全体の集計（caught・survived・timeout・unviable）、ファイルごとの内訳、見逃し一件ずつの位置と変異の内容と決着の対象かどうか（Approach and why の区別）が、実行したコミットとともに `docs/testing/merge-cli-test-cleanup.md` に書かれている。検知のうち、変異と関係のないテスト（tui_merge や agent_ssh のように負荷の下で落ちるもの）だけで検知されたものが名前とともに記録されている
- Shown by: artifact — `scripts/mutants.sh src/cli/merge.rs src/service/merge.rs src/cli/ref_guard.rs` をバックグラウンドで実行して完了を待ち、その出力を docs/testing/merge-cli-test-cleanup.md の「整理前」の節に集計と見逃しの表として書く
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 実行がメモリ上限で失敗し続ける、負荷の下で落ちるテストによる見かけの検知が多く、整理前と整理後の比較が成り立たない

### S2: 指定のエラーと JSON のエラーの根拠テストを書いて印を付ける

- Purpose: merge の指定の誤りが TBL-cli-008 のエラーで止まることと、--format json の指定では失敗も JSON で返ることに根拠テストを置く
- Specification: docs/ir/cli/merge.md#REQ-cli-046, docs/ir/cli/results.md#REQ-cli-018
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の merge 用モジュール, tests/common/mod.rs, docs/testing/merge-cli-test-cleanup.md
- Done when: REQ-cli-046 の `kotowari query` の tests が空でなく、REQ-cli-018 の tests に merge のテストがある。REQ-cli-046 の印付きテストは実行ファイルで TBL-cli-008 の五行（パスがない、--left か --right がない（それぞれ）、--left と --right が同じ、設定にないサーバ名（--left と --right のそれぞれ。--ref の名前は表の行に含まないため確かめない）、--format に text・json・diff 以外）のそれぞれで終了コード 2 になり書き込み先が変わらないことと、パスがない行を除く四行で標準エラーに表の文言が出ることを確かめる（パスがない行の文言は引数解析ライブラリのものなので確かめない）。REQ-cli-018 の印付きテストは実行ファイルで、--format json と --right のない merge が標準出力に error の項目を持つ JSON を出し終了コード 2 になることを確かめる。どのテストを根拠にし、どの既存テストの確かめ方を手本にしたかが docs/testing/merge-cli-test-cleanup.md にある
- Shown by: test — src/cli/merge.rs の test_merge_without_left_and_right_returns_error・test_merge_with_only_right_returns_error・test_merge_with_only_left_returns_error・test_run_merge_rejects_invalid_format_early・test_empty_paths_returns_error と tests/cli_error_handling.rs の test_merge_without_right_falls_back_and_fails_ssh・test_merge_no_paths_given、tests/contract/cli_results.rs の merge_without_an_explicit_destination_refuses_to_write を審査し、Approach and why の隔離した実行ファイルの呼び出しと一時的な設定ファイルで TBL-cli-008 の各行と JSON のエラーを確かめるテストを tests/contract/ の merge 用モジュールに新しく書いて印を付ける（元のテストは残す）
- Left to the implementer: 一つのテストで決定表の複数の行を確かめるか分けるか
- Stop and hand back if: TBL-cli-008 のエラーが接続より前に起きず、SSH の fixture なしでは表の行を観測できない

### S3: 終了コードと JSON の形の根拠テストを書いて印を付ける

- Purpose: merge の終了コードの規則と JSON の形に根拠テストを置く
- Specification: docs/ir/cli/merge.md#REQ-cli-047, docs/ir/cli/merge.md#REQ-cli-048
- Prerequisites: S2
- May change: tests/contract.rs, tests/contract/ の merge 用モジュール, tests/contract/cli_results.rs の印の行, tests/common/mod.rs, docs/testing/merge-cli-test-cleanup.md
- Done when: REQ-cli-047 と REQ-cli-048 の `kotowari query` の tests が空でない。REQ-cli-047 の印付きテストは、failed が空の merge で終了コード 0、failed が一件ある merge（参照先に対する競合で作る）で 2、エラーで止まった merge の実行ファイルの終了コードで 2 を確かめる。REQ-cli-048 の印付きテストは JSON に merged（path・status。バックアップを有効にした構成で backup、参照先を使う構成で ref_badge）、skipped（path と reason の項目があること。値は確かめない）、deleted（空でも出ること）、failed（path・error）があることと、ref（label と root）が参照先を使うときだけ出ること、merged の status がファイル全体を書き込んだとき "ok"、--dry-run では "would merge" になることを確かめる。FLAG の挙動に触れないよう、終了コード 0 の場合はスキップのない構成で --dry-run を付けずに作り、skipped を確かめるテストでは終了コードと標準エラーを確かめず、ref_badge と ref の場合は参照先に左右と同じ中身のファイルを置いた構成（三つの中身がそろう構成）で --dry-run を付けずに作る。どのテストを根拠にしたかが docs/testing/merge-cli-test-cleanup.md にある
- Shown by: test — src/service/merge.rs の test_merge_exit_code_success・test_merge_exit_code_failure・test_build_merge_output_with_ref・test_build_merge_output_no_ref_backward_compat・test_build_merge_output_deleted_empty_backward_compat と tests/cli_merge.rs の test_merge_json_format、tests/contract/cli_results.rs の dry_run_reports_the_merge_without_changing_the_destination・explicit_source_and_destination_merge_the_requested_file を審査し、execute_merge と format_json（または実行ファイルの --format json）を通すテストを新しく書いて印を付け、cli_results.rs のテストが要件を十分に確かめる場合は印の行に ID を足す（元のテストは残す）
- Left to the implementer: JSON の形を format_json の出力で確かめるか MergeOutput を serde_json の値にして確かめるか
- Stop and hand back if: backup の項目を出すのに、バックアップの取り込みで扱う構成（集約先の差し替え）以外の準備が要る

### S4: テキスト出力と参照先の警告の根拠テストを書いて印を付ける

- Purpose: merge のテキスト出力の行と、左右と同じ参照先を使わないことに根拠テストを置く
- Specification: docs/ir/cli/merge.md#REQ-cli-049, docs/ir/cli/merge.md#REQ-cli-050
- Prerequisites: S3
- May change: tests/contract.rs, tests/contract/ の merge 用モジュール, tests/common/mod.rs, docs/testing/merge-cli-test-cleanup.md
- Done when: REQ-cli-049 と REQ-cli-050 の `kotowari query` の tests が空でない。REQ-cli-049 の印付きテストは実行ファイルで、スキップの出ない構成で、書き込んだファイルの "Merged: パス"（バックアップを有効にした構成で " (backup: …)" が続くこと）、--dry-run の "Would merge: パス"、失敗したファイルの "Failed: パス (理由)"、書き込む対象もスキップも失敗もないときの "no files to merge in the specified path(s)" の行を確かめる。REQ-cli-050 の印付きテストは実行ファイルで、--ref に左と同じ指定と右と同じ指定のそれぞれで標準エラーに要件の文言が出て、参照先を使わずに merge が続く（書き込み先が更新され、--format json の出力に ref の項目がない）ことを確かめる。どのテストを根拠にしたかが docs/testing/merge-cli-test-cleanup.md にある
- Shown by: test — tests/cli_merge.rs の test_merge_dry_run_shows_plan・test_merge_writes_file・test_merge_equal_file_skipped・test_merge_duplicate_paths_deduplicated、src/service/output.rs の test_format_merge_text・test_format_merge_text_dry_run_prefix・test_format_merge_text_actual_backup_path・test_format_merge_text_dryrun_no_backup・test_format_merge_outcome_text_no_files・test_format_merge_outcome_text_success、src/cli/ref_guard.rs の四件を審査し、CliEnv（test-utils）で実行ファイルを動かすテストを新しく書いて印を付ける（元のテストは残す）
- Left to the implementer: none（"Failed: パス (理由)" の行は CliEnv::new_3way で --left local --right develop --ref staging とし、--force も --dry-run も付けず、local・develop・staging の三つに競合する中身のファイルを置いて作る）
- Stop and hand back if: SSH の fixture で失敗したファイルの行を作れる構成が作れない、バックアップの行を出す構成で集約先がテストの外に書かれる

### S5: 三者の競合の出し方の根拠テストを書いて印を付ける

- Purpose: 参照先に対する競合のあるファイルを書き込まず "three-way conflict" で出し、他のファイルは書き込むことに根拠テストを置く
- Specification: docs/ir/cli/merge.md#REQ-cli-051
- Prerequisites: S4
- May change: tests/contract.rs, tests/contract/ の merge 用モジュール, tests/contract/cli_results.rs の印の行, docs/testing/merge-cli-test-cleanup.md
- Done when: REQ-cli-051 の `kotowari query` の tests が空でない。印付きテストは --ref があり --force も --dry-run もない merge で、競合のあるファイルが書き込まれず failed の error がちょうど "three-way conflict" であることと、同じ実行の競合のないファイルが書き込まれることを確かめる。三つの中身がそろわないファイル（FLAG-cli-023）は構成に含めない。どのテストを根拠にしたかが docs/testing/merge-cli-test-cleanup.md にある
- Shown by: test — tests/contract/cli_results.rs の three_way_conflict_is_not_written_without_explicit_override と a_conflict_in_one_file_does_not_block_an_unrelated_three_way_merge、src/service/merge.rs の has_three_way_conflict を確かめるテスト（あれば）を審査する。cli_results.rs の一件目は error の値を "conflict" を含むかでしか確かめず、二件目は error の値を確かめないため、execute_merge を通して error の値を確かめるテストを新しく書いて印を付ける
- Left to the implementer: none
- Stop and hand back if: 競合のないファイルを同じ実行に含めると、そのファイルが三つの中身がそろわない扱い（FLAG-cli-023）になる構成しか作れない

### S6: 削除候補を利用者に一括で判断してもらい、決まったものを消す

- Purpose: 実装詳細をなぞるだけのテストを利用者の判断で消す
- Specification: docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S5
- May change: src/cli/merge.rs, src/service/merge.rs, src/cli/ref_guard.rs, src/service/output.rs のテスト部分, tests/cli_error_handling.rs, docs/testing/merge-cli-test-cleanup.md
- Done when: 利用者が削除候補の一覧（tests/cli_error_handling.rs の test_merge_help_shows_options、src/cli/merge.rs の test_make_args_default_format_is_text（テスト用の補助関数の既定値だけを確かめる）、src/service/merge.rs の test_build_merge_output（構造体を組み立てるだけの関数をなぞる））を一度に見て削除するものを決め、決まったものだけが消え、判断の結果が docs/testing/merge-cli-test-cleanup.md に記録されている。S2 から S5 で手本にした元のテストは候補に入れない
- Shown by: external — 削除候補の一覧（候補ごとに、同じ振る舞いを確かめて残るテストがあればその名前と、S1 の変異テストの結果で検知に効いていたか）を docs/testing/merge-cli-test-cleanup.md に書いてコミットして作業を返し、利用者の返答（消すものの一覧）が同じ文書に書き足されてから削除し、削除後に `cargo nextest run --all-features` が通ることを確認する
- Left to the implementer: 一覧の示し方
- Stop and hand back if: 利用者が一覧のうち一部の判断を保留した（保留分は消さずに残して報告する）

### S7: 整理後の変異テストで見逃しを比べ、全て決着させる

- Purpose: 削除で見逃しが増えていないことを確かめ、決着の対象に残った見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S6
- May change: tests/contract/ の merge 用モジュール（テストの追加）, tests/contract.rs, tests/common/mod.rs, .kotowari/mutants-equivalents.yaml, S6 で消したテストを戻すことだけのための S6 の各ファイルのテスト部分（既存のテストを消したり書き換えたりしない）, docs/testing/merge-cli-test-cleanup.md
- Done when: S1 と同じ三ファイルの変異テストの見逃しが、S1 の見逃しと、S1 で負荷の下で落ちるテストだけに検知された変異とを合わせたものの部分集合になっている（削除で増えた見逃しを生んだ削除は戻してある。S1 で負荷の下で落ちるテストだけに検知され S7 で見逃しになった変異は、決着の対象なら他の見逃しと同じく決着させる）。決着の対象の見逃しの全てに、足したテスト、同等変異の一覧への理由付きの登録（別の文脈のエージェントが落とすテストを書けなかった試みつき）、既存の FLAG-cli-0NN の範囲としての記録、新しい FLAG の候補としての報告のどれかが docs/testing/merge-cli-test-cleanup.md に記録されている。決着の対象でない見逃しは一覧として記録されている。同じ文書に REQ-cli-046 から 051 の verification とそれが要件の性質に合う理由が一行ずつあり、合わないと判断したものは IR を直さず候補として挙がっている
- Shown by: external — `scripts/mutants.sh src/cli/merge.rs src/service/merge.rs src/cli/ref_guard.rs` の出力と S1 の記録を突き合わせた結果を docs/testing/merge-cli-test-cleanup.md の「整理後」の節に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: none
- Stop and hand back if: 見逃しが不具合の疑いを示した、または verification の見直しの候補がある（どちらも文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S7 は完了しない。実装と IR は直さない）、負荷の下で落ちるテスト（tui_merge や agent_ssh）による見かけの検知が集計に混ざり、比較が成り立たない

### S8: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/cli/merge.md#REQ-cli-046, docs/ir/cli/merge.md#REQ-cli-047, docs/ir/cli/merge.md#REQ-cli-048, docs/ir/cli/merge.md#REQ-cli-049, docs/ir/cli/merge.md#REQ-cli-050, docs/ir/cli/merge.md#REQ-cli-051, docs/ir/cli/results.md#REQ-cli-018, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S7
- May change: none
- Done when: REQ-cli-046 から REQ-cli-051 の全てで `kotowari query` の tests が空でなく、REQ-cli-018 の tests に merge のテストがあり、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in $(seq -f 'REQ-cli-%03g' 46 51); do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari query REQ-cli-018 | jq '.items[0].tests'`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
