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
| REQ-cli-052 | req_cli_052_without_paths_every_changed_file_in_the_root_dir_is_compared | パスなしで、root_dir の直下と "d/" の下の変更のあるファイルが出て、変更のない "same.txt" は出ない |
| REQ-cli-052 | req_cli_052_file_paths_compare_only_those_files | 変更のあるファイル 3 件のうち 2 件のパスを指定すると、その 2 件だけが出る |
| REQ-cli-052 | req_cli_052_directory_path_compares_its_children_with_or_without_a_slash | "d/" の指定で "d/" の下（孫を含む）の変更のあるファイルだけが出て、ほかのディレクトリと直下のファイルは出ない。"d" の指定と JSON の文字列と終了コードが一致する |
| REQ-cli-001 | req_cli_001_directory_json_has_a_structured_entry_for_each_changed_file | ディレクトリを指定した JSON の files が配下の 2 件の項目を持ち、それぞれの hunks の lines が削除と追加の行になる |
| REQ-cli-053 | req_cli_053_json_has_files_and_summary_and_each_file_has_the_documented_keys | 打ち切らない JSON の上位のキーが files と summary だけで（truncated・changed_files_total・errors がない）、テキストの項目のキーが path・left・right・sensitive・truncated・hunks だけで（binary・left_hash・right_hash・note がない）、left と right の label と root、hunks の index・left_start・right_start が数であること、lines の type が "context"・"removed"・"added" の三つになり content が行の中身であること |
| REQ-cli-053 | req_cli_053_binary_hashes_and_note_appear_only_on_the_files_they_apply_to | テキスト・バイナリ・機密ファイルを並べて比べ、binary・left_hash・right_hash がバイナリの項目だけに、note が機密ファイルの項目だけに出る。errors が出ない |
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
標準エラーの警告と、main.rs が出すエラー（テキストでは標準エラーの "Error: …"、JSON では標準出力の {"error": ...}）と終了コード 2 は関数呼び出しでは観測できないため、実行ファイルを試験 SSH サーバに対して `diff --left local --right develop` で起動する。パスを 1〜20 個指定する経路（`run_diff_fast_path`）を通る。
起動の組み方は `tests/contract/scan_listing_cli.rs` の `launch_status` と同じで（`--config` に一時ディレクトリの設定を渡し、作業ディレクトリを一時ディレクトリの下にし、`env_clear` のうえ HOME・XDG の変数を一時ディレクトリに向けて PATH だけを引き継ぐ）、その補助は status に固定されているため、diff 用の `launch_diff` を同じ形で新しいモジュールに書いた。起動の前に `TestDirs::assert_isolated_config_at` の隔離の確認を通す。
場合は `tests/cli_diff_general.rs` の test_diff_nonexistent_file を手本にした。

| 要件 | 根拠テスト | 確かめること |
|---|---|---|
| REQ-cli-057 | req_cli_057_a_missing_path_is_warned_and_the_rest_are_compared | 変更のある "a.txt" と見つからない "missing.txt" を指定すると、標準エラーに "Warning: 'missing.txt' not found on either side" が出て（"a.txt" の警告とエラーは出ない）、標準出力に "a.txt" の差分が出て、終了コードが 1 になる |
| REQ-cli-057・056 | req_cli_057_every_path_missing_is_an_error_on_stderr_in_text | 見つからない 2 件を指定したテキストで、標準エラーに 2 件それぞれの警告と "Error: specified path(s) not found on either side" の行が出て、標準出力が空で、終了コードが 2 になる |
| REQ-cli-057・056 | req_cli_057_every_path_missing_is_a_json_error_on_stdout | 見つからない 1 件を指定した JSON で、標準出力が {"error": "specified path(s) not found on either side"} に一致し、標準エラーに警告が出て、終了コードが 2 になる |
