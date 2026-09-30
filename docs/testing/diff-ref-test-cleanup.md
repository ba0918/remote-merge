# diff の参照先と競合のテスト整理の記録

diff の --ref の参照先と競合の取り込みで加えた要件（REQ-cli-062 から REQ-cli-066）と、既存の要件 REQ-cli-016 の根拠テストを整理した過程の記録。
変異テストの結果、各要件の根拠にしたテスト、削除候補と利用者の判断、整理後の見逃しの決着を残す。

## 整理前の変異テスト

整理を始める前のコミット b5355c6（作業ツリーに変更のない状態）で、次のコマンドを実行した。
参照先と競合の規則を持つ関数に絞り、四つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh \
  --re ' in (build_diff_output|detect_conflicts|extract_changes|merge_overlapping_regions|merge_ranges|validate_ref_side|resolve_ref_source)$' \
  --re 'replace (build_diff_output|detect_conflicts|extract_changes|merge_overlapping_regions|merge_ranges|validate_ref_side|resolve_ref_source) -> ' \
  src/service/diff.rs src/diff/conflict.rs src/cli/ref_guard.rs src/service/source_pair.rs
```

回す前に同じ `--re` を付けた `cargo mutants --list --all-features --file src/service/diff.rs --file src/diff/conflict.rs --file src/cli/ref_guard.rs --file src/service/source_pair.rs` で件数を確かめ、計画と同じ 93 件だった。
構造体のフィールドを消す変異は混ざらなかった。

全体の集計は `mutants: caught=57 survived=19 timeout=4 unviable=13 equivalent=0`（93 件、実行時間は約 26 分）。
スクリプトの終了コードは 1 で、これは cargo-mutants が最後まで走った後に kotowari mutants が見逃しを error として返したもの（93 件の全てに結果がある）。
一度目の起動は、呼び出し側の待ち時間の設定の誤りに気づいて数秒で止め、結果を読まずに捨てた。上の集計は二度目の起動のもの。
関数ごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | 関数 | caught | survived | timeout | unviable |
|---|---|---|---|---|---|
| src/service/diff.rs | build_diff_output | 5 | 2 | 0 | 1 |
| src/diff/conflict.rs | detect_conflicts | 17 | 13 | 0 | 0 |
| src/diff/conflict.rs | extract_changes | 28 | 1 | 4 | 3 |
| src/diff/conflict.rs | merge_overlapping_regions | 2 | 2 | 0 | 0 |
| src/diff/conflict.rs | merge_ranges | 0 | 1 | 0 | 7 |
| src/cli/ref_guard.rs | validate_ref_side | 3 | 0 | 0 | 1 |
| src/service/source_pair.rs | resolve_ref_source | 2 | 0 | 0 | 1 |

タイムアウトの四件は全て src/diff/conflict.rs の extract_changes の `replace += with *=`（120・129・162・180 行）で、添字が進まずに止まらなくなる変異である。

### 見逃し

位置は変異が入る行、変異は kotowari mutants の出力の文言のまま。
「決着の対象」は計画の区別による。FLAG-cli-058 から 070 に当たる見逃しはその FLAG の範囲として記録だけにする。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/diff/conflict.rs:153 | replace + with * in extract_changes | 対象 |
| src/diff/conflict.rs:219 | replace && with \|\| in detect_conflicts | 対象 |
| src/diff/conflict.rs:224 | replace && with \|\| in detect_conflicts | 対象 |
| src/diff/conflict.rs:224 | replace < with <= in detect_conflicts | 対象 |
| src/diff/conflict.rs:224 | replace < with == in detect_conflicts | 対象 |
| src/diff/conflict.rs:224 | replace < with > in detect_conflicts | 対象 |
| src/diff/conflict.rs:224 | replace >= with < in detect_conflicts | 対象 |
| src/diff/conflict.rs:225 | replace == with != in detect_conflicts | 対象 |
| src/diff/conflict.rs:227 | replace && with \|\| in detect_conflicts | 対象 |
| src/diff/conflict.rs:227 | replace < with <= in detect_conflicts | 対象 |
| src/diff/conflict.rs:227 | replace < with == in detect_conflicts | 対象 |
| src/diff/conflict.rs:227 | replace < with > in detect_conflicts | 対象 |
| src/diff/conflict.rs:227 | replace >= with < in detect_conflicts | 対象 |
| src/diff/conflict.rs:230 | replace < with <= in detect_conflicts | 対象 |
| src/diff/conflict.rs:265 | replace < with <= in merge_overlapping_regions | 対象 |
| src/diff/conflict.rs:265 | replace < with == in merge_overlapping_regions | 対象 |
| src/diff/conflict.rs:286 | replace merge_ranges -> Option<Range<usize>> with None | 対象 |
| src/service/diff.rs:47 | delete match arm engine::DiffResult::Equal in build_diff_output | 対象（参照先との差の組み立て） |
| src/service/diff.rs:68 | replace && with \|\| in build_diff_output | 対象外（バイナリと symlink の競合。FLAG-cli-059・063 の範囲） |

- detect_conflicts の 219 行から 227 行は、左右の一方か両方が挿入だけ（参照先の行を消さない変更）のときに重なりを決める分岐で、整理前のテストは両方が同じ位置に挿入する場合と空の参照先に両方が足す場合しか通らない。230 行は両方が参照先の行を変えるときの重なりの判定で、`<=` にすると隣り合うだけの変更も重なりと見る。
- src/diff/conflict.rs:153 は挿入だけの変更の位置（直前の参照先の行の次）の計算である。
- merge_overlapping_regions と merge_ranges は、重なる競合の範囲を一つにまとめる処理である。merge_ranges の結果は "conflict_regions" の要素の中の TUI 用の範囲に入り、要素の形は FLAG-cli-058 の範囲のため、決着の仕方は整理後に判断する。
- src/service/diff.rs:47 は、左と参照先が同じときに "ref_hunks" を空の配列にする分岐で、この腕を消しても後ろの `_ => Some(vec![])` の腕が同じ値を返すため、同等変異の候補である。

## 要件ごとの根拠テスト

根拠テストは全て `tests/contract/diff_ref_cli.rs` にある。
標準エラーの警告、main.rs が出すエラーと終了コード、テキストの出力は関数呼び出しでは観測できないため、全て実行ファイルを試験 SSH サーバに対して起動して確かめる。
構成は `CliEnv::new_3way` の local・develop・staging の三つで、左右と参照先は引数で選ぶ。サーバ名の参照先は左 local・右 develop・参照先 staging、"local" の参照先は左 develop・右 staging・参照先 local で使う。
実行ファイルは作業ディレクトリの ".remote-merge.toml" で設定を上書きするため、作業ディレクトリは一時ディレクトリの下にする。
このモジュールは SSH fixture を使うため、ほかの SSH のテストと同じく `test-utils` の feature があるときだけ組み込む。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。
参照先には機密ファイルも symlink も置かない（FLAG-cli-063・070）。

| 要件 | 根拠テスト | 確かめること |
|---|---|---|
| REQ-cli-062（設定のサーバ名） | req_cli_062_a_configured_server_as_ref_makes_a_three_way_diff | `--ref staging` で JSON の "ref" の "label" が "staging" |
| REQ-cli-062（"local"） | req_cli_062_local_as_ref_makes_a_three_way_diff | 左 develop・右 staging で `--ref local` の "ref" の "label" が "local" |
| REQ-cli-062（設定にないサーバ名） | req_cli_062_an_unknown_ref_server_is_an_error_with_exit_code_two | 標準エラーに "Server 'nonexistent' not found in config"、終了コード 2 |
| REQ-cli-063（"ref" と "ref_hunks"） | req_cli_063_json_has_the_ref_and_the_diff_from_left_to_ref | "ref" の "label" と "root"、"ref_hunks" に左の行の削除と参照先の行の追加。"root" の形は IR が定めていないため、参照先のディレクトリを含むことだけを見る |
| REQ-cli-063（左と参照先が同じ） | req_cli_063_ref_hunks_are_empty_when_left_equals_the_ref | "ref_hunks" が空の配列 |
| REQ-cli-063（参照先のファイルを読めない） | req_cli_063_only_the_ref_is_shown_when_the_ref_file_cannot_be_read | 参照先にそのパスがないとき "ref" だけで "ref_hunks" がない。読めない理由の違いは FLAG-cli-062 のため確かめない |
| REQ-cli-063（--ref がない） | req_cli_063_without_ref_json_has_neither_ref_nor_ref_hunks | "ref" も "ref_hunks" もない |
| REQ-cli-064（参照先との差が空でない） | req_cli_064_text_shows_the_ref_diff_after_the_left_right_diff | 左右の差の行、"--- ref:staging:differs.txt (reference diff vs left)"、hunk の見出し "@@"、参照先の行の順に出る |
| REQ-cli-064（参照先との差が空） | req_cli_064_text_has_no_ref_section_when_left_equals_the_ref | 左右の差は出て、"--- ref:" の見出しは出ない |
| REQ-cli-065・REQ-cli-016（JSON） | req_cli_065_json_counts_and_locates_conflicts | 競合のある五つのファイル（一行の競合、離れた二か所の競合、一方が消し他方が変えた行、範囲の一部だけが重なる変更で左の範囲が右の範囲を含む組と右の範囲が左の範囲を含む組）で "conflict_count" が 1・2・1・1・1、"conflict_regions" が空でない。要素の中身は FLAG-cli-058 のため見ない |
| REQ-cli-065・REQ-cli-016（テキスト） | req_cli_065_text_states_the_conflicts_of_each_file_and_the_total_at_the_end | 同じ五つのファイルで、各ファイルの見出し "--- a/パス (local)"（REQ-cli-058）から次のファイルの見出しまでの区間に、そのファイルの数の "Conflicts: N region(s) …" の行が一つだけあり、空行を除いた最後の行が "6 conflict(s) detected across files"。ファイルの並び順は IR が定めないため固定しない |
| REQ-cli-065・REQ-cli-016（競合がない） | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text | 左右が別々の行を変えたファイル、同じ行を同じ内容に変えてほかの行で左右が違うファイル、同じ行を消してほかの行で左右が違うファイル、隣り合う行をそれぞれが変えたファイル、一方が行を足しただけで他方の変えた行から離れているファイル三つ（左が他方の変えた行の前に足す組と後ろに足す組、右が前に足す組）の七つ（どれも左右に差があり出力に出る）で、JSON に "conflict_count" も "conflict_regions" もなく、テキストに "Conflicts: " も "conflict(s) detected across files" もない |
| REQ-cli-066 | req_cli_066_a_ref_equal_to_either_side_warns_and_compares_without_the_ref | 左 local・右 develop で `--ref local` と `--ref develop` のそれぞれで警告の文言が標準エラーに出て、JSON に "ref" も "ref_hunks" もなく、左右の差の "hunks" が出る |

- ファイルが一つのときに "N conflict(s) detected across files" が出るかは IR が定めないため確かめない。
- 挿入だけの変更（参照先の行を消さずに行を足す変更）どうしや、挿入と行の変更が重なるかは、用語「競合」の「変更の行の範囲が重なる」が空の範囲について定めていないため、根拠テストで固定しない。
- --ref がないときのテキストに参照先の見出しが出ないことと、--ref がないときに競合の項目が出ないことは、REQ-cli-064・065 が --ref を指定した diff についてだけ定めるため確かめない。

### 取り込みで根拠とした 39 件の審査

tests/contract/cli_results.rs の二件は変えていない。
`three_way_diff_reports_conflicting_edits_to_the_same_line`（EX-cli-031）は関数呼び出しで競合の数と領域があることを見るもので、EX-cli-031 の印のまま残す。REQ-cli-016 の印は足さない（上の三件が REQ-cli-016 の印を持つ）。
`identical_changes_on_both_sides_have_no_three_way_conflict`（EX-cli-032）は左右が同じ内容でファイルが出力に出ない（FLAG-cli-065）ため、REQ-cli-016 の根拠に数えない。EX-cli-032 の印のまま残す。

残りの 37 件（置き換え元）は移し元に残した。それぞれを置き換えた根拠テストは次のとおり。「なし」は入口を通す根拠テストで置き換えていないもので、理由を書いた。

| ファイル | テスト | 置き換えた根拠テスト |
|---|---|---|
| src/service/diff.rs | test_ref_content_produces_ref_hunks | req_cli_063_json_has_the_ref_and_the_diff_from_left_to_ref |
| src/service/diff.rs | test_ref_content_same_as_left_produces_empty_ref_hunks | req_cli_063_ref_hunks_are_empty_when_left_equals_the_ref |
| src/service/diff.rs | test_ref_content_none_produces_none_ref_hunks | req_cli_063_only_the_ref_is_shown_when_the_ref_file_cannot_be_read |
| src/service/diff.rs | test_no_ref_backward_compat | req_cli_063_without_ref_json_has_neither_ref_nor_ref_hunks |
| src/service/diff.rs | test_conflict_count_with_ref | req_cli_065_json_counts_and_locates_conflicts |
| src/service/diff.rs | test_conflict_count_without_ref | なし。--ref がないときの競合の項目は REQ-cli-065 が定めない |
| src/diff/conflict.rs | test_basic_conflict | req_cli_065_json_counts_and_locates_conflicts（one.txt）。左右の行の中身は FLAG-cli-058 のため見ない |
| src/diff/conflict.rs | test_one_sided_change_no_conflict | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text（disjoint.txt。どの行も一方だけが変える） |
| src/diff/conflict.rs | test_both_same_change_no_conflict | 同上（same_change.txt） |
| src/diff/conflict.rs | test_multi_line_conflict | req_cli_065_json_counts_and_locates_conflicts（overlapping.txt。複数の行にまたがる変更の競合が一つ）。範囲と行の中身は FLAG-cli-058 のため見ない |
| src/diff/conflict.rs | test_separate_conflicts | req_cli_065_json_counts_and_locates_conflicts（two.txt） |
| src/diff/conflict.rs | test_delete_vs_modify_conflict | req_cli_065_json_counts_and_locates_conflicts（delete_vs_modify.txt） |
| src/diff/conflict.rs | test_both_delete_same_line_no_conflict | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text（same_delete.txt） |
| src/diff/conflict.rs | test_no_ref_returns_empty | なし。--ref がないときの競合の項目は REQ-cli-065 が定めない |
| src/diff/conflict.rs | test_all_identical_no_conflicts | なし。三つが同じファイルは出力に出ないため、入口から競合の有無を観測できない |
| src/diff/conflict.rs | test_empty_files | なし。同上（左右とも空のファイルは出力に出ない） |
| src/diff/conflict.rs | test_insert_conflict_both_insert_different | なし。挿入だけの変更どうしの重なりは IR が定めない |
| src/diff/conflict.rs | test_ref_empty_both_add_different | なし。同上（空の参照先への追加は挿入だけの変更） |
| src/diff/conflict.rs | test_overlapping_range_conflict | req_cli_065_json_counts_and_locates_conflicts（overlapping.txt） |
| src/service/output.rs | test_format_diff_text_with_ref_hunks | req_cli_064_text_shows_the_ref_diff_after_the_left_right_diff |
| src/service/output.rs | test_format_diff_text_no_ref_backward_compat | なし。--ref がないときのテキストは REQ-cli-064 が定めない。参照先との差がないときに見出しを出さない分岐は req_cli_064_text_has_no_ref_section_when_left_equals_the_ref が通る |
| src/service/output.rs | test_format_diff_text_with_conflicts | req_cli_065_text_states_the_conflicts_of_each_file_and_the_total_at_the_end |
| src/service/output.rs | test_format_diff_text_no_conflicts | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text |
| src/service/output.rs | test_format_multi_diff_text_with_conflicts | req_cli_065_text_states_the_conflicts_of_each_file_and_the_total_at_the_end |
| src/service/output.rs | test_format_multi_diff_text_no_conflicts | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text |
| src/service/output.rs | test_diff_output_conflict_count_zero_omitted_in_json | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text |
| src/service/output.rs | test_diff_output_conflict_count_nonzero_in_json | req_cli_065_json_counts_and_locates_conflicts |
| src/cli/ref_guard.rs | ref_same_as_left_returns_none | req_cli_066_a_ref_equal_to_either_side_warns_and_compares_without_the_ref |
| src/cli/ref_guard.rs | ref_same_as_right_returns_none | 同上 |
| src/cli/ref_guard.rs | ref_different_returns_some | req_cli_062_a_configured_server_as_ref_makes_a_three_way_diff |
| src/cli/ref_guard.rs | ref_none_returns_none | req_cli_063_without_ref_json_has_neither_ref_nor_ref_hunks |
| src/service/source_pair.rs | test_resolve_ref_source_remote | req_cli_062_a_configured_server_as_ref_makes_a_three_way_diff |
| src/service/source_pair.rs | test_resolve_ref_source_local | req_cli_062_local_as_ref_makes_a_three_way_diff |
| src/service/source_pair.rs | test_resolve_ref_source_nonexistent | req_cli_062_an_unknown_ref_server_is_an_error_with_exit_code_two |
| src/service/source_pair.rs | test_resolve_ref_source_none | req_cli_063_without_ref_json_has_neither_ref_nor_ref_hunks |
| src/service/source_pair.rs | test_resolve_ref_source_same_as_left | req_cli_066_a_ref_equal_to_either_side_warns_and_compares_without_the_ref（左右と同じ名前をエラーにせず、警告して参照先なしで続ける） |
| tests/cli_diff_general.rs | test_diff_with_ref | req_cli_062_local_as_ref_makes_a_three_way_diff と req_cli_064_text_shows_the_ref_diff_after_the_left_right_diff |

## 削除候補と利用者の判断

整理の計画で削除を利用者が一括で判断する段の入力。
利用者の返答（消すものの一覧）を下の「判断の結果」に書き足してから削除する。

一覧は、上の審査の置き換え元の 37 件を、入口を通す根拠テストで置き換えたもの（A、30 件）と、置き換えていないもの（B、7 件）に分けたもの。
「代わりの根拠」は、そのテストが確かめていた振る舞いを今確かめている根拠テスト（全て tests/contract/diff_ref_cli.rs）。

- src/cli/ref_guard.rs の候補（4 件）は、status の整理と merge の整理でも根拠にしたテストである。
- src/service/source_pair.rs の候補（5 件）は、status の整理でも根拠にしたテストである。
- src/service/output.rs の候補と tests/cli_diff_general.rs の test_diff_with_ref は、変異テストの裏付けがない。src/service/output.rs は変異テストの対象外で、tests/cli_diff_general.rs は `scripts/mutants.sh` が変異ごとに流すテストに入らないため、消しても整理後の変異テストで見逃しの増減として現れない。該当する行に「裏付けなし」と書いた。

候補にしていないもの:

- FLAG の挙動のテスト（src/diff/conflict.rs の test_conflict_info_serialization〈FLAG-cli-058〉、src/service/diff.rs の test_max_lines_applied_independently_to_ref_hunks〈FLAG-cli-061〉と test_conflict_count_binary_is_zero〈FLAG-cli-059〉）。
- src/diff/conflict.rs の TUI 用の行の判定の 9 件と `compute_conflict_if_complete` の 6 件。
- tests/contract/cli_results.rs の EX-cli-031・032 のテスト。

### A. 入口を通す根拠テストに置き換えた単体テスト（30 件）

| ファイル | テスト | 代わりの根拠 |
|---|---|---|
| src/service/diff.rs | test_ref_content_produces_ref_hunks | req_cli_063_json_has_the_ref_and_the_diff_from_left_to_ref |
| src/service/diff.rs | test_ref_content_same_as_left_produces_empty_ref_hunks | req_cli_063_ref_hunks_are_empty_when_left_equals_the_ref |
| src/service/diff.rs | test_ref_content_none_produces_none_ref_hunks | req_cli_063_only_the_ref_is_shown_when_the_ref_file_cannot_be_read |
| src/service/diff.rs | test_no_ref_backward_compat | req_cli_063_without_ref_json_has_neither_ref_nor_ref_hunks |
| src/service/diff.rs | test_conflict_count_with_ref | req_cli_065_json_counts_and_locates_conflicts |
| src/diff/conflict.rs | test_basic_conflict | req_cli_065_json_counts_and_locates_conflicts（one.txt） |
| src/diff/conflict.rs | test_one_sided_change_no_conflict | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text（disjoint.txt） |
| src/diff/conflict.rs | test_both_same_change_no_conflict | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text（same_change.txt） |
| src/diff/conflict.rs | test_multi_line_conflict | req_cli_065_json_counts_and_locates_conflicts（overlapping.txt） |
| src/diff/conflict.rs | test_separate_conflicts | req_cli_065_json_counts_and_locates_conflicts（two.txt） |
| src/diff/conflict.rs | test_delete_vs_modify_conflict | req_cli_065_json_counts_and_locates_conflicts（delete_vs_modify.txt） |
| src/diff/conflict.rs | test_both_delete_same_line_no_conflict | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text（same_delete.txt） |
| src/diff/conflict.rs | test_overlapping_range_conflict | req_cli_065_json_counts_and_locates_conflicts（overlapping.txt） |
| src/service/output.rs | test_format_diff_text_with_ref_hunks | req_cli_064_text_shows_the_ref_diff_after_the_left_right_diff。裏付けなし |
| src/service/output.rs | test_format_diff_text_with_conflicts | req_cli_065_text_states_the_conflicts_of_each_file_and_the_total_at_the_end。裏付けなし |
| src/service/output.rs | test_format_diff_text_no_conflicts | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text。裏付けなし |
| src/service/output.rs | test_format_multi_diff_text_with_conflicts | req_cli_065_text_states_the_conflicts_of_each_file_and_the_total_at_the_end。裏付けなし |
| src/service/output.rs | test_format_multi_diff_text_no_conflicts | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text。裏付けなし |
| src/service/output.rs | test_diff_output_conflict_count_zero_omitted_in_json | req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text。裏付けなし |
| src/service/output.rs | test_diff_output_conflict_count_nonzero_in_json | req_cli_065_json_counts_and_locates_conflicts。裏付けなし |
| src/cli/ref_guard.rs | ref_same_as_left_returns_none | req_cli_066_a_ref_equal_to_either_side_warns_and_compares_without_the_ref。status と merge の整理でも根拠にした |
| src/cli/ref_guard.rs | ref_same_as_right_returns_none | 同上。status と merge の整理でも根拠にした |
| src/cli/ref_guard.rs | ref_different_returns_some | req_cli_062_a_configured_server_as_ref_makes_a_three_way_diff。status と merge の整理でも根拠にした |
| src/cli/ref_guard.rs | ref_none_returns_none | req_cli_063_without_ref_json_has_neither_ref_nor_ref_hunks。status と merge の整理でも根拠にした |
| src/service/source_pair.rs | test_resolve_ref_source_remote | req_cli_062_a_configured_server_as_ref_makes_a_three_way_diff。status の整理でも根拠にした |
| src/service/source_pair.rs | test_resolve_ref_source_local | req_cli_062_local_as_ref_makes_a_three_way_diff。status の整理でも根拠にした |
| src/service/source_pair.rs | test_resolve_ref_source_nonexistent | req_cli_062_an_unknown_ref_server_is_an_error_with_exit_code_two。status の整理でも根拠にした |
| src/service/source_pair.rs | test_resolve_ref_source_none | req_cli_063_without_ref_json_has_neither_ref_nor_ref_hunks。status の整理でも根拠にした |
| src/service/source_pair.rs | test_resolve_ref_source_same_as_left | req_cli_066_a_ref_equal_to_either_side_warns_and_compares_without_the_ref。status の整理でも根拠にした |
| tests/cli_diff_general.rs | test_diff_with_ref | req_cli_062_local_as_ref_makes_a_three_way_diff と req_cli_064_text_shows_the_ref_diff_after_the_left_right_diff。裏付けなし |

### B. 置き換えていない単体テスト（7 件）

入口を通す根拠テストがないため、消すとその振る舞いを確かめるテストがなくなる。残すことを勧める。
挿入だけの変更の二件は、整理前の変異テストで見逃しのあった detect_conflicts の挿入の分岐（219 行から 227 行）の近くを確かめる唯一のテストでもある。

| ファイル | テスト | 置き換えていない理由 |
|---|---|---|
| src/service/diff.rs | test_conflict_count_without_ref | --ref がないときの競合の項目は REQ-cli-065 が定めない |
| src/diff/conflict.rs | test_no_ref_returns_empty | 同上 |
| src/diff/conflict.rs | test_all_identical_no_conflicts | 三つが同じファイルは出力に出ず、入口から観測できない |
| src/diff/conflict.rs | test_empty_files | 同上 |
| src/diff/conflict.rs | test_insert_conflict_both_insert_different | 挿入だけの変更どうしの重なりは IR が定めない |
| src/diff/conflict.rs | test_ref_empty_both_add_different | 同上 |
| src/service/output.rs | test_format_diff_text_no_ref_backward_compat | --ref がないときのテキストは REQ-cli-064 が定めない。裏付けなし |

### 判断の結果

利用者は勧めのとおりにすると答えた。消すのは tests/cli_diff_general.rs の test_diff_with_ref の 1 件だけで、A の残りの 29 件と B の 7 件は全て残す。

- status の整理と同じく、実行ファイルを起動する tests/ の直下の重複テストだけを消し、失敗の場所がすぐ分かる速い単体テストは残す。
- test_diff_with_ref が確かめていたこと（"local" の参照先での三者比較、左右の見出し、参照先との差の見出しと中身）は、tests/contract/diff_ref_cli.rs の req_cli_062_local_as_ref_makes_a_three_way_diff と req_cli_064_text_shows_the_ref_diff_after_the_left_right_diff が全て確かめる。
- B の 7 件は入口を通す代わりの根拠がなく、挿入だけの変更の 2 件は見逃しのあった挿入の分岐の近くを確かめる唯一のテストである。

## 整理後の変異テスト

削除を終えたコミット 22e1f4a で、整理前と同じコマンドを一回実行した。実行中は作業ツリーに触れていない。
回す前の `cargo mutants --list` は整理前と同じ 93 件だった。

全体の集計は `mutants: caught=59 survived=17 timeout=4 unviable=13 equivalent=0`（93 件、実行時間は約 28 分）。スクリプトの終了コードは整理前と同じく 1 で、kotowari mutants が見逃しを error として返したもの。

| ファイル | 関数 | caught | survived | timeout | unviable |
|---|---|---|---|---|---|
| src/service/diff.rs | build_diff_output | 5 | 2 | 0 | 1 |
| src/diff/conflict.rs | detect_conflicts | 19 | 11 | 0 | 0 |
| src/diff/conflict.rs | extract_changes | 28 | 1 | 4 | 3 |
| src/diff/conflict.rs | merge_overlapping_regions | 2 | 2 | 0 | 0 |
| src/diff/conflict.rs | merge_ranges | 0 | 1 | 0 | 7 |
| src/cli/ref_guard.rs | validate_ref_side | 3 | 0 | 0 | 1 |
| src/service/source_pair.rs | resolve_ref_source | 2 | 0 | 0 | 1 |

### 整理前との比較

整理後の見逃し 17 件は全て整理前の見逃しに含まれる。削除で増えた見逃しはないため、戻した削除はない。
消した tests/cli_diff_general.rs の test_diff_with_ref は `scripts/mutants.sh` が変異ごとに流すテストに入らないため、もともと変異テストには現れない。

整理前の見逃しのうち、src/diff/conflict.rs:227 の `replace && with ||` と `replace < with >` の二件は整理後に検知された。
どちらも req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text が落ちた。same_delete.txt で右が末尾に足した行（左の消した行から離れた挿入）を、変異では競合と数えるためである。

### テストを足した後の回し直し

見逃しを落とすため、req_cli_065 の三つのテストの構成に場合を足した（コミット c219976）。
競合のある組として、右の範囲が左の範囲を含む組（overlapping_mirror.txt）を、競合のない組として、隣り合う行をそれぞれが変えた組（adjacent.txt）と、一方が行を足しただけで他方の変えた行から離れているか直後に足した組（left_insert_before.txt、left_insert_after.txt、left_insert_next.txt、right_insert_before.txt、right_insert_next.txt）を加えた。
行を足しただけの変更が他方の変えた範囲の中や同じ位置に入る場合は、用語「競合」が空の範囲の重なりを定めていないため置いていない（下の新しい FLAG の候補。FLAG-cli-071 になった）。

回し直しは決定記録 2026-09-29-mutation-rerun-and-load の A2 に従い、整理後の実行で caught にならなかった 21 件（見逃し 17 件とタイムアウト 4 件）だけを、その名前に完全に一致する `--re` で回した。回す前の `cargo mutants --list` で 21 件であることを確かめた。

```sh
scripts/mutants.sh --re '^(src/diff/conflict\.rs:120:19:\ replace\ \+=\ with\ \*=\ in\ extract_changes|src/diff/conflict\.rs:129:19:\ replace\ \+=\ with\ \*=\ in\ extract_changes|src/diff/conflict\.rs:153:57:\ replace\ \+\ with\ \*\ in\ extract_changes|src/diff/conflict\.rs:162:19:\ replace\ \+=\ with\ \*=\ in\ extract_changes|src/diff/conflict\.rs:219:48:\ replace\ \&\&\ with\ \|\|\ in\ detect_conflicts|src/diff/conflict\.rs:224:36:\ replace\ \&\&\ with\ \|\|\ in\ detect_conflicts|src/diff/conflict\.rs:224:25:\ replace\ >=\ with\ <\ in\ detect_conflicts|src/diff/conflict\.rs:180:11:\ replace\ \+=\ with\ \*=\ in\ extract_changes|src/diff/conflict\.rs:224:47:\ replace\ <\ with\ ==\ in\ detect_conflicts|src/diff/conflict\.rs:224:47:\ replace\ <\ with\ >\ in\ detect_conflicts|src/diff/conflict\.rs:224:47:\ replace\ <\ with\ <=\ in\ detect_conflicts|src/diff/conflict\.rs:225:31:\ replace\ ==\ with\ !=\ in\ detect_conflicts|src/diff/conflict\.rs:227:25:\ replace\ >=\ with\ <\ in\ detect_conflicts|src/diff/conflict\.rs:227:47:\ replace\ <\ with\ ==\ in\ detect_conflicts|src/diff/conflict\.rs:227:47:\ replace\ <\ with\ <=\ in\ detect_conflicts|src/diff/conflict\.rs:230:25:\ replace\ <\ with\ <=\ in\ detect_conflicts|src/diff/conflict\.rs:265:35:\ replace\ <\ with\ ==\ in\ merge_overlapping_regions|src/diff/conflict\.rs:265:35:\ replace\ <\ with\ <=\ in\ merge_overlapping_regions|src/diff/conflict\.rs:286:5:\ replace\ merge_ranges\ \->\ Option<Range<usize>>\ with\ None|src/service/diff\.rs:47:17:\ delete\ match\ arm\ engine::DiffResult::Equal\ in\ build_diff_output|src/service/diff\.rs:68:36:\ replace\ \&\&\ with\ \|\|\ in\ build_diff_output)$' \
  src/service/diff.rs src/diff/conflict.rs src/cli/ref_guard.rs src/service/source_pair.rs
