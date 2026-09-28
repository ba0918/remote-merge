# merge の書き込みの中身のテスト整理の記録

merge の書き込みの中身の取り込みで加えた要件（REQ-merge-019 から REQ-merge-022）の根拠テストを整えた過程の記録。
変異テストの結果、各要件の根拠にしたテスト、整理後の見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テストは全て既存要件の根拠だったため、この整理ではテストを消していない。

## 整理前の変異テスト

整理を始める前のコミット bb37889（作業ツリーに変更のない状態）で、次のコマンドを一度だけ実行した。
三つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/service/merge_flow.rs src/cli/tolerant_io.rs src/service/status.rs
```

全体の集計は `mutants: caught=133 survived=13 timeout=0 unviable=17 equivalent=0`（163 件、実行時間は約 37 分）。
スクリプトの終了コードは 1 だった。これは kotowari mutants が見逃しを error として報告したためで、メモリ上限による停止ではない（cargo-mutants は完了し、結果が読まれている）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/service/merge_flow.rs | 23 | 10 | 0 | 8 |
| src/cli/tolerant_io.rs | 8 | 0 | 0 | 0 |
| src/service/status.rs | 102 | 3 | 0 | 9 |

### 見逃し

位置は変異が入る行と列、変異は kotowari mutants の出力の文言のまま。
「決着の対象」は計画の区別による。src/service/merge_flow.rs の `execute_single_merge` の通常ファイルの経路と `copy_permissions`、src/cli/tolerant_io.rs の `fetch_contents_required`、src/service/status.rs の `verified_content_pairs`・`needs_explicit_file_compare`・`needs_merge_content_compare`・`refine_status_with_content` の見逃しを対象にする。
src/service/status.rs の他の関数、`fetch_contents_tolerant`、symlink の分岐・`check_source_exists`・`execute_deletions`・`validate_hunk_merge_target`・`execute_hunk_merge` の見逃しは記録だけする。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/service/merge_flow.rs:256:14 | replace > with >= in copy_permissions | 対象（読み込み元の権限が 0 のときに権限を付けるか） |
| src/service/merge_flow.rs:256:18 | replace && with \|\| in copy_permissions | 対象（同上） |
| src/service/merge_flow.rs:413:33 | replace \|\| with && in execute_hunk_merge | 記録だけ（変更のまとまりを選ぶマージの回で扱う） |
| src/service/merge_flow.rs:427:13 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:428:13 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:479:21 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:480:21 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:481:21 | delete field hunk_info from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:502:17 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:503:17 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/status.rs:143:37 | replace + with * in collect_all_file_paths | 記録だけ（status の規則。容量の見積もりだけに使う式） |
| src/service/status.rs:320:15 | replace && with \|\| in needs_merge_content_compare | 対象（--checksum のときに中身を読み比べる組の条件） |
| src/service/status.rs:323:61 | replace && with \|\| in needs_merge_content_compare | 対象（同上） |

`copy_permissions` の二件は、読み込み元がリモートの場合（FLAG-merge-002）、新規ファイルの権限（FLAG-merge-003）、権限の変更の失敗（FLAG-merge-004）のどれにも当たらない（読み込み元の権限の値が 0 のときだけ違いが出る）。
`fetch_contents_required` と `verified_content_pairs`・`needs_explicit_file_compare`・`refine_status_with_content` には見逃しがなかった。

### 変異と関係のないテストだけによる検知

検知した 133 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗から少し進んで止まるため、集めた名前は検知したテストの全てではなく、先に失敗したものである。

負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）だけで検知された変異は次の一件だった。

| 位置 | 変異 | 失敗したテスト | 扱い |
|---|---|---|---|
| src/service/status.rs:137:5 | replace path_is_within_unloaded_dir -> bool with true | tui_merge の test_hunk_merge_left_to_right_with_l、test_hunk_merge_right_to_left_with_h_key、test_merge_cancel_with_n、test_sensitive_file_merge_requires_confirmation | status の規則で決着の対象ではない。整理後の比較では、この変異を見逃しの候補に含めて比べる |

この一件は決着の対象の関数ではないため、見かけの検知かどうかは確かめていない。

## 要件ごとの根拠テスト

根拠テストは `tests/contract/merge_write.rs` にある。手本にした元のテスト（`tests/contract/merge_paths.rs`、`tests/cli_merge.rs`）は消さず、書き換えていない。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。

準備は `tests/contract/merge_support.rs` の `Fixture` を使い、書き込み先 develop を `RuntimeTargets::with_local` で一時ディレクトリに差し替えて `execute_merge` と `execute_sync` を関数呼び出しで呼ぶ。
結果は --format json と同じ `format_json` で JSON にして確かめる。バックアップは無効にした構成（`fixture`）を使う。
sync の呼び出しに足した `Fixture::sync_json` と `sync_args` は --force を付ける（--force のない sync は書き込む予定があると確認のプロンプトで標準入力を読むため）。
配下のディレクトリの作成（`Fixture::write` は親ディレクトリを作らない）、更新時刻の読み書き、読めないファイルの作成の補助も `Fixture` に足した。
読めないファイルは権限を 0o200 に落として作り、開けないことを先に確かめる（root で実行されると読めてしまい、前提が崩れたことが分かるようにする）。

### 読めなかった側と原因の示し方（REQ-merge-019）

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-merge-019（merge） | merge_reports_which_side_could_not_be_read | tests/contract/merge_paths.rs の explicit_merge_checks_readability_even_when_file_sizes_differ・unreadable_source_fails_one_file_without_blocking_the_other_merge・two_unreadable_sides_are_not_reported_as_identical_empty_files。元のテストは error が "read failed:" で始まることだけを見る。一回の merge で三つのファイルを明示し、読み込み元だけが読めない source-locked.txt の error が "read failed: left: " で始まり "right: " と "; " を含まないこと、書き込み先だけが読めない destination-locked.txt の error が "read failed: right: " で始まり "left: " と "; " を含まないこと、両側が読めない both-locked.txt の error が "read failed: left: " で始まりその後に "; right: " が続くことを確かめる。原因の文言は確かめない |
| REQ-merge-019（sync） | sync_reports_which_side_could_not_be_read | 同じ三件と tests/contract/merge_paths.rs の sync_does_not_overwrite_a_destination_it_cannot_read。同じ構成を --force の sync で確かめる |

- どのファイルも左右で中身の長さを変えた（ファイルを明示したときは長さによらず中身を読み比べる）。
- 書き込み先を読めないが中身を読み比べない場合の error（FLAG-merge-005）は構成に含めない。ファイルを明示した読み比べだけを使う。

### ディレクトリ指定の既定の比べ方（REQ-merge-020）

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-merge-020（merge） | directory_merge_without_checksum_writes_only_files_the_metadata_and_contents_show_as_changed | tests/contract/merge_paths.rs の directory_sync_without_checksum_uses_the_metadata_quick_check（sync で、サイズと更新時刻が同じで中身が違うファイルが書かれないことだけを見る）。--checksum のない folder の merge で、merged がちょうど folder/control.txt の一件で、書かれない二つのファイルの中身と更新時刻が変わらないことを確かめる |
| REQ-merge-020（sync） | directory_sync_without_checksum_writes_only_files_the_metadata_and_contents_show_as_changed | 同じ元のテスト。同じ構成を --force の sync で確かめる |

- 構成: folder の下に、サイズと更新時刻が同じで中身が違う same-metadata.txt（書き込み先の更新時刻を `File::set_modified` で読み込み元に揃える）、サイズと中身が同じで書き込み先の更新時刻を 120 秒前にずらした same-bytes.txt、左右で中身の長さが違う対照の control.txt を置く。呼ぶ前に、秒単位の更新時刻が same-metadata.txt では同じで same-bytes.txt では 60 秒以上違うことを確かめる。
- merge の結果が per-file の出力（`MergeCommandOutput::Files`）になることは `Fixture::merge_json` が確かめる（それ以外の出力では落ちる）。書き込むものがないときの出力は確かめない。
- 一時的な書き換え（コミットに含めない）で、`needs_merge_content_compare` が常に --checksum のときの組を読み比べるようにすると、この二件が落ちることを確かめた。REQ-merge-019 の二件は、読めなかった側をつなぐ "; " を別の文字に変える書き換えで落ちることを確かめた。書き換えは元に戻した。

### ディレクトリ指定と複数パスの書き込み（REQ-merge-021、REQ-merge-022）

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-merge-021 | directory_merge_overwrites_changed_files_creates_source_only_files_and_skips_equal_files | tests/cli_merge.rs の test_merge_directory（配下の二つのファイルが上書きされることだけを見る）。folder の merge で、左右で中身の長さが違う changed.txt が上書きされ、読み込み元にだけある new.txt が書き込み先に作られ、中身が同じ same.txt が merged に出ず中身と更新時刻が変わらないこと、merged がちょうどこの二件で、failed が空で終了コードが 0 であることを確かめる |
| REQ-merge-022（二つのパス） | a_merge_of_two_paths_writes_each_of_them | tests/cli_merge.rs の test_merge_multiple_files。a.txt と b.txt を指定した merge で、merged がちょうどこの二件で、両方の中身が読み込み元に揃い、終了コードが 0 であることを確かめる |
| REQ-merge-022（同じパスの重ね指定） | a_path_given_twice_is_written_once | tests/cli_merge.rs の test_merge_duplicate_paths_deduplicated（標準出力の "Merged:" の数を見る）。a.txt を二度指定した merge で、merged にちょうど一度だけ出て、failed が空で、終了コードが 0 であることを確かめる。failed の error の文言は確かめない |

- REQ-merge-021 の構成には書き込み先にだけあるファイルを含めない（その扱いは symlink と削除の回の範囲）。
