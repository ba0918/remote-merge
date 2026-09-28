# merge の指定・確認・出力のテスト整理の記録

merge の指定・確認・出力の取り込みで加えた要件（REQ-cli-046 から REQ-cli-051）と、merge 側の根拠を足す REQ-cli-018 の根拠テストを整理した過程の記録。
変異テストの結果、各要件の根拠にしたテスト、削除候補と利用者の判断、整理後の見逃しの決着を残す。

## 整理前の変異テスト

整理を始める前のコミット 7209529（作業ツリーに変更のない状態）で、次のコマンドを一度だけ実行した。
三つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/cli/merge.rs src/service/merge.rs src/cli/ref_guard.rs
```

全体の集計は `mutants: caught=57 survived=6 timeout=0 unviable=10 equivalent=0`（73 件、実行時間は約 17 分）。
スクリプトの終了コードは 1 だった。これは kotowari mutants が見逃しを error として報告したためで、メモリ上限による停止ではない（cargo-mutants は完了し、結果が読まれている）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/cli/merge.rs | 32 | 3 | 0 | 5 |
| src/service/merge.rs | 22 | 3 | 0 | 4 |
| src/cli/ref_guard.rs | 3 | 0 | 0 | 1 |

この計画の変異テストは TBL-cli-008 の全ての行を裏付けない。
--left と --right が同じ行と設定にないサーバ名の行は `src/service/source_pair.rs`（sync の整理で変異テストの対象にした）が、--format の行は `src/service/output.rs` の `OutputFormat::parse` が決めており、どちらのファイルもこの計画の変異テストの対象外である。
merge のテキストと JSON の整形（`src/service/output.rs`）も対象外で、実行ファイルと `format_json` を通す根拠テストで確かめる。

### 見逃し

位置は変異が入る行と列、変異は kotowari mutants の出力の文言のまま。
「決着の対象」は計画の区別による。merge の入口の規則を持つ関数（src/cli/merge.rs の `validate_merge_args`（--hunks の分岐を除く）・`run_merge`・`execute_merge`・`print_merge_result`、src/service/merge.rs の `plan_merge`・`build_merge_output`・`merge_exit_code`・`has_three_way_conflict`・`check_r2r_guard`、src/cli/ref_guard.rs の `validate_ref_side`）の見逃しを対象にする。
ただし `execute_merge` のうち、左右の比較から削除計画まで（`fetch_trees_and_statuses_for_merge` の呼び出しから `skip_symlink_deletions` の呼び出しまで）と、ファイルごとの書き込みと削除の実行（`execute_single_merge` と `execute_deletions` の呼び出し）は merge の後の回で扱うため記録だけする。
機密ファイルの件数の通知（FLAG-cli-022）、三者の中身がそろわないときの分岐（FLAG-cli-023）、dry-run で競合を確かめない条件（FLAG-cli-024）に入った見逃しは、その FLAG の範囲として記録する。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/cli/merge.rs:246:32 | replace && with \|\| in execute_merge | FLAG-cli-022 の範囲（機密ファイルの件数の通知の条件） |
| src/cli/merge.rs:246:47 | replace && with \|\| in execute_merge | FLAG-cli-022 の範囲（同上） |
| src/cli/merge.rs:316:62 | delete ! in execute_merge | 対象（全てのファイルが競合で外れたときの早期の戻り） |
| src/service/merge.rs:69:22 | replace \|\| with && in has_three_way_conflict | 対象 |
| src/service/merge.rs:69:38 | replace \|\| with && in has_three_way_conflict | 対象 |
| src/service/merge.rs:77:9 | delete match arm (Ok(base), Ok(left), Ok(right)) in has_three_way_conflict | 対象 |

### 変異と関係のないテストだけによる検知

検知した 57 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗で止まる（例: src/cli/merge.rs:120 の変異では 2862 件中 2464 件で止まった）ため、集めた名前は検知したテストの全てではなく、先に失敗したものである。

負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）で検知された変異はなかった。
変異した関数を merge の入口の外から使うテストだけで検知された変異は次の四件で、いずれも変異した関数を実際に通るテストによる検知のため、見かけの検知ではない。

| 位置 | 変異 | 失敗したテスト | 関係 |
|---|---|---|---|
| src/cli/merge.rs:120:8 | delete ! in execute_merge | contract の backup_cleanup::cleanup_removes_expired_history_of_a_configured_server | 期限切れのバックアップの片付けを merge で行う |
| src/service/merge.rs:29:19 | replace && with \|\| in plan_merge | src/cli/sync.rs の build_dry_run_targets_includes_would_merge | sync も `plan_merge` を使う |
| src/service/merge.rs:86:5 | replace find_symlink_target -> Option<String> with Some("xyzzy".into()) | src/handler/merge_exec.rs の single_file_merge_stores_backup_outside_both_targets | TUI の書き込みも `find_symlink_target` を使う |
| src/service/merge.rs:86:5 | replace find_symlink_target -> Option<String> with Some(String::new()) | 同上 | 同上 |

ほかに、src/ の中の単体テストだけで検知された変異があり、整理で消すテストに頼っていないかを整理後の比較で注意する。

- `validate_merge_args` の三件（`Ok(())` にする変異、:69 の `||` を `&&`、:76 の `!=` を `==`）: src/cli/merge.rs の test_empty_paths_returns_error、test_merge_with_only_left_returns_error、test_merge_with_only_right_returns_error と --hunks のテスト
- `run_merge` を `Ok(0)`・`Ok(1)`・`Ok(-1)` にする三件: src/cli/merge.rs の test_run_merge_rejects_invalid_format_early
- `validate_ref_side` の三件: src/cli/ref_guard.rs の四件のうち三件
- `plan_merge` の :29:12 の `delete !`、`merge_exit_code` の三件、`check_r2r_guard` の五件: src/service/merge.rs の test_plan_merge_*、test_merge_exit_code_*、test_check_r2r_guard_*
- `merge_partial_nodes`・`merge_partial_node`・`merge_file_node` と `determine_merge_action`・`find_symlink_target` の検知: 計画で記録だけする関数の単体テスト

## 要件ごとの根拠テスト

根拠テストは `tests/contract/` の下に置いた。元にしたテスト（`src/` の単体テスト、`tests/cli_merge.rs`、`tests/cli_error_handling.rs`、`tests/contract/cli_results.rs`）は消さずに残した。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。

準備は `tests/contract/merge_support.rs` にまとめた。
local と、リモートの設定を持つ "develop"（書き込み先）と "staging"（参照先）の三つのディレクトリを一時ディレクトリに作り、一時的な設定ファイルに書く。
実行ファイルの呼び出しは、環境変数を消し、HOME・XDG_CONFIG_HOME・XDG_DATA_HOME と作業ディレクトリを一時ディレクトリにして `--config` で一時的な設定ファイルを渡す（`tests/common/mod.rs` の `remote_merge_cmd` は実際の HOME を渡すため使わない）。
接続より前に止まる指定だけをこの呼び出しで確かめるため、サーバの host は "example.invalid" のままにした。

### 指定の誤りと JSON のエラー（REQ-cli-046、REQ-cli-018）

根拠テストは `tests/contract/merge_cli.rs` にある。SSH の fixture を使わないため `test-utils` の feature で囲まない。
どのテストも、終了コードが 2 であることと、local・develop・staging の file.txt が変わらないことを確かめる。
文言は標準エラーのいずれかの行が表の文言全体を含むことを確かめる。行の前に付く "Error: " は IR に宣言がないため確かめない。
エラーで止まったときの終了コード 2（REQ-cli-047 の三つめの場合）は、表の行のうち文言を確かめる四件の印に REQ-cli-047 を足して根拠にした。

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-cli-046（パスがない） | a_merge_without_a_path_stops_before_writing | tests/cli_error_handling.rs の test_merge_no_paths_given。元のテストは終了コードが 0 でないことだけを見るが、2 であることと書き込み先が変わらないことを確かめる。文言は引数解析ライブラリのものなので確かめない。src/cli/merge.rs の test_empty_paths_returns_error は、実行ファイルでは clap が先に止めるため届かない `validate_merge_args` の分岐を見ており、手本にしなかった |
| REQ-cli-046、REQ-cli-047（--left か --right がない） | a_merge_without_left_or_right_stops_with_the_required_sides_error | src/cli/merge.rs の test_merge_with_only_right_returns_error、test_merge_with_only_left_returns_error、test_merge_without_left_and_right_returns_error と tests/contract/cli_results.rs の merge_without_an_explicit_destination_refuses_to_write。元のテストは関数呼び出しで文言の一部（"--left and --right"）を見るが、実行ファイルで --left だけと --right だけのそれぞれについて標準エラーが表の文言全体を含むことを確かめる。tests/cli_error_handling.rs の test_merge_without_right_falls_back_and_fails_ssh は、--right が既定のサーバに補われて SSH のエラーで失敗するという、今の実装と違う前提の説明を持ち、終了コードが 0 でないことしか見ないため手本にしなかった |
| REQ-cli-046、REQ-cli-047（--left と --right が同じ） | a_merge_with_the_same_left_and_right_stops_with_the_different_sides_error | 新しく書いた（merge の入口を通して確かめる既存テストはなかった）。--left develop --right develop |
| REQ-cli-046、REQ-cli-047（設定にないサーバ名） | a_merge_with_an_unknown_server_stops_with_the_not_found_error | 新しく書いた。--left と --right のそれぞれに設定にない "nowhere" を渡す。--ref の名前は表の行に含まないため確かめない |
| REQ-cli-046、REQ-cli-047（--format が text・json・diff 以外） | a_merge_with_an_unknown_format_stops_with_the_format_error | src/cli/merge.rs の test_run_merge_rejects_invalid_format_early。元のテストは `run_merge` の呼び出しでエラーの文言が "Unknown format" を含むことを見るが、実行ファイルで --format xml のとき標準エラーが表の文言全体を含むことを確かめる |
| REQ-cli-018 | a_json_merge_stopped_by_an_error_prints_the_error_as_json | tests/contract/cli_results.rs の json_diff_returns_a_parseable_error_when_configuration_is_invalid（diff の同じ要件の例）。--format json で --right のない merge が、標準出力に error の項目（"--left and --right are required" を含む文字列）を持つ JSON を出し、終了コード 2 になる |

### 終了コードと JSON の形（REQ-cli-047、REQ-cli-048）

根拠テストは `tests/contract/merge_results.rs` にある。
develop と staging を `RuntimeTargets::with_local` で一時ディレクトリに差し替えて `execute_merge` を呼び、結果を `--format json` と同じ `format_json` で整形して JSON として読む。
merge は確認のプロンプトを出さないため `force: false` を渡し、--force で変わる挙動（FLAG-cli-021）に触れない。
バックアップの項目は、バックアップを有効にして集約先を `with_backup_store` で一時ディレクトリに差し替えた構成で確かめる（それ以外の準備は要らなかった）。
参照先に対する競合は、local と develop が同じ行を別々に変え、staging が元の中身を持つ構成で作る。
ref_badge と ref の場合は、staging に develop と同じ中身を置いた構成（三つの中身がそろい、競合がない構成）で --dry-run を付けずに作る。

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-cli-047（failed が空 → 0） | a_merge_without_failed_files_exits_with_zero | src/service/merge.rs の test_merge_exit_code_success。スキップのない構成で --dry-run を付けずに書き込み、failed が空で終了コードが 0 |
| REQ-cli-047（failed が一件 → 2） | a_merge_with_a_failed_file_exits_with_two | src/service/merge.rs の test_merge_exit_code_failure。参照先に対する競合で failed が一件になり、終了コードが 2 |
| REQ-cli-047（エラーで止まった → 2） | tests/contract/merge_cli.rs の表の行の四件（上の節） | 実行ファイルの終了コード |
| REQ-cli-048（merged の path・status "ok"・backup、空の skipped・deleted・failed、ref がない） | json_has_the_written_file_with_ok_and_backup_and_every_list_without_ref | tests/cli_merge.rs の test_merge_json_format（JSON として読めてパスを含むことだけを見る）と src/service/merge.rs の test_build_merge_output_no_ref_backward_compat、test_build_merge_output_deleted_empty_backward_compat。バックアップを有効にした構成で、merged の一件の path・status "ok"・空でない backup、skipped・deleted・failed が空の配列として出ること、ref の項目がないことを確かめる |
| REQ-cli-048（--dry-run の status "would merge"） | dry_run_json_has_the_planned_file_as_would_merge | tests/contract/cli_results.rs の dry_run_reports_the_merge_without_changing_the_destination。JSON の merged の status が "would merge" で、書き込み先が変わらない |
| REQ-cli-048（ref の label と root、merged の ref_badge） | json_with_a_reference_has_ref_and_the_ref_badge | src/service/merge.rs の test_build_merge_output_with_ref。ref の label が "staging" で root が空でない文字列、merged の ref_badge が空でない文字列。ref_badge と root の値の形は要件にないため確かめない |
| REQ-cli-048（skipped の path・reason） | json_has_skipped_files_with_path_and_reason | 新しく書いた。機密ファイル .env のスキップで、skipped の要素に path と reason の項目があることだけを確かめ、reason の値（FLAG-cli-018）と終了コード（FLAG-cli-016）と標準エラーは確かめない |
| REQ-cli-048（failed の path・error） | json_has_failed_files_with_path_and_error | 新しく書いた。参照先に対する競合で、failed の要素の path と空でない error を確かめる |

- tests/contract/cli_results.rs の merge のテストには印の ID を足さなかった。dry_run_reports_the_merge_without_changing_the_destination と explicit_source_and_destination_merge_the_requested_file は `execute_merge` の結果の構造体を見ており、--format json の出力の形（項目の有無と名前）を確かめないため。explicit_source_and_destination_merge_the_requested_file は status の値も見ない。どちらのテストの本体も変えていない。

### テキスト出力と左右と同じ参照先（REQ-cli-049、REQ-cli-050）

根拠テストは `tests/contract/merge_cli_ssh.rs` にある。
標準出力への振り分け（非公開の `print_merge_result`）と標準エラーへの警告を含めて確かめるため、隔離された SSH fixture（`CliEnv::new_3way`）に対して実行ファイルを起動する。SSH fixture を使うため、ほかの SSH のテストと同じく `test-utils` の feature があるときだけ組み込む。
どのテストも --left local --right develop とし、参照先を使うときは --ref staging にして、--force も --dry-run も（dry-run の行を除き）付けない（リモート間の merge を止める挙動 FLAG-cli-020 と --force の働き FLAG-cli-021 に触れない）。
スキップの出ない構成だけを使い、行は標準出力の行の並びと完全に一致することを確かめる。
`CliEnv` の設定ファイルには `[backup]` の節がないためバックアップは有効で、集約先は fixture の XDG_DATA_HOME の下に書かれる（テストの中でそのディレクトリがあることを確かめる）。

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-cli-049（"Merged: パス" と " (backup: …)"） | a_written_file_is_reported_as_merged_with_its_backup | tests/cli_merge.rs の test_merge_writes_file と test_merge_duplicate_paths_deduplicated（"Merged:" の数だけを見る）、src/service/output.rs の test_format_merge_text_actual_backup_path と test_format_merge_text。標準出力がちょうど一行の "Merged: file.txt (backup: …)" で、括弧の中が空でない |
| REQ-cli-049（バックアップのない "Merged: パス"） | a_written_file_without_a_backup_is_reported_as_merged | src/service/output.rs の test_format_merge_text。設定ファイルに `[backup]` と `enabled = false` を書き足し、標準出力がちょうど "Merged: file.txt" |
| REQ-cli-049（"Would merge: パス"） | dry_run_reports_the_planned_file_as_would_merge | tests/cli_merge.rs の test_merge_dry_run_shows_plan（"Would merge:" を含むことだけを見る）と src/service/output.rs の test_format_merge_text_dry_run_prefix、test_format_merge_text_dryrun_no_backup。標準出力がちょうど "Would merge: file.txt" で、書き込み先が変わらない |
| REQ-cli-049（"Failed: パス (理由)"） | a_failed_file_is_reported_with_its_reason | 新しく書いた。local・develop・staging に競合する中身を置き --ref staging で起動し、標準出力がちょうど "Failed: file.txt (three-way conflict)" で、書き込み先が変わらない |
| REQ-cli-049（"no files to merge in the specified path(s)"） | nothing_to_merge_is_reported_as_no_files_to_merge | tests/cli_merge.rs の test_merge_equal_file_skipped（"no files to merge" を含むことだけを見る）と src/service/output.rs の test_format_merge_outcome_text_no_files、test_format_merge_outcome_text_success。同じ中身の file.txt で、標準出力がちょうどその一行 |
| REQ-cli-050 | a_reference_equal_to_either_side_is_warned_about_and_not_used | src/cli/ref_guard.rs の ref_same_as_left_returns_none、ref_same_as_right_returns_none（ref_different_returns_some、ref_none_returns_none も審査した）。--ref local（左と同じ）と --ref develop（右と同じ）のそれぞれで、標準エラーの一行が要件の文言と一致し、終了コード 0 で書き込み先が更新され、--format json の出力に ref の項目がない |

### 競合のあるファイルを失敗として出す（REQ-cli-051）

根拠テストは `tests/contract/merge_results.rs` にある（関数呼び出し。準備は上の終了コードと JSON の形の節と同じ）。

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-cli-051 | a_conflicting_file_fails_as_a_three_way_conflict_and_other_files_are_written | tests/contract/cli_results.rs の three_way_conflict_is_not_written_without_explicit_override（error の値を "conflict" を含むかでしか見ない）と a_conflict_in_one_file_does_not_block_an_unrelated_three_way_merge（error の値を見ない）。--ref staging があり --force も --dry-run もない一回の実行で、failed がちょうど `[{"path": "file.txt", "error": "three-way conflict"}]` で、競合のある file.txt が書き込まれず、競合のない other.txt（staging が develop と同じ中身）が merged のただ一件として書き込まれる |

- 三つの中身がそろわないファイル（FLAG-cli-023）は構成に含めない。other.txt は local・develop・staging の三つにある。
- src/service/merge.rs には `has_three_way_conflict` を直接確かめる単体テストがなかった。整理前の変異テストでは、この関数の変異は cli_results の a_conflict_in_one_file_does_not_block_an_unrelated_three_way_merge が検知していた。

## 削除候補と利用者の判断

整理の計画で削除を利用者が一括で判断する段の入力。
利用者の返答（消すものの一覧）を下の「判断の結果」に書き足してから削除する。

一覧は計画が名指しした 3 件である。
上の審査で根拠の確かめ方の手本にした元のテストは候補に入れていない。
「残る代わりのテスト」は、そのテストが確かめていた振る舞いを消した後も確かめているテスト。
「整理前の変異テストでの働き」は、整理前の変異テスト（コミット 7209529）でそのテストが変異の検知に効いていたか。

| ファイル | テスト | 理由 | 残る代わりのテスト | 整理前の変異テストでの働き |
|---|---|---|---|---|
| tests/cli_error_handling.rs | test_merge_help_shows_options | `merge --help` の出力に "--dry-run" があることだけを見る。ヘルプの文面は clap の引数定義から作られ、merge の IR はヘルプの内容を定めていない | なし（--dry-run の働きは tests/contract/merge_results.rs の dry_run_json_has_the_planned_file_as_would_merge と tests/contract/merge_cli_ssh.rs の dry_run_reports_the_planned_file_as_would_merge が確かめる）。ほかのサブコマンドのヘルプのテストと test_help_shows_usage は残る | なし。引数定義（src/main.rs）は変異テストの対象外 |
| src/cli/merge.rs | test_make_args_default_format_is_text | テスト用の補助関数 `make_args` の既定値（format が "text"）だけを見ており、製品コードを通らない | 不要（製品の振る舞いを確かめていない）。--format の既定値が text であることは、--format を付けない実行ファイルのテスト（tests/contract/merge_cli_ssh.rs のテキスト出力の四件）が出力の形で確かめる | なし。製品コードを呼ばないため、どの変異も検知しない |
| src/service/merge.rs | test_build_merge_output | 引数をそのまま構造体に詰める `build_merge_output` の結果の merged の件数と failed が空であることを見る。構造体を組み立てるだけの関数をなぞる | src/service/merge.rs の test_build_merge_output_with_ref、test_build_merge_output_no_ref_backward_compat、test_build_merge_output_with_deleted、test_build_merge_output_deleted_empty_backward_compat と、tests/contract/merge_results.rs の JSON の形の根拠テスト | なし。`build_merge_output` の変異（`Default::default()` にする）はコンパイルできず unviable で、ほかの変異はこの関数に入らない |

### 判断の結果

利用者は候補の 3 件（test_merge_help_shows_options、test_make_args_default_format_is_text、test_build_merge_output）を全て消すと決めた。
理由は上の表のとおりで、どれも整理前の変異テストの検知に効いておらず、merge の要件の根拠でもない。

決まった 3 件だけを消し、消した後に `cargo nextest run --all-features` が通ることを確かめた（2879 tests run: 2879 passed）。

## 整理後の変異テスト

削除を終えたコミット 9528904 で、整理前と同じコマンドを一回実行した。実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/cli/merge.rs src/service/merge.rs src/cli/ref_guard.rs
```