```

結果は `mutants: caught=10 survived=6 timeout=5 unviable=0 equivalent=0`（21 件、約 20 分、コミット c219976）。

- caught の 10 件は src/diff/conflict.rs の 224 行の五件、225 行、227 行の三件（`>=` を `<`、`<` を `==`、`<` を `<=`）、230 行。225 行は req_cli_065_json_counts_and_locates_conflicts と req_cli_065_text_states_the_conflicts_of_each_file_and_the_total_at_the_end（overlapping_mirror.txt が競合でなくなる）、ほかの九件は req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text（足した組を競合と数える）で落ちた。
- src/diff/conflict.rs:153 の `replace + with * in extract_changes` はタイムアウトになった。ログでは、tests/cli_diff.rs のディレクトリのリンクのテスト三件（directory_link_spelling_keeps_link_and_child_changes、directory_links_report_link_names_and_children_under_entry_path、directory_spelling_with_or_without_slash_finds_same_child_diff。どれも本体に --ref がなく、競合を調べる経路を通らないことを読んで確かめた。止まった理由は確かめていない）が 217 秒を超えて止まっており、diff_ref_cli のテストの結果はログにない。そのため、コミットに含めない一時的な書き換えでこの変異を入れ、`cargo nextest run --all-features --no-fail-fast --test contract diff_ref_cli` を流すと、req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text が落ちた（13 件のうち 1 件失敗）。変異では直後に足した行の位置が一行前にずれ、他方の変えた行と重なるためである。確かめた後に `git checkout` で戻し、`git diff --stat src/` が空に戻ることを確かめた。
- 整理前から続くタイムアウトの四件（extract_changes の `+=` を `*=`）は、ログで src/diff/conflict.rs の単体テストが 217 秒を超えて止まっており、添字が進まずに止まらなくなる変異である。

### 直後に足す組を除いた後の回し直し

足した組のうち left_insert_next.txt と right_insert_next.txt は、行を足すだけの変更が他方の変えた範囲の終わりに接するときを競合でないとするもので、FLAG-cli-071 が未決に残した空の範囲の重なりの一方の端を固定していた。
そのため二組を根拠テストから除いた（コミット 28cc95d）。競合のない組は七つになる。

除いたことで検知されなくなる変異を確かめるため、上の回し直しで caught とした、または一時的な書き換えで検知を確かめた src/diff/conflict.rs の 153 行・224 行・227 行の九件だけを、その名前に完全に一致する `--re` で回した（コミット 7bb7181。28cc95d の後にテキストの競合の数の確かめ方だけを変えたコミット）。
回す前の `cargo mutants --list` で、この三行の変異のうち九件がこの名前に当たることを確かめた。三行のほかの三件（153 行の `+` を `-`、227 行の `&&` を `||` と `<` を `>`）は、組を足す前の整理後の実行（22e1f4a）で caught だったため含めていない。

```sh
scripts/mutants.sh --re '^(src/diff/conflict\.rs:153:57:\ replace\ \+\ with\ \*\ in\ extract_changes|src/diff/conflict\.rs:224:36:\ replace\ \&\&\ with\ \|\|\ in\ detect_conflicts|src/diff/conflict\.rs:224:25:\ replace\ >=\ with\ <\ in\ detect_conflicts|src/diff/conflict\.rs:224:47:\ replace\ <\ with\ ==\ in\ detect_conflicts|src/diff/conflict\.rs:224:47:\ replace\ <\ with\ >\ in\ detect_conflicts|src/diff/conflict\.rs:224:47:\ replace\ <\ with\ <=\ in\ detect_conflicts|src/diff/conflict\.rs:227:25:\ replace\ >=\ with\ <\ in\ detect_conflicts|src/diff/conflict\.rs:227:47:\ replace\ <\ with\ ==\ in\ detect_conflicts|src/diff/conflict\.rs:227:47:\ replace\ <\ with\ <=\ in\ detect_conflicts)$' \
  src/diff/conflict.rs
