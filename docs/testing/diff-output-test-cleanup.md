# CLI diff の差分の出し方のテスト整理の記録

CLI diff の差分の出し方の取り込み（[決定記録](../decision/records/2026-09-29-adopt-diff-output.md)）で加えた要件（REQ-cli-052 から 061。REQ-cli-055 は[別の決定記録](../decision/records/2026-09-29-diff-max-files.md)で実装とテストが済んでいる）と、テストのなかった既存要件 REQ-cli-001 の根拠テストを整えた過程の記録。
各要件の根拠にしたテスト、変異テストの結果、見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テスト 129 件のうち実装の中身をなぞるだけのものは 6 件（`src/service/diff.rs` の使われていない `diff_exit_code` のテスト）だったが、これまでの回と同じく元の単体テストは消さないため、この整理ではテストを消していない。
テストを削除しない整理のため、整理前の変異テストは回していない（REQ-testing-012、[決定記録 A2](../decision/records/2026-09-28-mutation-scope.md#A2)）。

## 要件ごとの根拠テスト

手本にした元のテストは消さず、書き換えていない。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた（既存の挙動を確かめるテストのため、失敗する段階はない）。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
FLAG-cli-028 から 039 の挙動（旧総合仕様の JSON の例の形、ハッシュを計算する場所、片側で読めないテキストファイル、exclude に当たるファイルのパスの指定、行番号の値、include の外のディレクトリ、files_with_changes がバイナリ・symlink・機密を数えること、中身の同じバイナリ、ディレクトリの配下の変更のない機密ファイル、片側にだけある 0 バイトのファイル、100MB を超えるファイル）は確かめない。symlink・ディレクトリ symlink・root_dir の外（diff の次の回）と --ref・競合（その次の回）も確かめない。
終了コード 1、"N file(s) with changes out of M total" の数、summary.files_with_changes は、バイナリと隠した機密ファイルも数える（FLAG-cli-034）ため、変更のあるテキストファイルだけの構成で確かめ、バイナリと機密ファイルの場合ではこれらを確かめない。

### 関数呼び出しの根拠テスト（REQ-cli-001・052・053・054・056・057・058・059・060・061）

根拠テストは `tests/contract/diff_selection.rs`（比べる対象と終了コード）、`tests/contract/diff_format.rs`（JSON とテキストの形と --max-lines）、`tests/contract/diff_file_kinds.rs`（機密ファイル・片側にだけあるファイル・バイナリ）にあり、共有する組み方と読み方は `tests/contract/diff_support.rs` にある。
組み方は `tests/contract/diff_max_files.rs` と同じで、設定を `load_config_from_paths` で読み、`RuntimeTargets::with_local` でサーバ "develop" をローカルの一時ディレクトリに差し替え、`execute_diff` を関数呼び出しで実行する。バックアップは無効にし、集約先も一時ディレクトリに差し替えた。
設定に [filter] を書かないため、機密ファイルのパターンは既定値（".env"・".env.*" など）になる。
JSON は `format_json` を通した文字列を `serde_json` で読み、テキストは `format_multi_diff_text` の文字列で確かめ、終了コードは `execute_diff` が返す値で確かめる。
場合の分け方は、`tests/contract/cli_results.rs` の diff のテスト（EX-cli-001・002・052。"folder/" と "folder" の結果の一致まで確かめている）、`tests/cli_diff.rs` と `tests/cli_diff_general.rs` の該当するテスト、`src/service/diff.rs`（`convert_hunks` の --max-lines の単体テスト）と `src/service/output.rs`（テキストの形の単体テスト）の単体テストを手本にした。
ハッシュの期待値は製品の関数（`compute_sha256`）を使わず、テストの中で sha2 crate を直接使って作った。

| 要件 | 根拠テスト | 確かめること |
|---|---|---|
| REQ-cli-052 | req_cli_052_without_paths_every_changed_file_in_the_root_dir_is_compared | パスなしで、root_dir の直下の "a.txt" と "d/" の下の "d/b.txt"（どちらも変更のあるファイル）が出る。変更のない "same.txt" が出ないことは要件にないため確かめない |
| REQ-cli-052 | req_cli_052_file_paths_compare_only_those_files | 変更のあるファイル 3 件のうち 2 件のパスを指定すると、その 2 件だけが出る |
| REQ-cli-052 | req_cli_052_directory_path_compares_its_children_with_or_without_a_slash | "d/" の指定で "d/" の下（孫を含む）の変更のあるファイルだけが出て、ほかのディレクトリと直下のファイルは出ない。"d" の指定と JSON の文字列と終了コードが一致する |
| REQ-cli-001 | req_cli_001_directory_json_has_a_structured_entry_for_each_changed_file | ディレクトリを指定した JSON の files が配下の 2 件の項目を持ち、それぞれの hunks の lines が削除と追加の行になる |
| REQ-cli-053 | req_cli_053_json_has_files_and_summary_and_each_file_has_the_documented_keys | 打ち切らない JSON の上位のキーが files と summary だけで（truncated・changed_files_total・errors がない）、テキストの項目のキーが path・left・right・sensitive・truncated・hunks だけで（binary・left_hash・right_hash・note がない）、left と right の label が "local" と "develop" で root が文字列であること（root の書き方は要件にないため確かめない）、hunks の index・left_start・right_start が数であること、lines の type が "context"・"removed"・"added" の三つになり content が行の中身であること |
| REQ-cli-053 | req_cli_053_binary_hashes_and_note_appear_only_on_the_files_they_apply_to | テキスト・バイナリ・機密ファイルを並べて比べ、binary・left_hash・right_hash がバイナリの項目だけに、note が機密ファイルの項目だけに出る。errors が出ない |
| REQ-cli-053 | req_cli_053_json_has_truncated_and_changed_files_total_when_max_files_truncates | 変更のあるテキストファイル 2 件を --max-files 1 で比べると、JSON の上位に truncated が true、changed_files_total が 2 で出る |
| REQ-cli-054 | req_cli_054_change_lines_stop_at_the_limit_without_counting_context | 文脈 3 行の後に削除 2 行と追加 3 行の変更があるファイルで --max-lines 3 のとき、追加と削除の行が 3 行（削除 2 行と最初の追加 1 行）で止まり、前の文脈の行も出て、truncated が true で、テキストに "... (output truncated)" が出る。文脈の行を数えると変更の行が一行も出ないため、文脈を数えないことが分かる |
| REQ-cli-054 | req_cli_054_zero_or_no_limit_outputs_every_change_line | 同じファイルで --max-lines 0 と指定なしのそれぞれで、5 行全てが出て truncated が false で、"(output truncated)" が出ない |
| REQ-cli-056 | req_cli_056_exit_code_is_zero_without_changes_and_one_with_changes | 変更のないテキストファイルで 0、変更のあるテキストファイルで 1 |
| REQ-cli-057 | req_cli_057_a_glob_path_that_matches_nothing_is_an_error | glob 文字を含むパス（"nothing/*.txt"。全体の走査の後にパスを解決する `compute_statuses_and_resolve` の経路）が何にも当たらないとき、`execute_diff` がエラーを返し、その文が "specified path(s) not found on either side" を含む |
| REQ-cli-058 | req_cli_058_text_has_headers_hunk_lines_and_the_summary | 変更のある 2 件と変更のない 1 件を指定したテキストに、"--- a/f.txt (local)"、次の行に "+++ b/f.txt (develop)"、次に "@@" で始まる行、次に " keep"・"-old"・"+new" が並び、もう一つのファイルの見出しもあり、最後の行が "2 file(s) with changes out of 3 total" になる。"@@" の行の数字は確かめない（FLAG-cli-032） |
| REQ-cli-059 | req_cli_059_sensitive_text_is_hidden_without_force | 変更のある ".env" で sensitive が true、hunks が空、note が "Content hidden (sensitive file). Use --force to show." で、テキストに note が出て、テキストにも JSON にも中身が出ない |
| REQ-cli-059 | req_cli_059_sensitive_binary_hides_its_hashes_without_force | NUL を含む変更のある ".env.local" で同じく隠され、left_hash・right_hash がなく、テキストにも JSON にも左右のハッシュが出ず、テキストに "sha256" が出ない |
| REQ-cli-060 | req_cli_060_a_file_on_one_side_is_all_removed_or_all_added_lines | 左にだけある 2 行のファイルが全て削除の行、右にだけある 2 行のファイルが全て追加の行になり、テキストに "-"・"+" の接頭辞で出る。0 バイトのファイルは使わない（FLAG-cli-038） |
| REQ-cli-061 | req_cli_061_a_nul_in_the_first_8192_bytes_makes_the_file_binary | NUL を含むファイルで binary が true、hunks が空、left_hash・right_hash が左右の SHA-256 で、テキストに "Binary files differ (left: sha256=…, right: sha256=…)" の行が出る |
| REQ-cli-061 | req_cli_061_invalid_utf8_in_the_first_8192_bytes_makes_the_file_binary | 先頭に不正な UTF-8 を含むファイルで同じ |
| REQ-cli-061 | req_cli_061_a_nul_only_after_8192_bytes_leaves_the_file_as_text | 先頭 8,192 バイトが ASCII だけで、その後にだけ NUL を含むファイルで、binary・left_hash がなく、NUL を含む行が削除と追加の行として出る。境界で多バイト文字が切れて不正な UTF-8 にならないよう、先頭は ASCII だけにした |
| REQ-cli-061 | req_cli_061_the_side_without_the_binary_is_missing | 左にだけあるバイナリで left_hash だけが出て、テキストが "Binary files differ (left: sha256=…, right: missing)" になる |
| REQ-cli-061 | req_cli_061_the_unreadable_side_of_a_binary_is_missing | 左のバイナリをパーミッション 000 にして読めなくすると、left_hash がなく、テキストが "Binary files differ (left: missing, right: sha256=…)" になる。読めなくしても読めてしまう（root で動く）ときは確かめられないため飛ばす |

- 中身の同じバイナリを指定する場合（FLAG-cli-035）と、バイナリでない片側で読めないファイル（FLAG-cli-030）は使わない。
- errors は、比べられないパスがあるときの出方が diff の次の回（symlink と root_dir の外）の範囲のため、出ないことだけを確かめた。left_start・right_start は値を確かめない（FLAG-cli-032）。
- REQ-cli-056 のエラーの 2 と REQ-cli-057 の警告と JSON のエラーは、次の節の実行ファイルのテストで確かめる。比べられないファイルによる 2 と通常の出力は diff の次の回の範囲のため確かめない。

### 見つからないパスの警告とエラーの根拠テスト（REQ-cli-057・056、実行ファイル）

根拠テストは `tests/contract/diff_output_cli.rs` にある（`test-utils` の feature が要る。SSH の試験サーバを使うため）。
標準エラーの警告と、main.rs が出すエラー（テキストでは標準エラー、JSON では標準出力の JSON）と終了コード 2 は関数呼び出しでは観測できないため、実行ファイルを試験 SSH サーバに対して `diff --left local --right develop` で起動する。パスを 1〜20 個指定する経路（`run_diff_fast_path`）を通る。
起動の組み方は `tests/contract/scan_listing_cli.rs` の `launch_status` と同じで（`--config` に一時ディレクトリの設定を渡し、作業ディレクトリを一時ディレクトリの下にし、`env_clear` のうえ HOME・XDG の変数を一時ディレクトリに向けて PATH だけを引き継ぐ）、その補助は status に固定されているため、diff 用の `launch_diff` を同じ形で新しいモジュールに書いた。起動の前に `TestDirs::assert_isolated_config_at` の隔離の確認を通す。
場合は `tests/cli_diff_general.rs` の test_diff_nonexistent_file を手本にした。

| 要件 | 根拠テスト | 確かめること |
|---|---|---|
| REQ-cli-057 | req_cli_057_a_missing_path_is_warned_and_the_rest_are_compared | 変更のある "a.txt" と見つからない "missing.txt" を指定すると、標準エラーに "Warning: 'missing.txt' not found on either side" が出て（"a.txt" の警告とエラーは出ない）、標準出力に "a.txt" の差分が出て、終了コードが 1 になる |
| REQ-cli-057・056 | req_cli_057_every_path_missing_is_an_error_in_text | 見つからない 2 件を指定したテキストで、標準エラーに 2 件それぞれの警告が出て、標準出力か標準エラーに "specified path(s) not found on either side" の文が出て、終了コードが 2 になる。エラーの文の出力先、行の "Error: " の接頭辞、標準出力が空であることは要件にないため確かめない |
| REQ-cli-057・056 | req_cli_057_every_path_missing_is_a_json_error_on_stdout | 見つからない 1 件を指定した JSON で、標準出力が JSON として読めて "specified path(s) not found on either side" の文を含み、標準エラーに警告が出て、終了コードが 2 になる。JSON の形（{"error": ...}）は要件にないため確かめない |

## 整理後の変異テスト

変異テストは決着の対象の関数の全ての変異を一度実行し（全体の実行）、その後にテストを足したため、全体の実行で caught にならなかった変異を含む関数に絞って回し直した（下の「テストを足した後の回し直し」）。
全体の実行をしたのは根拠テストと記録を足した後のコミット 7633ae0 で、作業ツリーに変更のない状態で行い、実行中は作業ツリーにも他の cargo のコマンドにも触れていない。
PC の負荷を抑えるため並列数を 1 にした（スクリプトはサービスを CPUWeight=idle と Nice=19 で動かす）。

```sh
MUTANTS_JOBS=1 scripts/mutants.sh --re '(execute_diff|run_diff_fast_path|run_diff_full_scan|compute_statuses_and_resolve|read_file_bytes_tolerant|build_diff_output|convert_hunks|build_masked_diff_output|format_diff_text|format_binary_hashes|format_multi_diff_text|has_changes|limit_changed_files|omitted_changed_files|changes_without_reading|ChangedFileBudget|is_binary|compute_diff|build_hunks|make_hunk|compute_sha256)' src/cli/diff.rs src/service/diff.rs src/service/output.rs src/service/types.rs src/service/max_files.rs src/diff/engine.rs src/diff/binary.rs
```

結果は `mutants: caught=180 survived=39 timeout=0 unviable=11 equivalent=0`（230 件、74 分）。スクリプトの終了コードは 1 で、kotowari mutants が見逃しを error として報告したためである（メモリ上限での停止ではない）。

### 実行の前の一覧

実行の前に、計画の `--re` とファイルで `cargo mutants --list --all-features`（テストを走らせない一覧の表示）をメモリ上限と低い CPU 優先度の中で実行したところ 230 件で、計画に挙げた関数は全て変異の名前に現れた（`ChangedFileBudget` は `ChangedFileBudget::skip_unread` と `ChangedFileBudget::unread` に当たった。`ChangedFileBudget::new` には変異がない）。正規表現は直していない。
構造体のフィールドを消す変異は一覧になかった。
`run_diff_partial_scan` は、走査の上限の[記録](./scan-limits-test-cleanup.md)のとおり execute_diff から到達しないため対象に含まれていない。

### 関数ごとの内訳（全体の実行）

一覧の件数は実行の前の `cargo mutants --list` の件数、survived は kotowari mutants の報告から数えた。
全体の実行の変異ごとの出力（`target/mutants-run/`）は、次の回し直しがスクリプトの決まりどおり出力先を空にしたため残っておらず、caught と unviable の関数ごとの内訳は記録できなかった。回し直した関数の内訳は下の回し直しの表にある。回し直していない関数は survived が 0 で、一覧の件数が caught と unviable の和である。

| ファイル | 関数 | 一覧の件数 | survived |
|---|---|---|---|
| src/cli/diff.rs | compute_statuses_and_resolve | 4 | 1 |
| src/cli/diff.rs | execute_diff | 74 | 25 |
| src/cli/diff.rs | read_file_bytes_tolerant | 7 | 1 |
| src/cli/diff.rs | run_diff_fast_path | 15 | 4 |
| src/cli/diff.rs | run_diff_full_scan | 1 | 0 |
| src/diff/binary.rs | compute_sha256 | 2 | 0 |
| src/diff/engine.rs | build_hunks | 16 | 1 |
| src/diff/engine.rs | compute_diff | 14 | 1 |
| src/diff/engine.rs | is_binary | 2 | 0 |
| src/diff/engine.rs | make_hunk | 5 | 0 |
| src/service/diff.rs | build_diff_output | 8 | 2 |
| src/service/diff.rs | build_masked_diff_output | 1 | 0 |
| src/service/diff.rs | convert_hunks | 13 | 0 |
| src/service/max_files.rs | ChangedFileBudget::skip_unread | 10 | 0 |
| src/service/max_files.rs | ChangedFileBudget::unread | 2 | 0 |
| src/service/max_files.rs | changes_without_reading | 15 | 1 |
| src/service/max_files.rs | limit_changed_files | 10 | 0 |
| src/service/max_files.rs | omitted_changed_files | 2 | 0 |
| src/service/output.rs | format_binary_hashes | 2 | 0 |
| src/service/output.rs | format_diff_text | 8 | 1 |
| src/service/output.rs | format_multi_diff_text | 8 | 1 |
| src/service/types.rs | DiffOutput::has_changes | 11 | 1 |

timeout はなかった。

### 負荷の下で落ちるテストだけに検知された変異

全体の実行で caught と数えられた変異のうち、ログで失敗したテストが tui_merge だけだったものが 2 件あった。変異を一時的に書き入れて、そのテストだけを `cargo nextest run --all-features --no-fail-fast --test tui_merge -E 'test(test_hunk_merge_left_to_right_with_l)'` で回し直すと、どちらも同じテストが落ちたため、負荷ではなく変異で落ちたものとして caught のまま扱う。確かめた後に `git checkout` で戻し、`git diff --stat src/` が空に戻ることを確かめた。

| 変異 | ログで失敗したテスト | 回し直しの結果 |
|---|---|---|
| src/diff/engine.rs:445:37 replace == with != in make_hunk（new_start の 0 の場合が反転する） | test_hunk_merge_left_to_right_with_l | 落ちた |
| src/diff/engine.rs:450:31 replace != with == in make_hunk（new_start の数え方が反転する） | test_hunk_merge_left_to_right_with_l | 落ちた |

src/diff/engine.rs の 205:46（compute_diff）・436:37・442:31（make_hunk）の変異は tests/tui_integration.rs のテストだけで落ちたが、このテストは端末を使わず状態と差分エンジンを呼ぶもので固定の待ち時間に頼らないため、負荷の下で落ちるテストに当たらないと判断した。agent_ssh だけに検知された変異は全体の実行にはなかった（回し直しでの 1 件は下に書く）。

### 見逃しを落とすために足したテスト

全体の実行の見逃しのうち、要件の観測で落とせるものにテストを足した（コミット b264c43・7ed8f93。テストを消す・書き換える変更と、製品のコードの変更はない）。
足したテストが見逃しの変異で落ちることは、変異（cargo-mutants が書き出した差分）を一時的に書き入れて `cargo nextest run --all-features --no-fail-fast --test contract -E 'test(/diff_(selection|format|file_kinds|max_files|output_cli)::/)'` を回して確かめ、どれも確かめた後に `git checkout` で戻して `git diff --stat src/` が空に戻ることを確かめた。

| 変異 | 行の中身 | 落とすテスト |
|---|---|---|
| src/cli/diff.rs:225:42 replace \|\| with && in execute_diff | 片側にだけ子のあるディレクトリを展開する条件 | req_cli_052_a_directory_on_one_side_compares_its_children（REQ-cli-052・060） |
| src/cli/diff.rs:687:38 replace \|\| with && in run_diff_fast_path | 同じ条件の FastPath の側 | 同上 |
| src/cli/diff.rs:489:55・494:57 replace \|\| with && in execute_diff | 読めた中身のない側にハッシュを出す条件 | req_cli_061_an_existing_empty_side_of_a_binary_has_a_hash（REQ-cli-061。ある側は "missing" にしない） |
| src/cli/diff.rs:549:50 replace != with == in execute_diff | ディレクトリの配下のバイナリを出す条件 | req_cli_061_a_changed_binary_under_a_directory_is_reported（REQ-cli-061・052） |
| src/diff/engine.rs:426:28 replace + with - in build_hunks | 新しい hunk の終わりの位置 | req_cli_058_distant_changes_are_shown_in_separate_hunks（REQ-cli-058。二つ目の hunk を一行だけの削除にした） |
| src/service/output.rs:276:14 replace > with >= in format_multi_diff_text | ファイルの間の区切りの改行 | req_cli_058_text_starts_with_the_first_file_header（REQ-cli-058） |

req_cli_058_nearby_changes_are_all_shown_with_their_context（文脈の範囲で近い二つの変更が一つの hunk に並ぶ）も同じ build_hunks の結合の場合を確かめるために足したが、426:28 はこのテストでは落ちなかった（結合の側の行は変異の行と別で、二つ目の変更の後の行で hunk の終わりが求め直されるため）。

src/cli/diff.rs:174:74（delete ! in execute_diff。読まずに数える機密ファイルを --force なしに限る条件）と src/service/types.rs:131:32（replace && with || in DiffOutput::has_changes。機密ファイルを変更のあるファイルに数える条件）は、最初は `tests/contract/diff_max_files.rs` の req_cli_055_forced_empty_sensitive_file_on_one_side_is_not_counted（左にだけ空の ".env" を置き、--force で指定する）で落としていた。
このテストは片側にだけある 0 バイトのファイルを変更なしと扱うこと（FLAG-cli-038）に頼っていたため、左右に中身の同じ ".env" を置く req_cli_055_forced_unchanged_sensitive_file_is_not_counted に置き換えた（パス "a.txt" と ".env"、--max-files 1、--force で、"a.txt" だけが数えられ打ち切らない）。
置き換えたテストはどちらの変異でも落ちない。二つの変異を一つずつ一時的に書き入れて上と同じ `cargo nextest run` を回すと、どちらも 37 件全てが通った（確かめた後に `git checkout` で戻し、`git diff --stat src/` が空に戻ることを確かめた）。
パスを指定する経路では左右で中身の同じファイルは比べる対象に入らず、ディレクトリを指定した場合も --force の変更のない ".env" は項目に出なかった（使い捨ての確認。テストのファイルは元に戻した）。
二つの変異で出力が変わるのは、中身を読むまで変更のないことが分からない機密ファイル（片側にだけある 0 バイトのファイル、FLAG-cli-041 のファイル、読めないファイル）を --force で比べる場合だけで、どれも FLAG の範囲に触れるため、下の「見逃しと決着」で FLAG の範囲として決着とした。

### テストを足した後の回し直し

[決定記録 A2](../decision/records/2026-09-29-mutation-rerun-and-load.md#A2) が認める回し直しとして、全体の実行で見逃しになった変異を含む関数だけに `--re` で絞って回し直し、結果を上の全体の実行の記録と合わせて読む。
実行したのはコミット 7ed8f93 で、作業ツリーに変更のない状態で行い、実行中は作業ツリーにも他の cargo のコマンドにも触れていない。

```sh
MUTANTS_JOBS=1 scripts/mutants.sh --re '(execute_diff|run_diff_fast_path|compute_statuses_and_resolve|read_file_bytes_tolerant|build_diff_output|format_diff_text|format_multi_diff_text|has_changes|changes_without_reading|compute_diff|build_hunks)' src/cli/diff.rs src/service/diff.rs src/service/output.rs src/service/types.rs src/service/max_files.rs src/diff/engine.rs
```

結果は `mutants: caught=144 survived=30 timeout=0 unviable=6 equivalent=0`（180 件、56 分）。スクリプトの終了コードは 1 で、kotowari mutants が見逃しを error として報告したためである（メモリ上限での停止ではない）。
180 件は全体の実行でのこの 11 の関数の件数の和と同じである。上の表の 9 件が caught に変わり、新しい見逃しはなかった。

| ファイル | 関数 | 件数 | caught | survived | unviable |
|---|---|---|---|---|---|
| src/cli/diff.rs | compute_statuses_and_resolve | 4 | 3 | 1 | 0 |
| src/cli/diff.rs | execute_diff | 74 | 51 | 20 | 3 |
| src/cli/diff.rs | read_file_bytes_tolerant | 7 | 6 | 1 | 0 |
| src/cli/diff.rs | run_diff_fast_path | 15 | 12 | 3 | 0 |
| src/diff/engine.rs | build_hunks | 16 | 15 | 0 | 1 |
| src/diff/engine.rs | compute_diff | 14 | 12 | 1 | 1 |
| src/service/diff.rs | build_diff_output | 8 | 5 | 2 | 1 |
| src/service/max_files.rs | changes_without_reading | 15 | 14 | 1 | 0 |
| src/service/output.rs | format_diff_text | 8 | 7 | 1 | 0 |
| src/service/output.rs | format_multi_diff_text | 8 | 8 | 0 | 0 |
| src/service/types.rs | DiffOutput::has_changes | 11 | 11 | 0 | 0 |

回し直しでは src/cli/diff.rs:718:21 replace && with || in run_diff_fast_path が agent_ssh_deploy の agent_ssh_read_files_roundtrip だけの失敗で caught と数えられた。変異を一時的に書き入れて全てのテストを `cargo nextest run --all-features --no-fail-fast` で回すと、tests/contract/cli_results.rs の different_binary_files_report_hashes_without_text_lines が落ち（ほかは通った）、負荷に頼らないテストで落ちるため caught のまま扱う（nextest が最初の失敗から少し進んで止まるため、ログには先に落ちたテストだけが残った）。確かめた後に `git checkout` で戻し、`git diff --stat src/` が空に戻ることを確かめた。

### 根拠テストを直した後の回し直し

フルレビューの指摘で根拠テストを直した（値の固定を外し、テストを置き換えた）後に、直したテストが確かめる関数に絞って回し直した（[決定記録 A2](../decision/records/2026-09-29-mutation-rerun-and-load.md#A2)）。最初の試みは負荷のため途中で止め、その結果は使っていない。
流すテストを絞る判断（[決定記録 2026-09-29-mutation-test-selection](../decision/records/2026-09-29-mutation-test-selection.md)）の後の設定（--lib・tests/contract・tests/cli_diff.rs、テストの並列 3、デバッグ情報なし）で、コミット 9c9ff66 の作業ツリーに変更のない状態で行い、実行中は作業ツリーにも他の cargo のコマンドにも触れていない。

```sh
MUTANTS_JOBS=1 scripts/mutants.sh --re '(execute_diff|has_changes|build_hunks|format_multi_diff_text)' src/cli/diff.rs src/service/types.rs src/diff/engine.rs src/service/output.rs
```

結果は `mutants: caught=84 survived=21 timeout=0 unviable=4 equivalent=0`（109 件、43 分）。スクリプトの終了コードは 1 で、kotowari mutants が見逃しを error として報告したためである（メモリ上限での停止ではない）。

| ファイル | 関数 | 件数 | caught | survived | unviable |
|---|---|---|---|---|---|
| src/cli/diff.rs | execute_diff | 74 | 51 | 20 | 3 |
| src/diff/engine.rs | build_hunks | 16 | 15 | 0 | 1 |
| src/service/output.rs | format_multi_diff_text | 8 | 8 | 0 | 0 |
| src/service/types.rs | DiffOutput::has_changes | 11 | 10 | 1 | 0 |

survived の 21 件は全て下の「見逃しと決着」の表にある。前の回し直しと比べて、src/cli/diff.rs:174:74 と src/service/types.rs:131:32 が見逃しに戻り（上の「見逃しを落とすために足したテスト」のとおりテストを置き換えたため）、src/cli/diff.rs:353:21 が caught と数えられた。

src/cli/diff.rs:353:21（replace || with && in execute_diff）は tests/contract/merge_paths.rs の a_directory_link_does_not_merge_its_children_into_a_different_kind_of_target だけの失敗で caught と数えられた（"linked" を飛ばしたのに "linked/file.txt" を書き込んだ）。変異を一時的に書き入れて、そのテストだけを `cargo nextest run --stress-count 30` で 30 回、同じ絞ったテスト全体を 3 回回すと、どれも通った。変異で落ちたものと確かめられないため survived として扱い、決着は前の回し直しの記録のまま（下の表の 353:21）とする。確かめた後に `git checkout` で戻し、`git diff --stat src/` が空に戻ることを確かめた。このテストが一度だけ落ちた原因は調べていない。

### 見逃しと決着

見逃しは、テストを足した後の回し直しの survived の 30 件と、根拠テストを置き換えたことで見逃しに戻った src/cli/diff.rs:174:74 と src/service/types.rs:131:32 の 2 件である。30 件は全てのテストを流した回し直しで survived のため、既存の単体テストを含む全てのテストで落ちていない。2 件は流すテストを絞った回し直しで survived で、全てのテストでは確かめていない（174:74 は前の全てのテストの実行で負荷の下で落ちる tui_merge のテストだけで caught と数えられていた。[決定記録 2026-09-29-mutation-test-selection の A1](../decision/records/2026-09-29-mutation-test-selection.md#A1)）。

#### 記録だけするもの（計画の区分に当てはまる）

| 位置 | 変異 | 行の中身 | 区分と引き継ぎ先 |
|---|---|---|---|
| src/cli/diff.rs:190:17 | replace \|\| with && in execute_diff | 左右のどちらかのパスが root_dir の外に出るか | root_dir の外（diff の次の回） |
| src/cli/diff.rs:198:38 | replace \|\| with && in execute_diff | root_dir の外のパスがリンクか | root_dir の外と symlink（diff の次の回） |
| src/cli/diff.rs:233:21 | replace \|\| with && in execute_diff | 右の実パスが祖先にあるか（循環の検出） | ディレクトリ symlink の循環（diff の次の回、REQ-cli-021） |
| src/cli/diff.rs:254:37 | replace += with *= in execute_diff | ディレクトリを展開した項目の数を数える | 比較全体に一度だけ当てる走査の上限（diff の次の回、REQ-cli-021） |
| src/cli/diff.rs:255:40 | replace > with == / replace > with >= in execute_diff（2 件） | 展開した項目の数が上限を超えたか | 同上 |
| src/cli/diff.rs:289:17・290:17 | replace \|\| with && in execute_diff（2 件） | リンクの連なりに機密ファイルがあるか | symlink の機密ファイル（diff の次の回） |
| src/cli/diff.rs:304:21 | delete match arm Ok(TargetPath::Symlink{real_path, ..}) in execute_diff | リンク先が root_dir の外か | symlink と root_dir の外（diff の次の回） |
| src/cli/diff.rs:353:21 | replace \|\| with && in execute_diff | ディレクトリ symlink の右の実パスが祖先にあるか | ディレクトリ symlink の循環（diff の次の回、REQ-cli-021） |
| src/cli/diff.rs:377:40 | replace > with == / replace > with >= in execute_diff（2 件） | ディレクトリ symlink を展開した項目の数が上限を超えたか | 走査の上限（diff の次の回、REQ-cli-021） |
| src/cli/diff.rs:394:35 | replace += with *= in execute_diff | ディレクトリ symlink を展開した項目を走査したファイルに数える | ディレクトリ symlink（diff の次の回） |
| src/cli/diff.rs:400:21 | replace \|\| with && in execute_diff | リンク先の右の中身がバイナリか | symlink（diff の次の回） |
| src/cli/diff.rs:426:38 | replace \|\| with && in execute_diff | リンク先の片側が読めないか | symlink（diff の次の回） |
| src/cli/diff.rs:440:17 | replace \|\| with && in execute_diff | リンク先の右の中身がバイナリか | symlink（diff の次の回） |
| src/cli/diff.rs:671:17 | replace \|\| with && in run_diff_fast_path | 左右のどちらかの解決したパスが root_dir の外か | root_dir の外（diff の次の回） |
| src/service/output.rs:155:12 | delete ! in format_diff_text | symlink のリンク先の文字列が左右で違うか | symlink（diff の次の回） |
| src/service/diff.rs:47:17 | delete match arm engine::DiffResult::Equal in build_diff_output | --ref の参照先と左が同じときの ref_hunks | --ref と ref_hunks（diff のその次の回） |
| src/service/diff.rs:68:36 | replace && with \|\| in build_diff_output | バイナリと symlink で競合を検出しない | 競合（diff のその次の回）。--ref なしでは競合の検出が空を返すため、差が出るのは --ref のときだけ（実装を読んだ判断） |
| src/diff/engine.rs:153:34 | replace \|\| with && in compute_diff | 左右のどちらかがバイナリか | diff の CLI では compute_diff を呼ぶ前に execute_diff がバイナリを判定するため到達せず、merge も先に判定する（src/service/merge_flow.rs）。差が出るのは --ref の参照先との比較（競合の検出）と TUI の差分表示（実装を読んだ判断）。引き継ぎ先は diff のその次の回（--ref と競合）と凍結中の TUI |
| src/cli/diff.rs:549:30 | replace && with \|\| in execute_diff | ディレクトリの配下のバイナリを、ハッシュが違うときだけ出す | 中身の同じバイナリの扱い（FLAG-cli-035）。変異では配下の中身の同じバイナリも "Binary files differ" として出る |
| src/cli/diff.rs:718:48 | replace \|\| with && in run_diff_fast_path | 指定した中身の同じファイルがバイナリか | 中身の同じバイナリの扱い（FLAG-cli-035）。中身が同じため左右のバイナリの判定は同じになり、差が出ない（実装を読んだ判断） |
| src/cli/diff.rs:875:8 | delete ! in compute_statuses_and_resolve | メタデータだけでは決まらないファイルの中身を比べるか | 状態の判定（REQ-cli-027 の範囲） |
| src/cli/diff.rs:174:74 | delete ! in execute_diff | 読まずに数える機密ファイルを --force なしに限る条件 | 片側にだけある中身のないファイル（FLAG-cli-038）。落とせるのは --force で片側にだけある 0 バイトの機密ファイルを指定する場合で、FLAG-cli-038 の扱いに頼るため（上の「見逃しを落とすために足したテスト」） |
| src/service/types.rs:131:32 | replace && with \|\| in DiffOutput::has_changes | 機密ファイルを変更のあるファイルに数える条件 | files_with_changes と --max-files が数えるもの（FLAG-cli-034）。変異では --force で中身を出した変更のない機密ファイルも数える。落とせる入力は上と同じく FLAG-cli-038 に頼る |

#### 利用者の判断で決着した見逃し

この表は記録を出した時点で決着していなかった見逃しで、状況の欄はその時点のものである。どれも下の「利用者の判断」で決着した。

| 位置 | 変異 | 行の中身 | 状況 |
|---|---|---|---|
| src/cli/diff.rs:98:24 | replace == with != in execute_diff | 末尾の "/" を取ると空になるパス（""・"/"・"//"）のうち "./" だけを "." にする（"./" は前の分岐で "." になるため、この比較が真になることはない） | 変異では ""・"/"・"//" が "." になる。使い捨ての確認（下の「不具合の疑い」の 1）で ""・"/"・"."・"./"・"//" の全てが同じ結果（ファイルなし、終了コード 0）になったため、出力は変わらないと見られる。ただしその結果自体が不具合の疑いのため、同等変異として登録せず未決着とする |
| src/cli/diff.rs:183:16 | delete ! in execute_diff | 読まずに数える対象を、展開した子でないパスに限る | 別の文脈のエージェントに落とすテストを書かせたところ、下の「不具合の疑い」の 2 の入力で落とせるという回答だった。そのテストは不具合の疑いのある挙動を固定するため足していない |
| src/cli/diff.rs:722:25 | replace != with == in run_diff_fast_path | 読まずに数えるための大きさを、変更のあるファイルだけに記録する | 同上（同じ入力で落とせる） |
| src/service/max_files.rs:32:56 | replace > with >= in changes_without_reading | 左右にあるファイルを読まずに数えるのは、どちらも 0 バイトより大きく大きさが違うとき | 同じエージェントの回答では、左が 0 バイトより大きいのに読めず（パーミッション 000 か 100MB 超）右が 0 バイトのとき、元のコードは読んで変更なしとし、変異は読まずに数えて打ち切りと報告する。読めないファイルの扱い（FLAG-cli-030・039）に触れるテストになるため足していない |
| src/cli/diff.rs:214:54 | replace == with != in execute_diff | 項目のステータスを探す（片側にだけあるときにない側の読み込みの警告を抑える） | 変異では片側にだけあるファイルのない側について標準エラーに "Warning: …: … (treating as empty)" が出る（実装を読んだ判断）。この警告を出さないことは IR にない。計画の区分のどれにも当てはまらない。利用者の判断の 4 で FLAG-cli-030 の範囲として決着し、片側にだけあるファイルのない側の "treating as empty" の警告は FLAG-cli-038 にも書かれているため、引き継ぎ先に FLAG-cli-038 を併記する |
| src/cli/diff.rs:965:16 | delete ! in read_file_bytes_tolerant | quiet でないときだけ読めない側の警告を出す | 変異では警告の出る場合と出ない場合が反転する（実装を読んだ判断）。上と同じく警告の出し方は IR になく、区分に当てはまらない |

同等変異の登録はしていない。

## 不具合の疑い（新しい FLAG の候補）

根拠テストと見逃しの調べで、次の三つの疑いが見つかった。どれも実装は直していない。

1. **"." を指定した diff が差分を出さない。** 使い捨ての確認（左右に中身の違う "a.txt" と "d/b.txt" を置き、execute_diff のパスに ""・"/"・"."・"./"・"//" の一つずつを渡した。確かめた後にテストのファイルを元に戻した）で、全てがファイルなし・終了コード 0 になった。パスなしでは同じ構成で 2 件が出て終了コード 1 になる。IR は "." を diff の root_dir 全体とは定めていないが（REQ-cli-052 はパスなしだけを定める）、走査の範囲の判定表 TBL-scan-001 は "." と "./" と空の値を root_dir 全体の走査とし、変更があるのに終了コード 0 を返す。
2. **--max-files の値によって、同じファイルが変更のあるファイルに数えられたり数えられなかったりする。** 読まずに数える判定（`changes_without_reading`）は大きさだけを見るが、大きさが違っても、先頭 8,192 バイトより後にだけ不正な UTF-8 を含むファイルは読むと置き換え文字に揃って変更なしになる。使い捨ての確認（左右に中身の違う "a.txt" と、先頭 8,192 バイトが "a" で後に左は 0xFF、右は置き換え文字の UTF-8 の 3 バイトが続く "p.txt"。パス "a.txt" と "p.txt"）で、--max-files 0 と 2 では打ち切らず files_with_changes が 1、--max-files 1 では truncated が true で changed_files_total が 2 になった。REQ-cli-055 の「変更のあるファイルを数える」と食い違う。読めないファイル・100MB を超えるファイルでも同じことが起きると別の文脈のエージェントは読んだ（未実行）。
3. **先頭 8,192 バイトの境界で多バイト文字が切れた正しい UTF-8 のテキストがバイナリになる。** 使い捨ての確認（"a" を 8,191 バイト並べた後に "あ" と改行と左右で違う行を置いた）で binary が true、hunks が空になった。`is_binary` が先頭 8,192 バイトの切り出しをそのまま UTF-8 として読むためで、REQ-cli-061 の「先頭 8,192 バイトに不正な UTF-8 を含む」に当たるかは文から決まらない。

## 要件の verification の見直し

REQ-cli-001・005・009・018・052 から 061 の verification は全て unit である。いずれも、左右に置くファイルの中身と種類、指定するパス、--max-lines・--max-files・--force の値という具体的な場面で出力と終了コードが決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）と判断した。
REQ-cli-061 のバイナリの判定は任意のバイト列について成り立つべき規則にも読めるが、判定は NUL・不正な UTF-8・8,192 バイトの境界という有限の場合で分かれ、場合ごとに具体的な中身で確かめられるため unit のままとする案とした。ただし上の不具合の疑いの 3 が決まるまで、境界の場合のテストは置いていない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-cli-001 | unit | 配下に変更のあるファイルを置いたディレクトリを JSON で指定した場面で、files の項目が決まる |
| REQ-cli-005 | unit | 機密ファイルを --force なしと --force ありで比べる二つの場面で、中身を出すかが決まる |
| REQ-cli-009 | unit | バイナリを比べる場面で、ハッシュを出してテキストの差分を出さないことが決まる |
| REQ-cli-018 | unit | JSON を指定したコマンドが成功する場面と失敗する場面で、標準出力が JSON として読めることが決まる |
| REQ-cli-052 | unit | パスなし・ファイルのパス・ディレクトリのパス（末尾の "/" の有無）の場面で、比べるファイルが決まる |
| REQ-cli-053 | unit | テキスト・バイナリ・機密ファイル・打ち切りの有無の場面で、JSON のキーが決まる |
| REQ-cli-054 | unit | 文脈と変更の行の並びと --max-lines の値の場面で、出る行と truncated が決まる |
| REQ-cli-055 | unit | 変更のあるファイルの数・パスの指定の仕方・--max-files の値の場面で、出すファイルと総数が決まる |
| REQ-cli-056 | unit | 差分のない場面・ある場面・エラーの場面で、終了コードが決まる |
| REQ-cli-057 | unit | 指定したパスの一部または全てが見つからない場面で、警告とエラーと終了コードが決まる |
| REQ-cli-058 | unit | 変更のあるファイルを比べる場面で、テキストの見出し・行・最後の行が決まる |
| REQ-cli-059 | unit | 変更のある機密ファイル（テキストとバイナリ）を --force なしで比べる場面で、隠し方が決まる |
| REQ-cli-060 | unit | 片側にだけ中身のあるテキストファイルがある場面で、全ての行が削除か追加になる |
| REQ-cli-061 | unit | NUL・不正な UTF-8・境界の後の NUL・片側にない・読めないという場面で、判定とハッシュの出し方が決まる |

## 利用者の判断

次の判断を求める。S3 はこの判断が書き足されるまで完了しない。

1. 不具合の疑いの 1（"." を指定した diff が差分を出さない）を新しい FLAG にするか。src/cli/diff.rs:98:24 の見逃しは、この扱いが決まるまで未決着とする。案: FLAG にし、98:24 はその FLAG の範囲として記録する。
2. 不具合の疑いの 2（--max-files の値で数え方が変わる）を新しい FLAG にするか、REQ-cli-055 の実装課題として直すか。src/cli/diff.rs:183:16・722:25 と src/service/max_files.rs:32:56 の見逃しは、この扱いが決まるまで未決着とする。案: FLAG にし、三つの見逃しはその FLAG の範囲として記録する（読めないファイルに触れる 32:56 は FLAG-cli-030・039 の範囲も併記する）。
3. 不具合の疑いの 3（境界で多バイト文字が切れたテキストがバイナリになる）を新しい FLAG にするか。案: FLAG にし、決まったときに REQ-cli-061 の境界の場合のテストを足す。
4. src/cli/diff.rs:214:54 と 965:16（片側にだけあるときや読めないときの標準エラーの警告の出し方）をどう扱うか。案: 警告の出し方は IR にないため、読めないファイルの扱い（FLAG-cli-030）の範囲として記録だけにする。代わりに、片側にだけあるファイルのない側について警告を出さないことを REQ-cli-060 の一部として要件にし、実行ファイルのテストを足す選び方もある。

利用者の判断（2026-09-29）:

1. "." などを渡した diff が差分を出さない件は、新しい FLAG とし、テスト整理の後に直す（[決定記録 A23](../decision/records/2026-09-29-adopt-diff-output.md#A23)、FLAG-cli-040）。src/cli/diff.rs:98:24 の見逃しは FLAG-cli-040 の範囲として決着とする。
2. --max-files 1 のときだけ数えが変わる件は、FLAG に残す（[決定記録 A24](../decision/records/2026-09-29-adopt-diff-output.md#A24)、FLAG-cli-041）。src/cli/diff.rs:183:16・722:25 と src/service/max_files.rs:32:56 の見逃しは FLAG-cli-041 の範囲として決着とする。
3. 境目で切れた多バイト文字がバイナリになる件は、FLAG に残す（[決定記録 A25](../decision/records/2026-09-29-adopt-diff-output.md#A25)、FLAG-cli-042）。REQ-cli-061 の境界の場合のテストは、この FLAG が決まったときに足す。
4. src/cli/diff.rs:214:54 と 965:16（警告の出し方）は、読めないファイルの扱い（FLAG-cli-030）の範囲として記録だけにする。

この判断でテストは変えないため、変異テストは回し直していない。決着していない見逃しと、verification の見直しの候補は残っていない。

## FLAG-cli-040 を直す前の確かめ直し（2026-09-30）

FLAG-cli-040 を直す前に、上の「不具合の疑い」の 1 と同じ製品コード（この記録を書いてから `src/` は変わっていない）で確かめ直すと、再現しなかった。
左右に中身の違う "a.txt" と "d/b.txt" を置き、execute_diff を呼ぶ契約テストでパスに "."・"./"・空の値・"/" の一つずつと、"." と "a.txt" を並べたものを渡すと、どれもパスなしと同じ JSON と終了コード 1 になった。
試験 SSH サーバに対して実行ファイルを起動し、パスに "."・"./"・空の値・"/"・"//" の一つずつを渡しても、どれもパスなしと同じテキストと終了コード 1 になった（使い捨ての確認。テストのファイルは元に戻した）。
先の確認で違う結果になった原因は分からない。

利用者の判断で、この挙動を REQ-cli-052 に書き足して FLAG-cli-040 を外した（[決定記録](../decision/records/2026-09-30-diff-root-dir-path.md#A1)）。根拠テストとして `tests/contract/diff_selection.rs` に req_cli_052_a_path_naming_the_root_dir_compares_the_whole_root_dir を足した。今の挙動を確かめるテストのため、失敗する段階はない。

FLAG-cli-040 の範囲として決着していた src/cli/diff.rs:98:24（replace == with != in execute_diff）は、同等変異として `.kotowari/mutants-equivalents.yaml` に登録した。変異で変わるのは ""・"/"・"//" が "." になるかどうかだけで、どれも root_dir を指すパスとして扱われる。別の文脈のエージェントに落とすテストを探させたが書けず（コードを読んだ判断）、変異を一時的に書き入れて diff の契約テスト 53 件を回すと全て通った。確かめた後に `git checkout` で戻し、`git diff --stat src/` が空に戻ることを確かめた。登録の後に最後の回し直しの結果を kotowari mutants で読むと `caught=84 survived=20 timeout=0 unviable=4 equivalent=1` になった。