全体の集計は `mutants: caught=58 survived=5 timeout=0 unviable=10 equivalent=0`（73 件、実行時間は約 17 分）。
スクリプトの終了コードは整理前と同じく 1（見逃しの error による。メモリ上限での停止ではない）。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/cli/merge.rs | 33 | 2 | 0 | 5 |
| src/service/merge.rs | 22 | 3 | 0 | 4 |
| src/cli/ref_guard.rs | 3 | 0 | 0 | 1 |

### 整理前との比較

整理後の見逃し 5 件は全て整理前の見逃しに含まれる（src/cli/merge.rs:246 の二件、src/service/merge.rs:69 の二件と :77:9）。削除で増えた見逃しはないため、戻した削除はない。

src/cli/merge.rs:316:62 の `delete !` は、整理前は見逃し、整理後は tui_merge の test_hunk_merge_left_to_right_with_l と test_hunk_merge_right_to_left_with_h_key だけに検知された。
この変異は CLI の `execute_merge` の中にあり、TUI の hunk の書き込みは通らない。コミットに含めない一時的な書き換えでこの変異を入れて `cargo nextest run --all-features --no-fail-fast` を実行すると 2880 件が全て通ったため、負荷の下での見かけの検知と判断し、見逃しとして扱う。
これ以外に、負荷の下で落ちるテストだけで検知された変異はなかった。整理前と整理後で検知したテストの名前（nextest が最初の失敗で止まるまでのもの）の違いは、この一件を除き、同じ変異を検知した tests/cli_merge.rs のテストの数だけだった。

