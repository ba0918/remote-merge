# sync のテスト整理の記録

sync の取り込みで加えた要件（REQ-cli-038 から REQ-cli-045）の根拠テストを整理した過程の記録。
変異テストの結果、各要件の根拠にしたテスト、削除候補と利用者の判断、整理後の見逃しの決着を残す。

## 整理前の変異テスト

整理を始める前のコミット 3b92547（作業ツリーに変更のない状態）で、次のコマンドを一度だけ実行した。
三つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/service/sync.rs src/cli/sync.rs src/service/source_pair.rs
```

全体の集計は `mutants: caught=43 survived=20 timeout=0 unviable=17 equivalent=0`（80 件、実行時間は約 22 分）。
スクリプトの終了コードは 1 だった。これは kotowari mutants が見逃しを error として報告したためで、メモリ上限による停止ではない（cargo-mutants は完了し、結果が読まれている）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/service/sync.rs | 16 | 1 | 0 | 8 |
| src/cli/sync.rs | 16 | 19 | 0 | 2 |
| src/service/source_pair.rs | 11 | 0 | 0 | 7 |

### 見逃し

位置は変異が入る行、変異は kotowari mutants の出力の文言のまま。同じ行に同じ文言が二つあるものは、行の中の別の `>`（列 24 と列 44）への変異である。
「決着の対象」は計画の区別による。sync の入口の規則を持つ関数（src/service/sync.rs の `compute_target_status`・`compute_sync_summary`・`sync_exit_code`、src/service/source_pair.rs の `resolve_source_pairs`、src/cli/sync.rs の `run_sync`・`print_sync_result`・`validate_sync_args`・`build_dry_run_targets`・`print_sync_plan`・`execute_sync`）の見逃しを対象にする。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/cli/sync.rs:87 | replace run_sync -> anyhow::Result<i32> with Ok(0) | 対象 |
| src/cli/sync.rs:87 | replace run_sync -> anyhow::Result<i32> with Ok(-1) | 対象 |
| src/cli/sync.rs:298 | delete ! in execute_sync | 対象（確認の答えの判定） |
| src/cli/sync.rs:416 | replace print_sync_result -> anyhow::Result<()> with Ok(()) | 対象（テキストの分岐は FLAG-cli-012 の範囲） |
| src/cli/sync.rs:536 | replace fetch_partial_tree -> anyhow::Result<FileTree> with Ok(Default::default()) | 対象外（走査の取り方。計画の対象の関数にない） |
| src/cli/sync.rs:548 | replace print_sync_plan with () | 対象 |
| src/cli/sync.rs:557 | replace \|\| with && in print_sync_plan | 対象 |
| src/cli/sync.rs:557 | replace > with < in print_sync_plan（列 24） | 対象 |
| src/cli/sync.rs:557 | replace > with == in print_sync_plan（列 24） | 対象 |
| src/cli/sync.rs:557 | replace > with >= in print_sync_plan（列 24） | 対象 |
| src/cli/sync.rs:557 | replace > with < in print_sync_plan（列 44） | 対象 |
| src/cli/sync.rs:557 | replace > with == in print_sync_plan（列 44） | 対象 |
| src/cli/sync.rs:557 | replace > with >= in print_sync_plan（列 44） | 対象 |
| src/cli/sync.rs:559 | replace > with < in print_sync_plan | 対象 |
| src/cli/sync.rs:559 | replace > with == in print_sync_plan | 対象 |
| src/cli/sync.rs:559 | replace > with >= in print_sync_plan | 対象 |
| src/cli/sync.rs:562 | replace > with < in print_sync_plan | 対象 |
| src/cli/sync.rs:562 | replace > with == in print_sync_plan | 対象 |
| src/cli/sync.rs:562 | replace > with >= in print_sync_plan | 対象 |
| src/service/sync.rs:57 | replace == with != in compute_sync_summary | 対象 |

### 変異と関係のないテストだけによる検知

検知した 43 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
変異した関数と関係がなく、負荷の下で落ちることのあるテストだけで検知された変異は次の三件。

| 位置 | 変異 | 失敗したテスト |
|---|---|---|
| src/cli/sync.rs:87 | replace run_sync -> anyhow::Result<i32> with Ok(1) | tui_merge の test_merge_cancel_with_n |
| src/cli/sync.rs:219 | delete ! in execute_sync（列 28） | agent_ssh_deploy の agent_ssh_read_files_roundtrip |
| src/cli/sync.rs:219 | replace == with != in execute_sync（列 80） | tui_merge の test_hunk_merge_left_to_right_with_l、test_hunk_merge_right_to_left_with_h_key |

一件目は `run_sync`（実行ファイルの sync だけが呼ぶ）の戻り値の変異で、TUI のテストとは関係がない。見かけの検知と判断した。
二件目と三件目は :219 の、中身の読み比べに失敗したファイルを書き込み予定から外す条件で、`execute_sync` のうち merge と共通の区間（`compute_status_from_trees` から `plan_deletions` と `skip_symlink_deletions` の呼び出しまで）にあるため、決着の対象ではない。
整理後の比較では、この三件は検知と見逃しのどちらにもなりうるものとして扱う。

ほかに、src/ の中の単体テストだけで検知された変異があり、整理で移すか消すテストに頼っているため整理後の比較で注意する。

- `validate_sync_args` を `Ok(())` にする変異: src/cli/sync.rs の validate_missing_left、validate_empty_right
- `build_dry_run_targets` を `vec![]` にする変異: src/cli/sync.rs の build_dry_run_targets_includes_would_merge、build_dry_run_targets_includes_connection_failures
- `compute_target_status` の `delete !` 二件と `sync_exit_code` の四件: src/service/sync.rs の compute_target_status_* と sync_exit_code_*
- src/service/source_pair.rs の検知 11 件: 全て source_pair の単体テスト（resolve_source_pairs_* と status 用の関数のテスト）

## 要件ごとの根拠テスト

根拠テストは `tests/contract/` の下に置いた。
関数呼び出しの準備（書き込み先 "develop"・"staging"・"production" を一時ディレクトリに差し替えた構成）は `tests/contract/sync_support.rs` にまとめた。
関数呼び出しのテストは `force: true`（dry-run のテストは `dry_run: true`）を渡し、確認のプロンプトがテストのプロセスの標準入力を読まないようにした。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。
src/ の中の単体テストのうち、同じ振る舞いを `execute_sync` を通す根拠テストで確かめたものは、移し元から消した（計画の「移したテストは移し元から消える」による）。

### 指定（REQ-cli-038）

根拠テストは `tests/contract/sync_targets.rs` にある。

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-cli-038（受け付ける指定） | one_source_and_two_targets_are_accepted_and_both_targets_are_written | src/service/source_pair.rs の resolve_source_pairs_two_servers と src/cli/sync.rs の validate_valid_args_passes、validate_multiple_right_servers（移して消した）。--left local と --right の二つで、結果の left と targets の label と、両方の書き込み先が書き換わることを確かめる |
| REQ-cli-038（TBL-cli-006 の五行） | each_invalid_specification_stops_with_its_error_and_changes_no_target | src/cli/sync.rs の validate_missing_left、validate_empty_right と src/service/source_pair.rs の resolve_source_pairs_duplicate_server_error、resolve_source_pairs_unknown_server_error、resolve_source_pairs_left_equals_right_error（移して消した）。元のテストは文言の一部だけを見ていたが、表の文言と完全に一致することを確かめる。設定にないサーバ名は --left と --right のそれぞれで確かめる（元のテストは --right だけ）。どの場合も二つの書き込み先が変わらない |

- --right の一つが --left と同じ行は、--left develop --right staging develop で確かめ、先に並ぶ staging も書き換わらないことを見る。

### 処理の順と確認（REQ-cli-039、REQ-cli-040）

既存の根拠テストはなかったため、新しく書いた。
処理の順は関数呼び出しで `tests/contract/sync_targets.rs` に、確認は隔離された SSH fixture（`CliEnv::new_3way` の develop と staging）に対して実行ファイルを起動し、標準入力に答えを渡して `tests/contract/sync_cli.rs` で確かめる。
`tests/contract/sync_cli.rs` は SSH fixture を使うため、ほかの SSH のテストと同じく `test-utils` の feature があるときだけ組み込む。

| 要件 | 根拠テスト | 確かめること |
|---|---|---|
| REQ-cli-039 | targets_are_processed_and_reported_in_the_order_given | 接続できる二つの書き込み先を develop・staging と staging・develop の二通りの順で指定し、結果の targets がそれぞれ指定順に並ぶ |
| REQ-cli-040（まとめと一度だけの確認、"y" と "Y"） | the_plan_of_every_target_is_shown_and_asked_once_and_y_writes | 両方の書き込み先に書き込むファイル一つと --delete の削除予定一つがある構成で、標準エラーに "Sync: local -> develop, staging"、"[develop] 1 files to merge, 1 files to delete"、"[staging] 1 files to merge, 1 files to delete" が含まれ、"Proceed? [y/N] " がちょうど一度出る。"y" と "Y" のそれぞれで両方の書き込み先の file.txt が書き換わる |
| REQ-cli-040（書き込む予定のない書き込み先） | a_target_with_nothing_to_write_has_no_plan_line | staging に書き込む予定がない構成で、develop の行はあり staging の行がない |
| REQ-cli-040（それ以外の答え） | any_other_answer_cancels_without_writing_and_exits_with_zero | "n"、"N"、空行、"yes" のそれぞれで "Sync cancelled." が出て、終了コード 0 で、両方の書き込み先の二つのファイルが変わらない |
| REQ-cli-040（尋ねない場合） | force_dry_run_and_nothing_to_write_do_not_ask | --force（書き込まれる）、--dry-run（書き込まれない）、書き込む予定がない構成のそれぞれで "Proceed? [y/N] " が出ない。標準入力は閉じてある |

- 件数が 0 の部分を省いた行（例: 削除予定のない書き込み先の "[先] N files to merge"）は、IR と一致するとみなすかの判断が要るため確かめない。確認のテストは、どの書き込み先も書き込むファイルと削除予定の両方を持つか、どちらも持たない構成にした。
- "y" で書き込んだ後に削除が行われたかは確かめない（削除の成否と状態は FLAG-cli-011 の範囲）。確認を断ったときの標準出力は FLAG-cli-007 の範囲のため確かめない。

### 状態・集計・JSON・終了コード・dry-run（REQ-cli-041 から REQ-cli-045）

関数呼び出しの根拠テストは `tests/contract/sync_results.rs` に、エラーで止まったときの終了コードは実行ファイルで `tests/contract/sync_cli.rs` にある。
状態は dry-run でなく force を指定して書き込みまで進む経路で確かめ、どの場合も書き込む予定のある別の書き込み先を同じ実行に含めた（書き込む予定のある書き込み先が一つもないと、状態は別の経路で決まるため）。
読めないファイルは、書き込み先のファイルの権限を 0o200 にして作る（テストは uid 1000 で実行し、開けないことをテストの中で確かめる）。読み込み元と大きさを変えて、中身の読み比べではなく書き込み先の読み直しで失敗させる。
集計と JSON のテストは、develop が "success"（書き込み 3・削除 2）、staging が "partial"（書き込み 2・失敗 1・削除 1）、production が "failed"（失敗 3）になる一つの構成を使い、集計の五項目が互いに違う値（3・1・5・3・4）になるよう件数を選んだ。項目の取り違えを検知するため。

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-cli-041（書き込めたファイルあり・失敗なし → "success"、なし・あり → "failed"） | tests/contract/cli_results.rs の a_failed_sync_target_is_reported_separately_with_a_nonzero_exit_code（印に ID を足した） | src/service/sync.rs の compute_target_status_all_success、compute_target_status_all_failed（移して消した） |
| REQ-cli-041（あり・あり → "partial"） | a_target_with_written_and_failed_files_is_partial_and_the_exit_code_is_two | src/service/sync.rs の compute_target_status_partial（移して消した）。同じ書き込み先に書けるファイルと読めないファイルを置く |
| REQ-cli-041（なし・なし → "success"） | a_target_with_nothing_written_and_nothing_failed_is_success_and_the_exit_code_is_zero | src/service/sync.rs の compute_target_status_no_files（移して消した）。一つ目の書き込み先に差分があり、二つ目が同じ中身 |
| REQ-cli-041（三つの状態が一度に並ぶ）、REQ-cli-042 | summary_counts_targets_successful_targets_and_files_across_every_target | src/service/sync.rs の compute_sync_summary_multiple_servers（移して消した）。successful_servers が "success" の書き込み先だけを数えることを、"partial" と "failed" を含む三つの書き込み先で確かめる |
| REQ-cli-043 | json_has_the_source_every_target_with_lowercase_status_and_the_summary | src/service/types.rs の sync_target_result_deleted_empty_included、sync_target_status_serializes_lowercase（移して消した）。`format_json` の出力を JSON として読み、最上位の left・targets・summary、left の label と root、各 target の target・merged・skipped・deleted・failed・status、削除のない書き込み先の空の deleted、"success"・"partial"・"failed" の三つの値、summary の五項目を確かめる |
| REQ-cli-044（全て "success" → 0） | a_target_with_nothing_written_and_nothing_failed_is_success_and_the_exit_code_is_zero、tests/contract/cli_results.rs の every_successful_sync_target_returns_a_zero_exit_code（印に ID を足した） | src/service/sync.rs の sync_exit_code_all_success（移して消した） |
| REQ-cli-044（"partial" → 2） | a_target_with_written_and_failed_files_is_partial_and_the_exit_code_is_two | 新しく書いた |
| REQ-cli-044（"failed" → 2） | a_failed_target_makes_the_exit_code_two_even_when_another_target_succeeds | src/service/sync.rs の sync_exit_code_some_failed（移して消した） |
| REQ-cli-044（エラーで止まった → 2） | a_sync_stopped_by_an_error_exits_with_two | 新しく書いた。設定にないサーバ名と、--right の名前の重なりで、実行ファイルの終了コードが 2 になり書き込み先が変わらない |
| REQ-cli-045 | dry_run_lists_every_planned_file_as_would_merge_and_changes_no_target | src/cli/sync.rs の build_dry_run_targets_includes_would_merge（移して消した）。二つの書き込み先の merged に書き込む予定の二つのファイルが status "would merge" で並び、既存のファイルの中身も、まだないファイルの有無も、--delete の削除予定のファイルも変わらない |

- tests/contract/cli_results.rs の sync の三件は、終了コードを `assert_ne!(exit_code, 0)` でしか見ていない。計画どおり三件の印に REQ-cli-044 を足したが、"partial" と "failed" で 2 になることは上の新しいテストが確かめる。summary を確かめていないため REQ-cli-042 は足していない。
- a_failed_target_makes_the_exit_code_two_even_when_another_target_succeeds は cli_results.rs の a_failed_sync_target_is_reported_separately_with_a_nonzero_exit_code と同じ構成だが、終了コードが 2 であることを確かめるために書いた。cli_results.rs は印の行を足すことだけが計画の範囲のため、そちらの確かめ方は変えていない。
- dry-run の deleted の中身（FLAG-cli-013）、読めないファイルを含む dry-run の状態と終了コード（FLAG-cli-014）、接続に失敗した書き込み先の並び（FLAG-cli-008）は確かめない。削除は "success" か "partial" になる書き込み先（書き込めたファイルがある書き込み先）にだけ置き、削除の成否が状態を変えうる構成（FLAG-cli-011）を避けた。
- src/cli/sync.rs の build_dry_run_targets_includes_connection_failures は、接続に失敗した書き込み先の dry-run での状態だけを確かめ REQ-cli-045 の根拠にならないため移さず、削除候補に挙げる。

## 削除候補と利用者の判断

整理の計画で削除を利用者が一括で判断する段の入力。
利用者の返答（消すものの一覧）を下の「判断の結果」に書き足してから削除する。

一覧は計画が名指しした 3 件だけになった。
上の審査で根拠にしたテストの元のテストは、計画の指示どおり移した時点で移し元から消したため（計 17 件。各節の表の「移して消した」）、ここには含めない。
根拠にしなかった重複テストは、削除できるファイル（src/cli/sync.rs、src/service/sync.rs、src/service/source_pair.rs、src/service/types.rs のテスト部分）に残っていなかった。
「代わりの根拠」は、そのテストが確かめていた振る舞いを今確かめているテスト（モジュール名は tests/contract/ の下のファイル名）。
「変異テストの裏付け」は、そのテストを消したときに整理後の変異テストの見逃しの増減として現れうるかを書いた。

| ファイル | テスト | 理由と代わりの根拠 | 変異テストの裏付け |
|---|---|---|---|
| src/cli/sync.rs | validate_empty_paths | パスがないときの `validate_sync_args` のエラーを見る。パスの必須は `src/main.rs` の clap の `required = true` が `execute_sync` より前に止めるため、この分岐は実行ファイルからは届かない。パスを省いた sync の扱いは FLAG-cli-006 で未決で、その挙動は clap の側にある。代わりの根拠はない | なし。この分岐には変異が入らず（`is_empty()` の呼び出しは変異の対象にならない）、消しても見逃しとして現れない |
| src/cli/sync.rs | validate_rejects_invalid_format | sync の入口ではなく、status・diff・merge と共有する `OutputFormat::parse` が未知の形式を拒むことを見る。sync の IR は形式の拒否を定めていない。同じ関数は src/service/output.rs の test_output_format_parse と status_output の format_accepts_text_json_and_diff_as_text_and_rejects_other_values が確かめて残る | なし。src/service/output.rs は変異テストの対象外 |
| src/cli/sync.rs | build_dry_run_targets_includes_connection_failures | 接続に失敗した書き込み先の dry-run での状態だけを見るため、REQ-cli-045 の根拠にならない。接続に失敗した書き込み先の並び（FLAG-cli-008）に関わる。代わりの根拠はない（接続に失敗した書き込み先は、dry-run でない経路で cli_results の a_connection_failure_on_one_target_does_not_prevent_the_other_sync が見る） | 一部。整理前は `build_dry_run_targets` を `vec![]` にする変異をこのテストと build_dry_run_targets_includes_would_merge（移して消した）が検知していた。今は sync_results の dry_run_lists_every_planned_file_as_would_merge_and_changes_no_target も検知しうる。接続に失敗した書き込み先を足す部分には変異が入らない |

候補にしていないもの:

- FLAG-cli-006 から 014 の挙動を確かめるテスト（src/cli/sync.rs の build_dry_run_targets_includes_deletions。src/service/output.rs の format_sync_text_* 七件は計画で削除できるファイルに入っていない）。
- src/service/sync.rs の plan_deletions の四件（削除は merge の取り込みで扱う）。
- src/service/types.rs の delete_file_result_backup_none_omitted と delete_status_serializes_lowercase。deleted の要素の形は merge の --delete と共有し、sync の IR は deleted の要素の形を定めていないため。
- src/service/source_pair.rs に残るテスト（status・diff・merge の左右の決め方と参照先の解決。status の整理で残すと決めたもの）。

src/service/types.rs から移して消した sync_target_result_deleted_empty_included と sync_target_status_serializes_lowercase は、src/service/types.rs が変異テストの対象外のため、整理後の変異テストの裏付けがない。同じ直列化の属性（`deleted` の `#[serde(default)]` と空でも出すこと、`SyncTargetStatus` の小文字）は sync_results の json_has_the_source_every_target_with_lowercase_status_and_the_summary が確かめて残る。

### 判断の結果

（利用者の返答を待っている。消すものの一覧をここに書き足してから削除する。）
