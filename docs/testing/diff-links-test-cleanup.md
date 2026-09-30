# CLI diff の symlink とディレクトリのテスト整理の記録

CLI diff の symlink とディレクトリの取り込み（[決定記録](../decision/records/2026-09-30-adopt-diff-links.md)）の範囲の要件（REQ-cli-020 から 026）と例（EX-cli-039 から 061）について、印付きの根拠テストが十分かを一件ずつ見直し、足りないものを補った過程の記録。
あわせて、この範囲の変異テストの見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テストに実装の中身をなぞるだけのものはなかったため、テストは消さず、既存のテストの中身と期待も変えていない。製品のコードも変えていない。
テストを削除しない整理のため、整理前の変異テストは回していない（REQ-testing-012、[決定記録 A2](../decision/records/2026-09-28-mutation-scope.md#A2)）。

## 例と要件ごとの見直し

例の Given・When・Then と要件の文の各部分を、印付きのテストが観測しているかで判断した。
FLAG-cli-043 から 055 と既存の FLAG に当たる部分は確かめない（note の文言は FLAG-cli-048、機密の symlink の終了コードは FLAG-cli-046、SSH の側だけ参照先がない場合は FLAG-cli-044、ローカルの通常ファイルと SSH のディレクトリ symlink の組は FLAG-cli-045、パスを指定しない diff は FLAG-cli-047、root_dir の外へ出る連鎖の機密の判定は FLAG-cli-051、比べなかったときの hunks のリンク文字列は FLAG-cli-052、同じ symlink の "Link target:" は FLAG-cli-054、読めない通常のファイルは FLAG-cli-030）。
テストは全て `tests/cli_diff.rs` にあり、左はローカル、右は試験 SSH サーバ（エージェントは無効）で、実行ファイルを起動して標準出力と終了コードを確かめる。root_dir の外に置くファイルとディレクトリは、既存のテストと同じく一時ディレクトリの base（local と remote の親）の下の、local と remote のどちらでもない場所に置いた。

判定の「十分」は印付きのテストが FLAG に当たらない部分を全て観測していること、「補った」は足りない部分にテストを足したこと、「FLAG の範囲として補わない」は足りない部分が FLAG の範囲にだけあることを表す。

### 例

| 例 | 印付きのテスト | 判定 | 理由 |
|---|---|---|---|
| EX-cli-039 | diff_shows_link_targets_and_resolved_file_contents、json_diff_keeps_link_targets_and_reports_unexamined_external_content、json_diff_separates_link_names_from_resolved_text_changes、link_target_and_resolved_text_have_separate_json_fields（tests/contract/cli_results.rs）、text_diff_shows_link_targets_before_resolved_content_lines（足した） | 補った | JSON ではリンク文字列（link_targets）と内容（hunks）が分かれることを確かめていたが、テキストはリンク文字列と内容が出力のどこかに含まれることしか見ていなかった。"-link target:"・"+link target:" の行、"Resolved content differs"、内容の "-"・"+" の行がこの順に別々の行で出ることを確かめるテストを足した |
| EX-cli-040 | same_link_target_with_changed_content_is_a_diff | 十分 | 左右の内容が出ることと終了コード 1 を確かめている |
| EX-cli-053 | matching_links_with_matching_contents_have_no_difference | 十分 | 終了コード 0 と "0 file(s) with changes" を確かめている。差分のない項目が出ること（FLAG-cli-054）は確かめない |
| EX-cli-041 | link_and_regular_file_with_equal_contents_still_differ_in_kind | 十分 | 終了コード 1、左のリンク文字列、右（通常ファイル）が null、内容の差がないことを確かめている。種類の違いの note の文言は FLAG-cli-048 |
| EX-cli-042 | external_binary_links_report_distinct_targets_and_sha256_hashes、binary_link_text_shows_both_link_names_and_binary_hashes、external_binary_link_hashes_are_sha256_of_the_target_contents（足した） | 補った | ハッシュが 64 文字で左右で違うことしか見ておらず、参照先の中身の SHA-256 であること（REQ-cli-020 の「通常のバイナリ diff と同じ SHA-256 ハッシュ」）を確かめていなかった。left_hash と right_hash が参照先の中身の SHA-256 と一致することを確かめるテストを足した |
| EX-cli-054 | one_sided_file_link_shows_available_content_without_read_error | 十分 | リンク文字列と片側の内容が出ること、終了コード 1（取得エラーの 2 ではない）を確かめている。相手側がないときの note の文言は FLAG-cli-048 |
| EX-cli-043 | directory_links_report_link_names_and_children_under_entry_path | 十分 | "shared" の項目の左右のリンク文字列と、"shared/child.txt" の項目の左右の内容を確かめている |
| EX-cli-044 | nested_directory_link_cycle_keeps_readable_child_diff_and_reports_cycle、cycle_on_one_side_of_a_directory_link_is_reported（足した） | 補った | 左右の両方で循環する場合だけを試していた。例の Given は「配下の symlink が既に辿った祖先を指す」で、左右のどちらかだけで起きても循環である。左だけに循環のリンクを置き、循環の理由と終了コード 2、読めた子の左右の内容が残ることを確かめるテストを足した |
| EX-cli-045 | directory_link_entry_limit_counts_children_across_nested_links、directory_link_entry_limit_keeps_the_child_diff_read_before_the_limit（足した） | 補った | 「読めた範囲の差分」を files に "left" が含まれることで確かめていたが、"shared" の項目のリンク文字列 "left-dir" だけで満たされ、子の差分が残ることを観測していなかった。上限に達する前に読んだ "shared/a.txt" の項目に左右の内容が残ることを確かめるテストを足した |
| EX-cli-055 | external_directory_nested_secret_stays_hidden_without_force、nested_link_from_external_directory_to_secret_shows_neither_contents_nor_hashes（足した） | 補った | 下の「EX-cli-055 に足したテスト」のとおり |
| EX-cli-046 | directory_link_against_plain_directory_keeps_link_and_child_results、directory_link_against_plain_directory_compares_the_child_read_through_the_link（足した） | 補った | 子の差分を右（通常のディレクトリ）の内容が含まれることでしか見ておらず、左のリンクを通して子を読んだことを観測していなかった。"shared/child.txt" の項目に左右の内容が出ることを確かめるテストを足した |
| EX-cli-061 | directory_link_against_regular_file_shows_both_kinds_of_content、directory_link_against_regular_file_reports_the_link_target（足した） | 補った | 左の子と右の本文は確かめていたが、Then の「リンク文字列の差」を見ていなかった。"shared" の項目の link_targets の left が "actual"、right が null であることを確かめるテストを足した。種類の違いの note の文言は FLAG-cli-048、向きが逆の組は FLAG-cli-045 |
| EX-cli-056 | one_sided_directory_link_shows_child_contents_without_read_error、one_sided_directory_link_reports_its_link_target_and_null_for_the_missing_side（足した） | 補った | 子の内容とエラーがないことは確かめていたが、Then の「片側のリンク文字列」を見ていなかった。"shared" の項目の link_targets の left が "actual"、項目がない右が null であることを確かめるテストを足した |
| EX-cli-057 | different_directory_link_names_count_as_change_even_if_children_match | 十分 | 終了コード 1、files_with_changes が 1、files が "shared" の一件だけであることを確かめている |
| EX-cli-047 | unreadable_child_of_directory_link_keeps_other_child_diffs、unreadable_child_of_directory_link_is_reported_with_a_reason（足した） | 補った | 読めない子の errors の path は確かめていたが、reason を見ていなかった。reason に "unreadable" が含まれることを確かめるテストを足した。読めない子は壊れたリンクの場合だけを試し、読めない通常のファイルの子は読めない通常のファイルの扱い（FLAG-cli-030）の範囲のため補わない |
| EX-cli-048 | broken_link_keeps_other_diffs_and_reports_unreadable_target、broken_link_on_the_local_side_is_an_error_even_when_the_other_side_reads（足した） | 補った | 左右の両方で参照先がない場合だけを試していた。ローカルの側だけ参照先がなく SSH の側は読める場合に、エラー（errors の path と reason）と終了コード 2 になり他のファイルの差分が残ることを確かめるテストを足した。SSH の側だけ参照先がない場合は FLAG-cli-044 の範囲のため確かめない |
| EX-cli-049 | sensitive_target_contents_remain_hidden_through_an_ordinary_link_name | 十分 | テキストと JSON の両方で参照先の内容が出ないことを確かめている |
| EX-cli-050 | force_explicitly_shows_sensitive_link_target_changes | 十分 | --force で左右の内容が出ることと終了コード 1 を確かめている |
| EX-cli-058 | external_link_is_not_read_without_explicit_permission、json_diff_keeps_link_targets_and_reports_unexamined_external_content | 十分 | 終了コード 2、リンク文字列、"not compared" の理由、errors の path、内容が出ないことを確かめている |
| EX-cli-059 | external_file_content_is_compared_only_with_follow_flag | 十分 | --follow-external-links だけ（--force なし）で root_dir の外の左右の内容が出ることを確かめている |
| EX-cli-051 | directory_spelling_with_or_without_slash_finds_same_child_diff、test_diff_trailing_slash_normalized、directory_argument_with_and_without_slash_has_the_same_child_diff（tests/contract/cli_results.rs） | 十分 | "src" と "src/" で標準出力と終了コードが同じで、子の差分が出ることを確かめている |
| EX-cli-052 | matching_directory_contents_have_no_diff_with_either_spelling、equal_directory_diff_json_reports_no_changed_files（tests/contract/cli_results.rs） | 十分 | どちらの指定でも終了コード 0 と差分なしを確かめている |
| EX-cli-060 | directory_link_spelling_keeps_link_and_child_changes、directory_link_slash_does_not_discard_link_or_child_diff（tests/contract/cli_results.rs） | 十分 | "shared" と "shared/" で出力と終了コードが同じで、リンク文字列と子の差分が出ることを確かめている |

### 要件

要件の文のうち、例で確かめている部分は上の表による。ここには例のない部分と、要件に直接印を付けたテストを書く。

| 要件 | 要件に直接印を付けたテスト | 判定 | 理由 |
|---|---|---|---|
| REQ-cli-020 | directory_link_against_binary_file_reports_the_file_hash（足した）、link_to_binary_against_link_to_text_reports_both_hashes（足した） | 補った | 例のバイナリの場合は左右ともバイナリの参照先だけだった。「参照先がバイナリなら通常のバイナリ diff と同じ SHA-256 ハッシュ」は片側だけがバイナリでも当たる（通常のファイルの diff は片側がバイナリならバイナリとして扱う）。左がバイナリを指すリンク・右がテキストを指すリンクの組で両側のハッシュを、左がディレクトリ symlink・右がバイナリの通常ファイルの組で右のハッシュを確かめるテストを足した。「組み合わせによらず」のうち例のない組（ファイル symlink と通常のディレクトリなど）は、例として定められていないため足していない |
| REQ-cli-021 | cycle_below_plain_subdirectory_is_reported_before_entry_limit、plain_subdirectory_entries_under_a_directory_link_count_toward_the_entry_limit（足した）、entries_up_to_the_limit_under_a_directory_link_are_compared_completely（足した） | 補った | 「指定した比較全体で既存の最大走査件数を一回だけ適用」について、例 EX-cli-045 は入れ子のリンクの配下を数える場合だけで、リンクの配下の通常のサブディレクトリの子を数えることと、上限ちょうどの件数では不完全としないこと（既存の走査の上限の EX-scan-009「上限以下なら打ち切らない」）を確かめていなかった。前者は --max-entries 3 でサブディレクトリと子 3 件（計 4 件）なら件数超過のエラーになること、後者は --max-entries 3 で子 3 件のリンクと、サブディレクトリ 1 件と子 2 件のリンクがそれぞれエラーにならず全ての子を比べることを確かめるテストを足した。後者はディレクトリも 1 件と数えるか（FLAG-scan-010）によらず上限以下になる構成にした |
| REQ-cli-022 | なし | 十分 | 各部分は EX-cli-047・048・058（読めない参照先、読めない子、範囲外）、EX-cli-044・045（循環と件数超過）、EX-cli-054（片側の項目がない場合はエラーにならない）で確かめている。通常のディレクトリの展開のテキストの理由は FLAG-cli-050 |
| REQ-cli-023 | nested_link_to_sensitive_file_does_not_show_resolved_contents、sensitive_intermediate_link_name_masks_even_when_final_name_is_public、sensitive_link_name_hides_the_target_contents_without_force（足した）、sensitive_link_chain_on_one_side_hides_its_contents（足した） | 補った | 最終参照先と途中の段のリンク文字列は確かめていたが、入口名が機密パターンに当たる symlink の場合と、左右の片側だけの連鎖が機密ファイルに行き着く場合を試していなかった。".env" という名前のリンクが普通の名前のファイルを指す場合と、左だけ途中のリンクが ".env" を指す場合に、テキストと JSON に内容が出ないことを確かめるテストを足した。root_dir の外と「バイナリハッシュに表示しない」は EX-cli-055 に足したテストで確かめる |
| REQ-cli-024 | json_link_item_keeps_the_symlink_flag（足した） | 補った | JSON の symlink の項目の symlink フラグをどのテストも確かめていなかった。symlink が true で link_targets の left と right を持つことを確かめるテストを足した。テキストの区別は EX-cli-039、項目がない側が null は EX-cli-056、通常ファイルの側が null は EX-cli-041 で確かめる |
| REQ-cli-025 | trailing_slash_does_not_bypass_external_directory_link_guard、empty_directory_has_same_no_diff_result_with_or_without_slash | 十分 | 例 EX-cli-051・052・060 と合わせて、ディレクトリとディレクトリ symlink の "/" の有無で結果が変わらないことを確かめている |
| REQ-cli-026 | a_parent_link_cannot_read_outside_root_without_follow_flag、trailing_slash_does_not_bypass_external_directory_link_guard、nested_link_to_outside_file_is_not_read_without_follow_flag（足した）、force_does_not_read_an_external_link_without_follow_flag（足した）、parent_directory_path_is_rejected_even_with_follow_flag（足した）、one_side_reaching_outside_through_a_directory_link_is_not_compared_without_follow_flag（足した）、one_sided_external_link_reports_its_link_target_without_follow_flag（足した） | 補った | 入口のリンクが root_dir の外を指す場合は確かめていたが、次を試していなかった。root_dir の中のディレクトリリンクの配下の入れ子のリンクが外を指す場合、--force だけでは外を辿らないこと（--force と独立）、入力パスの親ディレクトリへの遡りが --follow-external-links でも拒否されること、左右の片側だけが外へ出る場合（ディレクトリリンクを通るパスで中身が同じとき、片側だけのリンクのリンク文字列）。それぞれテストを足した。「status・merge・sync には適用せず」は、実装ではこのオプションが diff のサブコマンドにしか定義されていないが、要件は他のコマンドで拒否するか無視するかを定めないため、今の拒否を固定するテストは足していない |

このほか、変異テストの見逃しを落とすために、EX-cli-044 に returning_to_a_traversed_directory_on_one_side_is_reported_as_a_cycle、REQ-cli-023 に sensitive_intermediate_link_reached_through_dot_components_hides_contents、REQ-cli-058（この計画の対象の外の要件で、テキストの総数を定める）に text_total_counts_the_children_compared_under_a_directory_link を足した。理由は下の「見逃しと決着」にある。

### EX-cli-055 に足したテスト

nested_link_from_external_directory_to_secret_shows_neither_contents_nor_hashes は例の Given のとおりに組む。
左右それぞれに root_dir の外の別の場所（base の下の "left-outside" と "right-outside"）を用意し、その中の "shared-dir" に、同じ場所の "secret/.env" を指す相対のリンク "nested"（リンク文字列は左右とも "../secret/.env"）を置く。左右の root_dir の "shared" は、それぞれの "shared-dir" を絶対パスで指すディレクトリ symlink にする。
".env" の中身は NUL を含むバイナリで、左右で違う。symlink の項目のハッシュは参照先がバイナリのときだけ付くため、テキストではハッシュが出ないという確認が隠す処理を壊しても通ってしまうからである。
--follow-external-links を付け --force なしの JSON で、標準出力に左右の中身の目印（"left-example"・"right-example"）が出ず、"shared/nested" の項目の link_targets が左右とも "../secret/.env"（入れ子のリンクが解決されている）で、left_hash と right_hash が null であることを確かめる。
"shared" の項目そのもの（FLAG-cli-054）と終了コード（FLAG-cli-046）は確かめない。リンク文字列を左右で同じにしたため、"shared/nested" の hunks にリンク文字列の行は出ない（FLAG-cli-052 に触れない）。
このテストで隠れるのは、入れ子のリンクのリンク文字列の名前 ".env" が機密パターンに当たる経路である。`sensitive_link_chain` が root_dir の外で連鎖を辿らない部分（FLAG-cli-051）は確かめない。

既存の external_directory_nested_secret_stays_hidden_without_force は、最終参照先が外部のディレクトリの中の ".env" で、左右が同じ外部のディレクトリを指すため、連鎖が外部のディレクトリからさらに別の場所へ出る場合を試しておらず、中身がテキストのためハッシュを確かめられない。

### 足したテストが狙いの挙動を壊すと落ちることの確認

足したテストは既存の挙動を確かめるものなので、書いた時点の実装に対して最初の実行で通ることを確かめた（失敗する段階はない）。
そのうえで、テストごとに狙いの挙動を壊す変更を src/ に一時的に書き入れ、`cargo nextest run --all-features --no-fail-fast --test cli_diff`（テストを絞ったものを含む）を回して、足したテストが落ちることを確かめた。確かめるたびに `git checkout -- src/` で戻し、`git diff --stat src/` が空であることを確かめた。
同じ変更で、同じ例の既存のテストが落ちなかったものは「既存のテスト」の欄に書く（足したテストが既存のテストの見ていない部分を見ていることの裏付け）。

| 足したテスト | 一時的な変更 | 結果 | 既存のテスト |
|---|---|---|---|
| text_diff_shows_link_targets_before_resolved_content_lines | src/service/output.rs の format_diff_text の `if !output.hunks.is_empty()`（"Resolved content differs" を出す条件）の `!` を消す | 落ちた | diff_shows_link_targets_and_resolved_file_contents は通った |
| json_link_item_keeps_the_symlink_flag | src/service/diff.rs の build_symlink_diff_output の `symlink: true` を false にする | 落ちた | — |
| external_binary_link_hashes_are_sha256_of_the_target_contents | src/cli/diff.rs のファイル symlink の left_hash を中身の先頭 1 バイトを除いた SHA-256 にする | 落ちた | external_binary_links_report_distinct_targets_and_sha256_hashes は通った |
| directory_link_entry_limit_keeps_the_child_diff_read_before_the_limit | ディレクトリ symlink の展開で件数超過を検出したときに、それまでの files を捨てる（`file_diffs.clear()`） | 落ちた | directory_link_entry_limit_counts_children_across_nested_links は通った |
| directory_link_against_plain_directory_compares_the_child_read_through_the_link | ディレクトリを展開した子の左の読み込みを空にする | 落ちた | directory_link_against_plain_directory_keeps_link_and_child_results は通った |
| directory_link_against_regular_file_reports_the_link_target、one_sided_directory_link_reports_its_link_target_and_null_for_the_missing_side | build_symlink_diff_output の link_targets の left を None にする。別に、right を左のリンク文字列にする | どちらの変更でも両方落ちた | directory_link_against_regular_file_shows_both_kinds_of_content と one_sided_directory_link_shows_child_contents_without_read_error は通った |
| unreadable_child_of_directory_link_is_reported_with_a_reason | 参照先が読めないときの理由 "symlink target unreadable; resolved content not compared"（二か所）を空の文字列にする | 落ちた | unreadable_child_of_directory_link_keeps_other_child_diffs は通った |
| sensitive_link_name_hides_the_target_contents_without_force | symlink の内容を隠す条件 `(sensitive \|\| target_sensitive) && !args.force` から入口名の `sensitive` を除く | 落ちた | — |
| nested_link_from_external_directory_to_secret_shows_neither_contents_nor_hashes | 同じ条件からリンク文字列と連鎖の `target_sensitive` を除く | 落ちた | external_directory_nested_secret_stays_hidden_without_force は通った |
| nested_link_to_outside_file_is_not_read_without_follow_flag | root_dir の外かどうかの二つの確かめ（ループの先頭と symlink の分岐）を、ディレクトリを展開した子では行わないようにする | 落ちた | a_parent_link_cannot_read_outside_root_without_follow_flag と trailing_slash_does_not_bypass_external_directory_link_guard は通った |
| force_does_not_read_an_external_link_without_follow_flag | 同じ二つの確かめを --force のときは行わないようにする | 落ちた | external_link_is_not_read_without_explicit_permission は通った |
| parent_directory_path_is_rejected_even_with_follow_flag | src/service/path_resolver.rs の check_path_traversal が ".." を拒否しないようにする | 落ちた | — |

次のテストは、前の回（[差分の出し方の記録](./diff-output-test-cleanup.md)の「記録だけするもの」の表）で「diff の次の回」とした見逃しの位置を手がかりに、その行の条件が要件のどの部分に当たるかを読んで足した。どれも上の表の理由のとおり要件の文が定める部分で、見逃しを落とすことだけを目的にしたものではない。
確かめ方は上と同じで、cargo-mutants が作るのと同じ変異を一時的に書き入れ、`cargo nextest run --all-features --no-fail-fast --test cli_diff` の 60 件を回した。落ちたのはどれも足したテストの一件だけだった。

| 変異 | 行の中身 | 落ちたテスト |
|---|---|---|
| src/cli/diff.rs:190:17 replace \|\| with && in execute_diff | 左右のどちらかのパスが root_dir の外に出るか | one_side_reaching_outside_through_a_directory_link_is_not_compared_without_follow_flag |
| src/cli/diff.rs:671:17 replace \|\| with && in run_diff_fast_path | 左右のどちらかの解決したパスが root_dir の外か | 同上 |
| src/cli/diff.rs:198:38 replace \|\| with && in execute_diff | root_dir の外のパスがリンクか | one_sided_external_link_reports_its_link_target_without_follow_flag |
| src/cli/diff.rs:353:21 replace \|\| with && in execute_diff | ディレクトリ symlink の右の実パスが祖先にあるか | cycle_on_one_side_of_a_directory_link_is_reported |
| src/cli/diff.rs:254:37 replace += with *= in execute_diff | 通常のディレクトリを展開した項目の数を数える | plain_subdirectory_entries_under_a_directory_link_count_toward_the_entry_limit |
| src/cli/diff.rs:255:40 replace > with >= / replace > with == in execute_diff | 通常のディレクトリを展開した項目の数が上限を超えたか | entries_up_to_the_limit_under_a_directory_link_are_compared_completely（どちらの変異でも） |
| src/cli/diff.rs:377:40 replace > with >= / replace > with == in execute_diff | ディレクトリ symlink を展開した項目の数が上限を超えたか | 同上（どちらの変異でも） |
| src/cli/diff.rs:289:17・290:17 replace \|\| with && in execute_diff | リンクの連なりに機密ファイルがあるか | sensitive_link_chain_on_one_side_hides_its_contents（どちらの変異でも） |
| src/cli/diff.rs:400:21 replace \|\| with && in execute_diff | ディレクトリ symlink の組で右の中身がバイナリか | directory_link_against_binary_file_reports_the_file_hash |
| src/cli/diff.rs:426:38 replace \|\| with && in execute_diff | リンク先の片側が読めないか | broken_link_on_the_local_side_is_an_error_even_when_the_other_side_reads |
| src/cli/diff.rs:440:17 replace \|\| with && in execute_diff | ファイル symlink の右の中身がバイナリか | link_to_binary_against_link_to_text_reports_both_hashes |

同じ確かめで、src/cli/diff.rs:233:21（replace || with && in execute_diff。通常のディレクトリの右の実パスが祖先にあるか）と 394:35（replace += with *= in execute_diff。ディレクトリ symlink を展開した項目を summary.scanned_files に数える）は 60 件全てが通った。この二つは下の変異テストの結果で決着させる。

## 変異テスト

この範囲の見逃しに絞って、足したテストをコミットした後のコミット 2e28690 で一度回した（作業ツリーの変更は、この記録の新しいファイルだけ）。
回したのは、前の回の[記録](./diff-output-test-cleanup.md)の「記録だけするもの」の表で「diff の次の回」とした `src/cli/diff.rs` の 17 件と `src/service/output.rs:155:12` の 1 件の位置と、前の回に回していない symlink の補助の関数の全ての変異である。
前の回の後に src/ は変わっていない（`git log 9c9ff66..HEAD -- src/` が空）ため、位置はそのまま使った。549:30 と 718:48 は前の回の表で FLAG-cli-035 の区分のため回していない。
実行中は作業ツリーにも他の cargo のコマンドにも触れていない。並列数は既定の 2 のまま。

```sh
scripts/mutants.sh \
  --re '^src/cli/diff\.rs:(190:17|198:38|233:21|254:37|255:40|289:17|290:17|304:21|353:21|377:40|394:35|400:21|426:38|440:17|671:17): ' \
  --re '^src/service/output\.rs:155:12: ' \
  --re '(replace (sensitive_link_chain|path_escapes_root|resolved_path_outside_root|link_target_for_diff|inspected_real_path|read_existing_diff_file|build_symlink_diff_output) -> | in (sensitive_link_chain|path_escapes_root|resolved_path_outside_root|link_target_for_diff|inspected_real_path|read_existing_diff_file|build_symlink_diff_output)$)' \
  src/cli/diff.rs src/service/diff.rs src/service/output.rs
```

結果は `mutants: caught=42 survived=6 timeout=0 unviable=1 equivalent=0`（49 件、約 10 分）。スクリプトの終了コードは 1 で、kotowari mutants が見逃しを error として報告したためである（メモリ上限での停止ではない）。
流すテストはスクリプトの決まりどおり単体テスト・tests/contract・tests/cli_diff.rs に限られ、負荷の下で落ちる TUI やエージェントのテストは含まれない。変異ごとのログで失敗したテストは全てこの三つのどれかだった。

### 実行の前の一覧と正規表現の直し

実行の前に、計画の三つの `--re` と三つのファイルで `cargo mutants --list --all-features` をメモリ上限と低い CPU 優先度の中で実行すると 33 件だった。
位置の指定に当たる変異は 23 件で、前の回の表の 18 件は全て現れた。残りの 5 件は同じ位置の別の演算子の変異（254:37 と 394:35 の `-=`、255:40 と 377:40 の `<`。前の回は caught）で、位置で錨を打った正規表現には演算子を区別する手段がないため含めて回した。
`execute_diff` の変異は位置の指定に当たる 20 件だけで、それ以外の変異は混ざっていなかった。構造体のフィールドを消す変異は一覧になかった（三つのファイルの全ての変異の一覧 247 件にもなかった）。

ただし計画の三つ目の `--re`（`' in (関数名)$'`）は、名前が "in 関数名" で終わる関数の中の変異にしか当たらず、関数全体の戻り値を置き換える変異（名前が "replace 関数名 -> 型 with 値" の形）に当たらなかった。
そのため `path_escapes_root`・`inspected_real_path`・`read_existing_diff_file` は一件も当たらず、他の関数も戻り値の置き換えが抜けていた。計画の「関数ごと回す」に合わせて、戻り値の置き換えにも当たるよう上のコマンドの三つ目の `--re` に直した。直した後の一覧は 49 件で、直す前の 33 件を全て含み、増えたのは 16 件の戻り値の置き換えだけだった。

| ファイル | 関数 | 一覧の件数 | caught | survived | unviable |
|---|---|---|---|---|---|
| src/cli/diff.rs | execute_diff（位置の指定） | 20 | 17 | 3 | 0 |
| src/cli/diff.rs | run_diff_fast_path（位置の指定） | 1 | 1 | 0 | 0 |
| src/service/output.rs | format_diff_text（位置の指定） | 1 | 1 | 0 | 0 |
| src/cli/diff.rs | sensitive_link_chain | 7 | 4 | 3 | 0 |
| src/cli/diff.rs | path_escapes_root | 2 | 2 | 0 | 0 |
| src/cli/diff.rs | resolved_path_outside_root | 4 | 4 | 0 | 0 |
| src/cli/diff.rs | link_target_for_diff | 4 | 4 | 0 | 0 |
| src/cli/diff.rs | inspected_real_path | 2 | 2 | 0 | 0 |
| src/cli/diff.rs | read_existing_diff_file | 4 | 4 | 0 | 0 |
| src/service/diff.rs | build_symlink_diff_output | 4 | 3 | 0 | 1 |

unviable の 1 件は `replace build_symlink_diff_output -> DiffOutput with Default::default()`（DiffOutput に Default がない）。
前の回に見逃しだった 18 件のうち、S1 で足したテストで 15 件が caught になり、233:21・304:21・394:35 の 3 件が見逃しのまま残った（どのテストが落としたかは上の「足したテストが狙いの挙動を壊すと落ちることの確認」の二つ目の表と一致した）。

### 見逃しと決着

見逃しは 6 件で、どれも FLAG-cli-043 から 055・既存の FLAG・--ref・エージェントの経路の範囲に当たらないため、全て決着の対象にした。

| 位置 | 変異 | 行の中身 | 決着 |
|---|---|---|---|
| src/cli/diff.rs:233:21 | replace \|\| with && in execute_diff | 通常のディレクトリの左右の実パスのどちらかが既に辿った祖先か | テストを足した。左だけ、入口のディレクトリ symlink "shared"（実体は "left-dir/sub"）の配下に ".." を指すリンク "up" を置くと、"shared/up/sub" は通常のディレクトリで実パスが入口と同じになる。変異では片側だけの循環を見落とし、一段深い "shared/up/sub/up" で循環が報告される。returning_to_a_traversed_directory_on_one_side_is_reported_as_a_cycle（EX-cli-044）で、循環のエラーのパスが "shared/up/sub" であることを確かめる（REQ-cli-022 の「比較できなかった入口側のパスと理由」） |
| src/cli/diff.rs:304:21 | delete match arm Ok(TargetPath::Symlink{real_path, ..}) in execute_diff | symlink の実パスが root_dir の外か | 同等変異として登録した。同じループの先頭の確かめ（path_escapes_root）が同じパス・同じ root_dir・同じ inspect_path の結果で同じ比較をして先に "outside root_dir" のエラーにするため、この腕が true を返すことはない。別の文脈のエージェントに出力が変わる入力を探させたが見つからなかった（登録の why に書いた）。腕を消せば変異もなくなるが、製品のコードを変えない計画のため消していない |
| src/cli/diff.rs:394:35 | replace += with *= in execute_diff | ディレクトリ symlink を展開した子を走査したファイルの数に数える | テストを足した。数は JSON の summary.scanned_files とテキストの最後の行の "out of N total" に出て、REQ-cli-058 はこの N を「走査したファイルの数」とする。text_total_counts_the_children_compared_under_a_directory_link（REQ-cli-058）で、子を 2 件持つディレクトリ symlink を指定したテキストの N が 2 以上であることを確かめる。リンクの項目自体を数えるか（今は数えて 3 になる）は要件から決まらないため、値は固定していない |
| src/cli/diff.rs:636:17 | delete match arm Component::CurDir in sensitive_link_chain | リンク文字列の "." の段を読み飛ばす | テストを足した。root_dir の中の連鎖で途中のリンク文字列が機密パターンに当たるかを見る部分で、root_dir の外へ出る連鎖（FLAG-cli-051）ではない。sensitive_intermediate_link_reached_through_dot_components_hides_contents（REQ-cli-023）で、"./inner.txt" を指すリンクと "sub/../inner.txt" を指すリンクから ".env" という名前のリンクを経て普通の名前のファイルへ行き着く場合に、内容が出ないことを確かめる |
| src/cli/diff.rs:637:41 | replace match guard next.pop() with true in sensitive_link_chain | リンク文字列の ".." の段で一つ上に戻る | 同上 |
| src/cli/diff.rs:637:41 | replace match guard next.pop() with false in sensitive_link_chain | 同上 | 同上 |

足した三つのテストは、変異を一時的に書き入れて `cargo nextest run --all-features --no-fail-fast --test cli_diff`（63 件）を回し、それぞれの変異でそのテストの一件だけが落ちることを確かめた（確かめるたびに `git checkout -- src/` で戻し、`git diff --stat src/` が空であることを確かめた）。
同等変異を登録した後に最初の実行の結果を kotowari mutants で読むと `caught=42 survived=5 timeout=0 unviable=1 equivalent=1` になった。

### テストを足した後の回し直し

[決定記録 A2](../decision/records/2026-09-29-mutation-rerun-and-load.md#A2) が認める回し直しとして、最初の実行で見逃しになった変異を含む関数（位置の指定の `execute_diff` と `sensitive_link_chain`）だけに絞って回し直し、結果を最初の実行の記録と合わせて読む。
実行したのは同等変異を登録した後のコミット 48f57f7 で（作業ツリーの変更はこの記録の新しいファイルだけ）、実行中は作業ツリーにも他の cargo のコマンドにも触れていない。

```sh
scripts/mutants.sh \
  --re '^src/cli/diff\.rs:(190:17|198:38|233:21|254:37|255:40|289:17|290:17|304:21|353:21|377:40|394:35|400:21|426:38|440:17|671:17): ' \
  --re '(replace sensitive_link_chain -> | in sensitive_link_chain$)' \
  src/cli/diff.rs
```

結果は `mutants: caught=27 survived=0 timeout=0 unviable=0 equivalent=1`（28 件、約 7 分）。スクリプトの終了コードは 0。
28 件は最初の実行の位置の指定の 21 件（`execute_diff` 20 件と `run_diff_fast_path` 1 件）と `sensitive_link_chain` の 7 件で、最初の実行の見逃しのうち 233:21・394:35・636:17 と 637:41 の 2 件は足したテストで caught になり（ログで失敗したテストは上の表の足したテストだった）、304:21 は同等変異として数えられた。新しい見逃しはない。
最初の実行で回して回し直していない関数（`format_diff_text` の位置の指定、`path_escapes_root`・`resolved_path_outside_root`・`link_target_for_diff`・`inspected_real_path`・`read_existing_diff_file`・`build_symlink_diff_output`）は、最初の実行で見逃しがなかった。

決着していない見逃し、新しい FLAG の候補、verification の見直しの候補はない。
要件の文から決まらないため値や挙動を固定しなかった点が二つある。ディレクトリ symlink の項目自体を REQ-cli-058 の「走査したファイルの数」に数えるか（上の 394:35）と、REQ-cli-026 の「status・merge・sync には適用せず」がオプションを拒否することか無視することか（上の「要件」の表）である。どちらも今の要件の読みで満たせるように確かめたため FLAG の候補にはしていないが、要件を詳しくするときの材料として残す。

## 要件の verification の見直し

REQ-cli-020 から 026 の verification は全て unit である。いずれも、左右に置く symlink・ディレクトリ・ファイルの組み合わせ、指定するパスの書き方、--follow-external-links・--force・--max-entries の有無という具体的な場面で、出力と終了コードが決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）と判断した。見直しの候補はない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-cli-020 | unit | 左右の symlink・通常ファイル・ディレクトリの種類と有無、参照先の中身（テキストかバイナリか）の組の場面で、リンク文字列と内容の差と差分の有無が決まる |
| REQ-cli-021 | unit | ディレクトリ symlink の配下の構成（入れ子のリンク、循環、件数）と --max-entries の値の場面で、子の項目とエラーが決まる |
| REQ-cli-022 | unit | 参照先がない・読めない子がある・範囲外・循環・件数超過の場面で、files に残るものと errors と終了コードが決まる |
| REQ-cli-023 | unit | 入口名・途中のリンク文字列・最終参照先のどれが機密パターンに当たるかと --force の有無の場面で、内容とハッシュを出すかが決まる |
| REQ-cli-024 | unit | symlink の項目を含む比較の場面で、JSON の欄とテキストの行の出し方が決まる |
| REQ-cli-025 | unit | 同じディレクトリを末尾の "/" の有無で指定する二つの場面で、結果が同じになる |
| REQ-cli-026 | unit | リンクが root_dir の外を指すかと --follow-external-links・--force の有無、入力パスの書き方の場面で、辿るかとエラーが決まる |