### 見逃しの決着

テストを足したものは、コミットに含めない一時的な書き換えでその変異を入れ、足したテストが落ちることを確かめた（書き換えは元に戻し、src/ に差分がないことを確かめた）。

| 位置 | 変異 | 決着 |
|---|---|---|
| src/cli/merge.rs:246:32 | replace && with \|\| in execute_merge | FLAG-cli-022 の範囲として記録する（機密ファイルの件数の通知の条件）。テストは足さない |
| src/cli/merge.rs:246:47 | replace && with \|\| in execute_merge | 同上 |
| src/service/merge.rs:69:22 | replace \|\| with && in has_three_way_conflict | テストを足した。tests/contract/merge_results.rs の a_binary_file_changed_on_only_one_side_is_not_a_conflict_and_is_written（REQ-cli-051）で、UTF-8 として読めない二つのファイルのうち a.bin は左だけ、b.bin は右だけが参照先から変わる構成で、failed が空、二つとも merged に並び書き込まれ、終了コード 0 になることを確かめる。変異では b.bin（左が参照先と同じ）が中身の比較に進み、UTF-8 でないため競合とされて落ちる |
| src/service/merge.rs:69:38 | replace \|\| with && in has_three_way_conflict | 同じテストを足した。変異では a.bin（右が参照先と同じ）も b.bin も競合とされて落ちる |
| src/service/merge.rs:77:9 | delete match arm (Ok(base), Ok(left), Ok(right)) in has_three_way_conflict | FLAG-cli-025 の範囲として記録する（[決定記録 A1](../decision/records/2026-09-28-merge-cli-mutant-flags.md#A1)）。別々の箇所の変更を競合とするかは未決のため、テストは足さない（下の「新しい FLAG の候補と利用者の判断」の一件目） |
| src/cli/merge.rs:316:62 | delete ! in execute_merge | FLAG-cli-026 の範囲として記録する（[決定記録 A2](../decision/records/2026-09-28-merge-cli-mutant-flags.md#A2)）。書き込むファイルのない merge で集約先の場所が決まらないときに止まるかは未決のため、テストは足さない（下の「新しい FLAG の候補と利用者の判断」の二件目） |

決着の対象でない見逃しはない（整理前も整理後も、見逃しは全て決着の対象か FLAG の範囲だった）。
テストを足した二件は、足した後に変異テストを回し直してはいない。一件ずつ一時的な書き換えで検知を確かめた。

### 新しい FLAG の候補と利用者の判断

どちらも、変異を落とすテストを書くと、IR が決めていない挙動を根拠テストで決めてしまうため、テストを足さずに利用者の判断を待った。実装と IR は変えていない。

一件目: src/service/merge.rs:77:9（参照先に対して左右が別々の箇所を変えたテキストのファイル）

- 観測: `has_three_way_conflict` は、三つの中身が全て違うテキストのファイルでも、`detect_conflicts` が重なる変更を見つけなければ競合としない。コミットに含めない一時的なテストで、参照先 "a b c d e"（各行）、左 "A b c d e"、右 "a b c d E" を --ref 付き・--force なしで merge すると、failed は空、終了コード 0 で、右（書き込み先）は左の中身 "A b c d e" に書き換わり、右の変更 "E" は失われた。変異（この分岐を消す）では三つの中身が全て違うテキストのファイルが全て "three-way conflict" になり、右の変更は残る。
- IR との関係: REQ-cli-051 は競合を「参照先に対して左右が異なる変更をした」ものとし、REQ-cli-016 は「参照先に対して左右に異なる変更があるとき」競合を示すとする。REQ-cli-017 は競合のある内容を利用者の判断なしに一方の変更だけを選んで上書きしないとする。EX-cli-031 は同じ箇所を別々に変えた場合だけを例にしている。別々の箇所の変更を競合に含むかを IR は決めておらず、含むと読むなら今の実装は右の変更を黙って上書きする（不具合の疑い）。
- 判断してほしいこと: (a) 別々の箇所の変更は競合でなく、今の実装（左の中身で書き込む）が IR の意図どおり（その場合、変異を落とすテストを足す）か、(b) 別々の箇所の変更も競合で、今の実装が IR と食い違う（FLAG として記録する）か、(c) 未決の FLAG として残すか。

二件目: src/cli/merge.rs:316:62（全てのファイルが競合で外れたときの早期の戻り）

- 観測: この変異では、全てのファイルが参照先に対する競合で外れた merge が早期に戻らず書き込みの段に進み、何も書き込まずに同じ出力を返す。違いはバックアップの集約先の予約だけで、予約した空の場所は `finish_backup_session` が片付ける。そのため違いが出るのは、バックアップが有効で集約先の場所が決まらない構成だけである。その構成では、今の実装は failed に "three-way conflict" を出すが、変異では "backup store location could not be determined" のエラーで止まる。逆向き（ファイルが全て機密ファイルのスキップなどで外れ、競合もないとき）には、今の実装が書き込みの段に進んでエラーで止まり、変異では早期に戻ってスキップの結果を返す。
- IR との関係: REQ-backup-018 は「バックアップが有効で集約先の場所が決まらないとき、merge と sync は書き込む前にエラーで止まる」とする。書き込むファイルが一つもない merge にもこれが及ぶかを IR は決めておらず、今の実装は競合で外れたときは止まらず、スキップで外れたときは止まる。どちらに合わせるテストも、その決まっていない点を決めてしまう。
- 判断してほしいこと: (a) 書き込むファイルがないときは止まらないのが意図どおり（競合の場合の今の挙動を根拠テストで確かめ、スキップの場合との違いを FLAG にする）、(b) 書き込むファイルがなくても止まるのが意図どおり（競合の場合の今の挙動を FLAG として記録する）、(c) 未決の FLAG として残す、のどれにするか。同等変異としては登録しない（集約先の場所が決まらない構成で観測できる違いがあるため）。

利用者の判断: 二件ともどちらの扱いも決めず、(c) 未決の FLAG として残す。一件目は FLAG-cli-025「別々の箇所の変更は競合か」（[決定記録 A1](../decision/records/2026-09-28-merge-cli-mutant-flags.md#A1)）、二件目は FLAG-cli-026「書き込むファイルのない merge と集約先の場所」（[決定記録 A2](../decision/records/2026-09-28-merge-cli-mutant-flags.md#A2)）として記録された。
この二件の見逃しはそれぞれの FLAG の範囲として決着させ、テストは足さない。これで決着の対象の見逃しは全て決着した。

## 要件の verification の見直し

merge の要件の verification は全て unit で、いずれも具体的な場面の入力で結果が決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-cli-046 | unit | 決定表 TBL-cli-008 の各行の指定に対して、エラーの文言と終了コードが決まる |
| REQ-cli-047 | unit | failed の有無とエラーの場面で終了コードが決まる |
| REQ-cli-048 | unit | 一つの実行の結果（書き込み・dry-run・参照先・スキップ・失敗の場面）に対して JSON の項目と値が決まる |
| REQ-cli-049 | unit | 書き込み・dry-run・失敗・対象なしの場面でテキストの行が決まる |
| REQ-cli-050 | unit | --ref が左か右と同じ場面で、警告の文言と参照先を使わないことが決まる |
| REQ-cli-051 | unit | 左右と参照先の中身の組に対して、競合として失敗に出すか書き込むかが決まる |