```

結果は `mutants: caught=4 survived=5 timeout=0 unviable=0 equivalent=0`（9 件、約 6 分）。スクリプトの終了コードは 1 で、kotowari mutants が見逃しを error として返したもの。

- caught の四件は 224 行の `replace && with ||`、`replace >= with <`、`replace < with >` と、227 行の `replace >= with <`。どれも req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text が落ちた。残した挿入だけの組（left_insert_before.txt・left_insert_after.txt・right_insert_before.txt）のどれかを競合と数えるためと見られる（どの組で落ちたかはログにない）。
- 見逃しの五件は 153 行の `replace + with * in extract_changes`、224 行と 227 行の `replace < with ==` と `replace < with <=`。どれも二組でしか検知されていなかった。
- 153 行の変異は今回はタイムアウトにならず、テストが約 49 秒で終わって見逃しになった。前の回し直しでのタイムアウトの理由は確かめていない。

### 見逃しの決着

| 位置 | 変異 | 決着 |
|---|---|---|
| src/diff/conflict.rs:153 | replace + with * in extract_changes | FLAG-cli-071 の範囲として記録する。検知していた直後に足す組を除いたため（上の節）。挿入だけの変更の位置を一行前にずらす変異で、他方の変えた範囲の終わりに接する挿入を範囲の中に入れる。残した組では検知されない（直後に足す組を除いた後の回し直しで見逃し）。空の範囲の端の扱いのほかに検知できる入力があるかは確かめていない |
| src/diff/conflict.rs:224 | replace && with \|\|、replace >= with <、replace < with > | テストを足した（left_insert_before.txt・left_insert_after.txt）。直後に足す組を除いた後の回し直しで caught |
| src/diff/conflict.rs:224 | replace < with ==、replace < with <= | FLAG-cli-071 の範囲として記録する。直後に足す組を除いた後の回し直しで見逃し（上の節）。`<=` は他方の変えた範囲の終わりに接する挿入を競合とし、`==` は範囲の中への挿入を競合とせず終わりに接する挿入を競合とする。どちらも挿入だけの変更と他方の範囲の重なりの扱い（FLAG-cli-071）を変える |
| src/diff/conflict.rs:225 | replace == with != in detect_conflicts | テストを足した（overlapping_mirror.txt）。回し直しで caught |
| src/diff/conflict.rs:227 | replace >= with < | テストを足した（right_insert_before.txt）。直後に足す組を除いた後の回し直しで caught |
| src/diff/conflict.rs:227 | replace < with ==、replace < with <= | FLAG-cli-071 の範囲として記録する。224 行の同じ変異と同じ理由（左右を入れ替えた分岐） |
| src/diff/conflict.rs:230 | replace < with <= in detect_conflicts | テストを足した（adjacent.txt）。回し直しで caught |
| src/diff/conflict.rs:219 | replace && with \|\| in detect_conflicts | FLAG-cli-071 の範囲として記録する（行を足すだけの変更の競合。利用者の判断で未決の FLAG として残した）。下の節 |
| src/diff/conflict.rs:265 | replace < with == in merge_overlapping_regions | FLAG-cli-072 の範囲として記録する（複数の変更と重なる変更の競合の数。利用者の判断で未決の FLAG として残した）。下の節 |
| src/diff/conflict.rs:265 | replace < with <= in merge_overlapping_regions | FLAG-cli-071 の範囲として記録する。下の節 |
| src/diff/conflict.rs:286 | replace merge_ranges -> Option<Range<usize>> with None | 既存の FLAG-cli-058 の範囲として記録する。merge_ranges の結果は、まとめた競合の "conflict_regions" の要素の中の "left_diff_range"・"right_diff_range"（TUI 用の範囲）にだけ入り、"conflict_count" と "conflict_regions" があることは変わらない。要素の形は FLAG-cli-058 で未決のため根拠テストで確かめない |
| src/service/diff.rs:47 | delete match arm engine::DiffResult::Equal in build_diff_output | 同等変異として .kotowari/mutants-equivalents.yaml に登録した。この腕を消すと Equal は後ろの `_ => Some(vec![])` の腕に入り、同じ値を返す。別の文脈のエージェントに、公開された build_diff_output を参照先の中身を左と同じにして呼び、左右に差がある組・左右とも同じ組・左が空の組で "ref_hunks" とその JSON を確かめるテストを書かせたが、変異を書き入れても通り、落ちるテストを書けなかった |

同等変異を登録した後に結果を読み直すと、整理後の実行は `mutants: caught=59 survived=16 timeout=4 unviable=13 equivalent=1`、回し直しは `mutants: caught=10 survived=5 timeout=5 unviable=0 equivalent=1` になる（`kotowari mutants --tool cargo-mutants --format text` を同じ outcomes.json に対して実行）。
回し直しで残る見逃し 5 件は、FLAG-cli-071・072 の範囲の三件、FLAG-cli-058 の範囲の一件、決着の対象でない一件である。
直後に足す組を除いた後は、この回し直しで caught だった 224 行と 227 行の四件と、一時的な書き換えで検知を確かめた 153 行の一件も見逃しになり、FLAG-cli-071 の範囲に移した。

決着の対象でない見逃し（整理前から変わらない）:

- src/service/diff.rs:68 の `replace && with || in build_diff_output`（バイナリと symlink の競合。FLAG-cli-059・063 の範囲）

### 新しい FLAG の候補

次の二つは IR が定めていないため、根拠テストで固定せず、利用者の判断を仰いだ。実装は直していない。
どちらもコミットに含めない一時的なテスト（`detect_conflicts` を直接呼ぶもの）で、元のコードと変異での競合の数を確かめた。確かめた後にテストを消し、`git checkout` で src を戻して `git diff --stat src/` と `git status --short` が空に戻ることを確かめた。

1. 挿入だけの変更の重なり（src/diff/conflict.rs:219 と、265 の `<=`）
   用語「競合」は「同じ箇所とは参照先からの変更の行の範囲が重なること」とするが、行を足すだけの変更は参照先の行の範囲が空で、空の範囲が重なるかを定めていない。
   実装は、両方が同じ位置に足したときと、一方が足した位置が他方の変えた範囲の中（範囲の先頭の行の後ろから末尾の行まで）にあるときを競合とし、範囲の先頭の行の前に足したときも競合とする。
   219 行の変異は、一方だけが足したときに位置が範囲の先頭と同じときだけを競合とするもので、参照先 "1..5" の 2・3 行を左が変え、右が 2 行と 3 行の間に足した組で、元のコードでは競合 1、変異では 0 になった。
   265 行の `<=` は、隣り合う二つの競合を一つにまとめる変異で、行を変える変更どうしでは隣り合う競合ができないため、挿入だけの変更が関わるときにだけ違いが出ると見られる（コードを読んだ推測で、違いの出る入力は確かめていない）。
2. 一つの変更が他方の複数の変更と重なるときの競合の数（src/diff/conflict.rs:265 の `==`）
   参照先 "a\nb\nc" に対して左が三行を全て変え、右が 1 行目と 3 行目を別々に変えた組で、実装は重なる二つの組を一つにまとめて "conflict_count" を 1 とし、変異では 2 になった。REQ-cli-065 の「競合の数」がこの場合に 1 か 2 かを IR は定めていない。

利用者の判断: 二つとも未決の FLAG として残す（勧めを採用）。取り込みとテスト整理の中では仕様を決めず、次にこの機能を扱うときに決める。

- 1 は FLAG-cli-071（行を足すだけの変更の競合）として docs/ir/cli/FLAGS.md に記録された。判断は決定記録 [2026-09-30-diff-ref-mutant-flags](../decision/records/2026-09-30-diff-ref-mutant-flags.md) の A1。src/diff/conflict.rs:219 の `replace && with ||` と 265 の `replace < with <=` はこの FLAG の範囲とする。
  直後に足す組を除いた後は、153 の `replace + with *`、224 と 227 の `replace < with ==` と `replace < with <=` の五件もこの FLAG の範囲とする（上の決着の表）。決定記録の A1 はこの五件を挙げていない。
- 2 は FLAG-cli-072（複数の変更と重なる変更の競合の数）として記録された。判断は同じ決定記録の A2。src/diff/conflict.rs:265 の `replace < with ==` はこの FLAG の範囲とする。

これで決着の対象の見逃しは全て決着した。

## 要件の verification の見直し

この計画の要件の verification は全て unit で、いずれも具体的な場面の入力で結果が決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。
property の要件はないため、REQ-testing-010 の proptest の置き場所の確かめは当たらない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-cli-062 | unit | 指定した参照先の名前ごとに、三者比較になるかエラーになるかが決まる |
| REQ-cli-063 | unit | 参照先の有無と左と参照先の中身の組ごとに、JSON の項目が決まる |
| REQ-cli-064 | unit | 参照先との差が空かどうかで、テキストの見出しと hunk の有無が決まる |
| REQ-cli-065 | unit | 三つの中身の組ごとに、競合の数と表示が決まる |
| REQ-cli-066 | unit | 参照先が左と同じか右と同じかで、警告の文言と比較の続け方が決まる |
| REQ-cli-016 | unit | 参照先に対する左右の変更の組ごとに、競合を示すかが決まる |
