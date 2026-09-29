# 走査の上限と不完全な一覧のテスト整理の記録

走査の上限と不完全な一覧の取り込み（[決定記録](../decision/records/2026-09-29-adopt-scan-limits.md)）で加えた要件（REQ-scan-008・REQ-scan-009 と判定表 TBL-scan-001）と、既存の要件 REQ-scan-004 の例 EX-scan-008・009 の SSH とエージェントの経路の根拠テストを整えた過程の記録。
各要件の根拠にしたテスト、変異テストの結果、見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テスト 63 件に実装の中身をなぞるだけのものはなかったため、この整理ではテストを消していない。
テストを削除しない整理のため、整理前の変異テストは回していない（REQ-testing-012、[決定記録 A2](../decision/records/2026-09-28-mutation-scope.md#A2)）。

## 要件ごとの根拠テスト

手本にした元のテストは消さず、書き換えていない。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた（既存の挙動を確かめるテストのため、失敗する段階はない）。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
FLAG-scan-010 から 014 の挙動（件数の数え方の経路ごとの違い、ちょうど上限の件数、find のタイムアウト、読めないディレクトリ、同じ最上位のディレクトリの下を複数指定した sync）は確かめない。存在しないディレクトリを指定した sync、merge の --hunks と --max-entries（FLAG-merge-018）、パスを指定した diff の範囲とディレクトリ symlink の上限（REQ-cli-021）、TUI の走査も確かめない。

### 上限の超過の案内と走査の範囲（REQ-scan-008・009、関数呼び出し）

根拠テストは `tests/contract/scan_limit_scope.rs` にある。
組み方は `tests/contract/scan_limits.rs` の `scan_fixture` と同じで、設定を `load_config_from_paths` で読み、`RuntimeTargets::with_local` でサーバ "develop" をローカルの一時ディレクトリに差し替え、`execute_status`・`execute_diff`・`execute_merge`・`execute_sync` を関数呼び出しで実行する。
merge と sync は全て `dry_run: true` で実行し、書き込みを起こさない。バックアップは無効にし、集約先も一時ディレクトリに差し替えた。
場合の分け方は `src/service/fast_path.rs` の `resolve_scan_strategy` と `fast_path_to_parent_dirs` の単体テストを手本にし、純粋関数の戦略ではなく関数呼び出しの結果（上限の超過のエラーになるか、merged に載るパス）で確かめる。

どの範囲を走査したかは、上限を超える範囲を走査したときだけ上限の超過のエラーになることで見分ける。
左右の root_dir に、ファイル 10 件の "big/"、3 件ずつの "small/" と "small2/"、直下のファイル "top.txt" を置き、左右で中身を変えて全てのファイルに差分があるようにし、上限を 5 にした。
全体の走査はディレクトリを数えても数えなくても 5 を大きく超え（ファイル 17 件）、"small/" だけ・"small2/" だけの走査はディレクトリを数えても 5 を下回り（ファイル 3 件）、"small/" と "small2/" を合わせるとディレクトリを数えなくても 5 を超える（ファイル 6 件）。
このため件数の数え方（FLAG-scan-010）とちょうど上限の件数（FLAG-scan-011）に触れない。

「エラーになる」は、関数がエラーを返し、その文（`{:#}` で原因をつないだもの）が "Tree scan truncated" を含むことで確かめる。merge と sync では左（読み込み元）の走査が先に上限を超えてエラーになる。
「エラーにならない」は、merge では関数が Ok を返し、終了コードが 0、failed が空で、merged に期待したパスの集まりが揃うことで確かめる。sync は右の書き込み先の走査の失敗（接続の失敗を含む）を関数のエラーにせず結果の中の失敗として返すため、Ok であることに加え、書き込み先が一つで、その failed が空で、終了コードが 0 で、merged に期待したパスの集まりが揃うことで確かめる。

| 要件・判定表の行 | 根拠テスト | 確かめること |
|---|---|---|
| REQ-scan-008 | every_command_reports_the_scan_limit_with_three_ways_out | status（パスなし）、diff（パスなし）、merge と sync（パス "."。パスなしは走査の前に別のエラーで止まるため）のそれぞれで、エラーの文が "Tree scan truncated at 5 entries." で始まり、"--max-entries"・"max_scan_entries"・"specify file paths" を含む |
| REQ-scan-009（status と、パスなしの diff） | status_and_diff_without_paths_scan_the_whole_root_dir | status とパスなしの diff が上限の超過のエラーになる。パスを指定した diff は cli diff の話題のため確かめない（[決定記録 A8](../decision/records/2026-09-29-adopt-scan-limits.md#A8)） |
| TBL-scan-001 "."・"./"・空の値 | a_root_marker_or_an_empty_path_scans_the_whole_root_dir | merge と sync のそれぞれで "."・"./"・"" の一つずつが上限の超過のエラーになる |
| TBL-scan-001 glob 文字 | a_path_with_glob_characters_scans_the_whole_root_dir | merge と sync のそれぞれで "small/*.txt" が上限の超過のエラーになる |
| TBL-scan-001 21 個以上 | twenty_one_paths_scan_the_whole_root_dir_but_twenty_do_not | merge と sync のそれぞれで "small/a.txt" を 21 個重ねるとエラーになり、20 個ではエラーにならず merged が "small/a.txt" だけになる。判定は重複を除かずに数えるため、同じパスを重ねて個数だけを変えた |
| TBL-scan-001 末尾の "/" の混在 | mixing_directory_and_file_paths_scans_the_whole_root_dir | merge と sync のそれぞれで "small/" と "small/a.txt" が上限の超過のエラーになる |
| TBL-scan-001 --delete | delete_scans_the_whole_root_dir | merge と sync のそれぞれで --delete を付けた "small/" が上限の超過のエラーになる |
| TBL-scan-001 全て末尾が "/" | directory_paths_scan_only_below_each_directory_with_the_limit_per_directory | merge と sync のそれぞれで "small/" がエラーにならず merged が "small/" の下の 3 件だけになり、"small/" と "small2/" も（合わせると上限を超えるが）エラーにならず merged が二つの下の 6 件になる。二つは最上位のディレクトリが違うため FLAG-scan-014 に触れない |
| TBL-scan-001 全て末尾が "/" でない | file_paths_scan_only_their_parent_directories_unless_one_is_directly_under_the_root_dir | merge と sync のそれぞれで "small/a.txt" がエラーにならず merged が "small/a.txt" だけになり、"top.txt"（root_dir の直下のファイル）は上限の超過のエラーになる |

- 判定表の行ごとにテストを分け、各テストの中で merge と sync を同じ場合で回した。
- merged の比較は集まり（`BTreeSet`）で行い、重ねて指定したパスが merged で一つにまとまるかどうかには触れない。
