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
左右の root_dir に、ファイル 10 件の "big/"、3 件ずつの "small/" と "small2/"、ファイル 1 件ずつの "one00/" から "one20/" の 21 個のディレクトリ、直下のファイル "top.txt" を置き、左右で中身を変えて全てのファイルに差分があるようにし、上限を 5 にした。
全体の走査はディレクトリを数えても数えなくても 5 を大きく超え（ファイル 38 件）、"big/" だけの走査もディレクトリを数えなくても 5 を超え（ファイル 10 件）、"small/" だけ・"small2/" だけ・"oneNN/" の一つだけの走査はディレクトリを数えても 5 を下回り（ファイル 3 件か 1 件）、"small/" と "small2/" を合わせるとディレクトリを数えなくても 5 を超える（ファイル 6 件）。
このため件数の数え方（FLAG-scan-010）とちょうど上限の件数（FLAG-scan-011）に触れない。

「エラーになる」は、関数がエラーを返し、その文（`{:#}` で原因をつないだもの）が "Tree scan truncated" を含むことで確かめる。merge と sync では左（読み込み元）の走査が先に上限を超えてエラーになる。
「エラーにならない」は、merge では関数が Ok を返し、終了コードが 0、failed が空で、merged に期待したパスの集まりが揃うことで確かめる。sync は右の書き込み先の走査の失敗（接続の失敗を含む）を関数のエラーにせず結果の中の失敗として返すため、Ok であることに加え、書き込み先が一つで、その failed が空で、終了コードが 0 で、merged に期待したパスの集まりが揃うことで確かめる。

| 要件・判定表の行 | 根拠テスト | 確かめること |
|---|---|---|
| REQ-scan-008 | every_command_reports_the_scan_limit_with_three_ways_out | status（パスなし）、diff（パスなし）、merge と sync（パス "."。パスなしは走査の前に別のエラーで止まるため）のそれぞれで、エラーの文が "Tree scan truncated at 5 entries." で始まり、"--max-entries"・"max_scan_entries"・"specify file paths" を含む |
| REQ-scan-009（status と、パスなしの diff） | status_and_diff_without_paths_scan_the_whole_root_dir | status とパスなしの diff が上限の超過のエラーになる。パスを指定した diff は cli diff の話題のため確かめない（[決定記録 A8](../decision/records/2026-09-29-adopt-scan-limits.md#A8)） |
| TBL-scan-001 "."・"./"・空の値 | a_root_marker_or_an_empty_path_scans_the_whole_root_dir | merge と sync のそれぞれで "."・"./"・"" の一つずつが上限の超過のエラーになる |
| TBL-scan-001 glob 文字 | a_path_with_glob_characters_scans_the_whole_root_dir | merge と sync のそれぞれで "small/*.txt"・"small/?.txt"・"small/[ab].txt" の一つずつが上限の超過のエラーになる（判定表の三つの文字をそれぞれ確かめる） |
| TBL-scan-001 21 個以上 | twenty_one_paths_scan_the_whole_root_dir_but_twenty_do_not | merge と sync のそれぞれで、互いに異なる 21 個のパス "one00/f.txt" から "one20/f.txt" がエラーになり、そのうち 20 個ではエラーにならず merged がその 20 件になる。親ディレクトリが全て違い、親ディレクトリごとの走査は 1 件ずつで上限を大きく下回るため、エラーになるかどうかはパスの個数だけで決まる。判定表は重複したパスを個数に数えるかを決めていないため、同じパスを重ねる指定は使わない |
| TBL-scan-001 末尾の "/" の混在 | mixing_directory_and_file_paths_scans_the_whole_root_dir | merge と sync のそれぞれで "small/" と "small/a.txt" が上限の超過のエラーになる |
| TBL-scan-001 --delete | delete_scans_the_whole_root_dir | merge と sync のそれぞれで --delete を付けた "small/" が上限の超過のエラーになる |
| TBL-scan-001 全て末尾が "/" | directory_paths_scan_only_below_each_directory_with_the_limit_per_directory | merge と sync のそれぞれで "small/" がエラーにならず merged が "small/" の下の 3 件だけになり、"big/" は（ディレクトリの走査にも上限を当てるため）上限の超過のエラーになり、"small/" と "big/" も（二つ目のディレクトリも走査するため）上限の超過のエラーになり、"small/" と "small2/" は（合わせると上限を超えるが）エラーにならず merged が二つの下の 6 件になる。指定したディレクトリはどれも最上位のディレクトリが違うため FLAG-scan-014 に触れない |
| TBL-scan-001 全て末尾が "/" でない | file_paths_scan_only_their_parent_directories_unless_one_is_directly_under_the_root_dir | merge と sync のそれぞれで "small/a.txt" がエラーにならず merged が "small/a.txt" だけになり、"small/a.txt" と "small2/a.txt" は（二つの親ディレクトリを合わせると上限を超えるが）エラーにならず merged がその 2 件になり、"big/0.txt" は（親ディレクトリ "big/" の走査が上限を超えるため）上限の超過のエラーになり、"small/a.txt" と "big/0.txt" も（二つ目のパスの親ディレクトリも走査するため）上限の超過のエラーになり、"top.txt"（root_dir の直下のファイル）は上限の超過のエラーになる |

- 判定表の行ごとにテストを分け、各テストの中で merge と sync を同じ場合で回した。
- merged の比較は集まり（`BTreeSet`）で行う。
- 指定したファイルのパスは、親ディレクトリの走査に載らなくても merged に載る（実装を読んだ判断と、二つ目の親ディレクトリを走査しない一時的な書き換えでも "small/a.txt" と "small2/a.txt" の merged が変わらなかった観測による）。そのため、親ディレクトリを全て走査することは merged ではなく、二つ目に上限を超える親ディレクトリが来る指定で上限の超過になることで確かめる。

### 上限の超過と上限の内の一覧をリモートの経路で（EX-scan-008・009、REQ-scan-004）

根拠テストは `tests/contract/scan_limits_cli.rs` にある（`test-utils` の feature が要る。SSH の試験サーバを使うため）。
既存の `tests/contract/scan_limits.rs` の status_reports_a_scan_limit_instead_of_returning_a_partial_file_list と status_uses_the_explicit_limit_and_lists_every_file_when_it_fits は関数呼び出しでローカルの経路だけを通るため、前の回の[記録](./scan-listing-test-cleanup.md)の `tests/contract/scan_listing_cli.rs` と同じ組み方で、実行ファイルを試験 SSH サーバに対して `status --left local --right develop --all --format json --max-entries <上限>` で起動する。
起動の前後の確認（`--config` と一時ディレクトリの作業ディレクトリ、`env_clear`、SSH の経路の `TestDirs::assert_isolated_config_at`、エージェントの経路の `assert_isolated_agent_config` と、起動の後の試験サーバのコマンドの記録で " agent --root " があり "find -L" がないことの確認）は前の回のテストと同じで、補助を共有した。
共有のため、`scan_listing_cli.rs` の起動の補助 `launch_status`・`status_over_ssh` に status の後に足す引数を渡せるようにし、エージェントの経路の起動を一覧に直す前の出力を返す `AgentFixture::status_via_agent` に分け、これらと `statuses_by_path`・`AgentFixture` をモジュールの外から使えるようにした。前の回のテストの中身と期待は変えていない。

上限の超過のエラーの文は三つの経路で同じで、どちらの側の走査かを示さない。左の走査が先にエラーになると右の経路を通らずに通ってしまうため、上限の超過の場合は左の local の root_dir をファイル 1 件にし、右だけにファイル 10 件を置いて上限を 3 にした。
上限の超過は、終了コードが 0 以外であることと、標準出力と標準エラーをつないだものに "Tree scan truncated" が含まれることで確かめる。
加えて、例の Then の「完全な一覧としては返されない」を、標準出力が "files" を持つ JSON として読めないことで確かめる（一覧を出しつつ失敗の終了コードを返す実装を落とすため）。
使い捨ての確認として、同じ構成のまま上限だけを 50 にするとこの二つのテストが失敗する（status が成功する）ことを確かめ、エラーが右の走査の上限で起きていることを確かめた（確かめた後に元に戻した）。
上限の内の場合は左右に同じ 3 件（"a.txt"、"dir/b.txt"、"dir/c.txt"）を置いて上限を 50 にし、一覧がその 3 件の "equal" だけに一致する（`assert_eq!`）ことで、右の経路が打ち切らずに全てのファイルを載せたことを確かめる。

| 例 | 経路 | 根拠テスト | 確かめること |
|---|---|---|---|
| EX-scan-008 | SSH | status_reports_a_scan_limit_on_the_remote_side_over_ssh | 左 1 件・右 10 件・上限 3 で、終了コードが 0 以外になり "Tree scan truncated" が出て、標準出力が "files" を持つ JSON にならない |
| EX-scan-008 | エージェント | status_reports_a_scan_limit_on_the_remote_side_via_the_agent | 同じ構成と期待。エージェントで走査したことを起動の後に確かめる |
| EX-scan-009 | SSH | status_lists_every_remote_file_within_the_scan_limit_over_ssh | 左右に同じ 3 件・上限 50 で、終了コードが 0 で、一覧が 3 件の "equal" だけになる |
| EX-scan-009 | エージェント | status_lists_every_remote_file_within_the_scan_limit_via_the_agent | 同じ構成と期待。エージェントで走査したことを起動の後に確かめる |

- ファイルの数と上限には十分な差を取り、件数の数え方（FLAG-scan-010）とちょうど上限の件数（FLAG-scan-011）に触れない。

## 整理後の変異テスト

変異テストは決着の対象の関数に絞って一度だけ実行した。並列数は既定の 2 である。
実行したのはテストと記録を足した後のコミット 5cf5fc4 で、作業ツリーに変更のない状態で行い、実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh --re '(check_truncation|resolve_max_entries|resolve_scan_strategy|fast_path_to_parent_dirs|has_root_parent_dir|is_root_marker|has_glob_chars|fetch_tree_by_strategy|fetch_partial_tree|fetch_trees_and_statuses_for_merge|fetch_partial_trees|run_diff_full_scan|fetch_(remote_)?tree_recursive|fetch_(remote_)?tree_for_subpath|walk_single_root|scan_local_tree_recursive_with_include|list_tree_recursive|list_tree\b|handle_list_tree|ScanIterator.*::next)' src/runtime/side_io.rs src/runtime/target_io.rs src/runtime/core.rs src/config.rs src/service/fast_path.rs src/cli/sync.rs src/cli/merge.rs src/cli/diff.rs src/local/mod.rs src/ssh/client.rs src/agent/client.rs src/agent/dispatch.rs src/agent/tree_scan.rs
```

結果は `mutants: caught=75 survived=5 timeout=2 unviable=16 equivalent=0`（98 件、29 分）。スクリプトの終了コードは 1 で、kotowari mutants が見逃しを error として報告したためである（メモリ上限での停止ではない）。

### 実行の前の一覧と正規表現の直し

実行の前に、計画の `--re` とファイルで `cargo mutants --list --all-features`（テストを走らせない一覧の表示）をメモリ上限の中で実行したところ 96 件で、計画が `fetch_tree_recursive|fetch_tree_for_subpath` で当てるとしていた src/runtime/core.rs の `CoreRuntime::fetch_remote_tree_recursive` と `CoreRuntime::fetch_remote_tree_for_subpath`（SSH の経路で打ち切りの印を受けて `check_truncation` を呼ぶ関数）が変異の名前に現れなかった。名前の途中に "remote_" が入り、正規表現に一致しないためである。
そのため、名前を実装に合わせて `fetch_(remote_)?tree_recursive|fetch_(remote_)?tree_for_subpath` に直した。直した正規表現での一覧は 98 件で、増えたのはこの二つの関数の変異 1 件ずつだけだった。直した後、計画に挙げた関数は全て変異の名前に現れた。
`fetch_tree_recursive|fetch_tree_for_subpath` は、src/runtime/side_io.rs の `CoreRuntime::fetch_tree_recursive`・`fetch_tree_for_subpath`・`try_agent_fetch_tree_recursive`・`try_agent_fetch_tree_for_subpath` と、src/runtime/target_io.rs のローカルとリモートの実装に当たった。`ScanIterator.*::next` は `<impl Iterator for ScanIterator<'_>>::next` のほかに `ScanIterator<'a>::next_valid_path`（2 件）にも当たった。`list_tree\b` は src/agent/client.rs の `AgentClient::list_tree` だけに当たった。
正規表現に名前の一致する関数の外の変異として、構造体のフィールドを消す変異が 3 件混ざった（`SshClient::build_client_config` の `client::Config` の inactivity_timeout・keepalive_interval・keepalive_max）。cargo-mutants 27.1.0 がフィールドを消す変異を `--re` で除かないためで、計画の区分により他の関数のものとして記録だけする。
`Dispatcher::handle_list_tree` の `ScanOptions` のフィールドを消す変異 4 件（root・exclude・include・max_entries）は対象の関数の中のもので、フィールドの値の区分に従う。

`run_diff_partial_scan`（src/cli/diff.rs）は対象に含めていない。diff は指定したパスの末尾の "/" を `resolve_scan_strategy` を呼ぶ前に取り除く（`execute_diff` の最初のループ）ため、戦略が PartialScan になることがなく、到達できない（実装を読んだ判断）。

### 関数ごとの内訳

`outcomes.json` から数えた。一覧の件数は実行の前の `cargo mutants --list` の件数と同じである。括弧の中は、下の「負荷の下で落ちるテストだけに検知された変異」で見逃しとして扱う件数で、caught の数に含まれる。

| ファイル | 関数 | 一覧の件数 | caught | survived | timeout | unviable |
|---|---|---|---|---|---|---|
| src/agent/client.rs | AgentClient::list_tree | 4 | 2 | 0 | 0 | 2 |
| src/agent/dispatch.rs | Dispatcher::handle_list_tree（フィールドを消す変異 4 件を含む） | 8 | 4 | 2 | 1 | 1 |
| src/agent/tree_scan.rs | `<impl Iterator for ScanIterator>::next` | 4 | 3 | 0 | 0 | 1 |
| src/agent/tree_scan.rs | ScanIterator::next_valid_path | 2 | 1 | 0 | 1 | 0 |
| src/cli/diff.rs | run_diff_full_scan | 1 | 1 | 0 | 0 | 0 |
| src/cli/merge.rs | fetch_partial_trees | 1 | 1 | 0 | 0 | 0 |
| src/cli/merge.rs | fetch_trees_and_statuses_for_merge | 5 | 2 | 0 | 0 | 3 |
| src/cli/sync.rs | fetch_partial_tree | 1 | 1 | 0 | 0 | 0 |
| src/cli/sync.rs | fetch_tree_by_strategy | 1 | 1 | 0 | 0 | 0 |
| src/config.rs | resolve_max_entries | 2 | 2 | 0 | 0 | 0 |
| src/local/mod.rs | scan_local_tree_recursive_with_include | 7 | 5 | 0 | 0 | 2 |
| src/local/mod.rs | walk_single_root | 12 | 8 | 0 | 0 | 4 |
| src/runtime/core.rs | CoreRuntime::fetch_remote_tree_for_subpath | 1 | 1 | 0 | 0 | 0 |
| src/runtime/core.rs | CoreRuntime::fetch_remote_tree_recursive | 1 | 1 | 0 | 0 | 0 |
| src/runtime/side_io.rs | CoreRuntime::fetch_tree_for_subpath | 1 | 1 | 0 | 0 | 0 |
| src/runtime/side_io.rs | CoreRuntime::fetch_tree_recursive | 1 | 1 | 0 | 0 | 0 |
| src/runtime/side_io.rs | CoreRuntime::try_agent_fetch_tree_for_subpath | 2 | 1 | 1 | 0 | 0 |
| src/runtime/side_io.rs | CoreRuntime::try_agent_fetch_tree_recursive | 2 | 2 | 0 | 0 | 0 |
| src/runtime/side_io.rs | check_truncation | 1 | 1 | 0 | 0 | 0 |
| src/runtime/target_io.rs | LocalTargetIo::fetch_tree_for_subpath | 5 | 5 | 0 | 0 | 0 |
| src/runtime/target_io.rs | LocalTargetIo::fetch_tree_recursive | 1 | 1 | 0 | 0 | 0 |
| src/runtime/target_io.rs | RemoteTargetIo::fetch_tree_for_subpath | 2 | 2 | 0 | 0 | 0 |
| src/runtime/target_io.rs | RemoteTargetIo::fetch_tree_recursive | 1 | 1 | 0 | 0 | 0 |
| src/service/fast_path.rs | fast_path_to_parent_dirs | 3 | 3 | 0 | 0 | 0 |
| src/service/fast_path.rs | has_glob_chars | 7 | 7 | 0 | 0 | 0 |
| src/service/fast_path.rs | has_root_parent_dir | 4 | 4 | 0 | 0 | 0 |
| src/service/fast_path.rs | is_root_marker | 4 | 4 | 0 | 0 | 0 |
| src/service/fast_path.rs | resolve_scan_strategy | 6 | 5 | 0 | 0 | 1 |
| src/ssh/client.rs | SshClient::build_client_config（フィールドを消す変異） | 3 | 1（1） | 2 | 0 | 0 |
| src/ssh/client.rs | SshClient::list_tree_recursive | 5 | 3 | 0 | 0 | 2 |

unviable の 16 件は全て、関数の戻り値を `Default::default()` を含む値にする変異で、その型（`FileNode`、`AgentFileEntry`、`AgentResponse` など）が `Default` を実装しないため組み立てられない。
timeout の 2 件（src/agent/dispatch.rs:96:9 の handle_list_tree を `vec![]` にする変異と、src/agent/tree_scan.rs:123:9 の next_valid_path を `Some(Default::default())` にする変異）は、エージェントが応答を返さなくなるか走査が終わらなくなる変異で、テストが時間切れで止まった。kotowari mutants では notice で、見逃しとしては数えない。

### 検知したテスト

nextest は最初の失敗から少し進んで止まるため、cargo-mutants の変異ごとのログに集まる失敗したテストの名前は先に失敗したものだけになる。
そのため、足したテストが要件の変異で落ちることを、変異（cargo-mutants が書き出した差分）を一時的に書き入れて `cargo nextest run --all-features --no-fail-fast --test contract -E 'test(/scan_limit_scope|scan_limits_cli/)'` を回して確かめた。どれも確かめた後に `git checkout` で戻し、`git diff --stat src/` が空に戻ることを確かめた。

| 変異 | 落ちた足したテスト（13 件中） |
|---|---|
| src/runtime/side_io.rs:1077:5 check_truncation を `Ok(())` にする（上限の超過を報告しない） | 上限の超過を期待する 10 件全て（scan_limit_scope の 8 件と scan_limits_cli の上限の超過の 2 件） |
| src/service/fast_path.rs:70:20 replace > with >= in resolve_scan_strategy（20 個で全体を走査する） | twenty_one_paths_scan_the_whole_root_dir_but_twenty_do_not |
| src/service/fast_path.rs:76:42 delete ! in resolve_scan_strategy（全て末尾が "/" でも全体を走査する） | directory_paths_scan_only_below_each_directory_with_the_limit_per_directory |
| src/service/fast_path.rs:101:5 has_glob_chars を false にする | a_path_with_glob_characters_scans_the_whole_root_dir |
| src/agent/dispatch.rs:113:74 replace >= with < in Dispatcher::handle_list_tree（エージェントの打ち切りの印が反転する） | status_reports_a_scan_limit_on_the_remote_side_via_the_agent、status_lists_every_remote_file_within_the_scan_limit_via_the_agent |
| src/runtime/core.rs:754:9 fetch_remote_tree_recursive を `Ok(Default::default())` にする（SSH の経路が何も載せない） | status_reports_a_scan_limit_on_the_remote_side_over_ssh、status_lists_every_remote_file_within_the_scan_limit_over_ssh |
| src/service/fast_path.rs:95:5 is_root_marker を false にする | なし（既存の単体テストで caught） |
| src/service/fast_path.rs:131:5 has_root_parent_dir を false にする | なし（既存の単体テストで caught） |

is_root_marker と has_root_parent_dir を false にする変異では、"." は親ディレクトリが "./" のファイル形式のパスとして、"./" と "top.txt" は "./" のディレクトリの走査として扱われ、どちらも root_dir 全体の走査になって上限の超過のエラーになるため、足したテストは落ちない（コマンドの走査の範囲は変わらない）。どちらも src/service/fast_path.rs の既存の単体テストで caught になっている。

### 負荷の下で落ちるテストだけに検知された変異

次の 1 件は caught と数えられたが、失敗したのは tests/ssh_integration.rs のテストだけだった。
変異を一時的に書き入れて、そのテストだけを `cargo nextest run --all-features --no-fail-fast --test ssh_integration -E 'test(test_connect_with_verifier_accept_succeeds)'` で回し直すと通ったため、負荷の下で落ちるテストが落ちたものと判断し、見逃しとして扱う（下の「見逃しと決着」）。確かめた後に `git checkout` で戻し、`git diff --stat src/` が空に戻ることを確かめた。

| 変異 | ログで失敗したテスト | 回し直しの結果 |
|---|---|---|
| src/ssh/client.rs:215:13 delete field keepalive_max from struct client::Config expression in SshClient::build_client_config | test_connect_with_verifier_accept_succeeds | 通った |

tui_merge と agent_ssh のテストだけに検知された変異はなかった。

### 見逃しと決着

見逃しは、survived の 5 件と、上の負荷の下で落ちるテストだけに検知された 1 件の、合わせて 6 件である。
ここに挙げた見逃しは全て survived（または負荷の下の失敗だけ）のため、既存の単体テストでも落ちていない。

#### 記録だけするもの（計画の区分に当てはまる）

| 位置 | 変異 | 行の中身 | 区分 | 記録 |
|---|---|---|---|---|
| src/agent/dispatch.rs:103:13 | delete field include from struct ScanOptions expression in Dispatcher::handle_list_tree | `include: include.to_vec(),` | exclude と include の当て方（REQ-config-004・023、FLAG-scan-003 から 007 の範囲） | エージェントの走査に include が渡らなくなる。対象の関数の中のフィールドで、値の区分に従った |
| src/ssh/client.rs:213:13・214:13・215:13 | delete field inactivity_timeout / keepalive_interval / keepalive_max from struct client::Config expression in SshClient::build_client_config（215:13 は負荷の下の失敗だけ） | `inactivity_timeout: Some(Duration::from_secs(inactivity_secs)),` ほか | 対象の関数の外（SSH の接続の設定で、走査の上限とは関係しない） | 接続の無通信時間と keepalive が既定値になる。前の回の[記録](./scan-listing-test-cleanup.md)と同じ区分 |

#### 決着の対象の見逃し

| 位置 | 変異 | 行の中身 | 決着 |
|---|---|---|---|
| src/agent/dispatch.rs:104:13 | delete field max_entries from struct ScanOptions expression in Dispatcher::handle_list_tree | `max_entries,` | 記録だけする（`fail_on_truncation` が偽のときの扱い、TUI の範囲。引き継ぎ先は凍結中の TUI）。下の説明を参照。利用者の判断で確定 |
| src/runtime/side_io.rs:783:9 | replace CoreRuntime::try_agent_fetch_tree_for_subpath -> Option<anyhow::Result<FileTree>> with None | 関数全体 | 既存の FLAG（FLAG-scan-010・011・013・001）の範囲として記録する。下の説明を参照。利用者の判断で確定 |

src/agent/dispatch.rs:104:13 について（実装を読んだ判断で、変異の下の出力は比べていない）: 変異で `ScanOptions` の max_entries が既定値の `usize::MAX` になり、エージェントは件数で走査を止めずに全ての項目を返す。
打ち切りの印は同じ関数の 113 行で、要求された max_entries と数えた件数から `chunk.is_last && chunk.total_scanned >= max_entries` で求める。元のコードでは数えた件数は min(全件数, 上限) で、変異では全件数のため、どちらも「全件数が上限以上」のときに限り印が立ち、印の立ち方は変わらない（ちょうど上限の件数で印が立つのは FLAG-scan-011 の挙動で、これも変わらない）。
CLI は印が立つと `check_truncation` で上限の超過のエラーにする（`fail_on_truncation` が真）ため、エラーになるかどうかと、エラーにならないときの一覧は変わらない。変わるのは、印が立っても一覧を使う `fail_on_truncation` が偽の呼び出し（TUI の走査）で受け取る一覧が、上限までの一部から全件になることと、走査の時間とメモリである。
計画はフィールドを消す変異を値の区分に従わせるとし、max_entries は「上限の超過の検出と報告」の区分に当たるため決着の対象にしたが、検出と報告は変わらず、違いは TUI だけが使う値に出るため、記録だけの区分（`fail_on_truncation` が偽のときの扱い）に入れる案とした。この区分の当て方を利用者の判断に挙げる。

src/runtime/side_io.rs:783:9 について（実装を読んだ判断で、変異の下の出力は比べていない）: 変異でエージェントを有効にしたサーバのパスを指定した部分走査（merge・sync の末尾が "/" のパスや、ファイルのパスの親ディレクトリの走査）が、エージェントを使わず SSH の経路（`check_sudo_fallback` の後の `fetch_remote_tree_for_subpath`、find の走査）で行われる。全体の走査（`try_agent_fetch_tree_recursive`）はエージェントのままである。
二つの経路の違いとして分かっているものは、件数の数え方（ディレクトリを数えるか、FLAG-scan-010）、ちょうど上限の件数の扱い（FLAG-scan-011）、読めないディレクトリの扱い（FLAG-scan-013）、ディレクトリ symlink の配下の列挙（FLAG-scan-001）で、全て既存の FLAG の挙動である。これらに触れない構成では、二つの経路の一覧は同じになる（前の回と今回の SSH とエージェントの経路のテストで、同じ構成に同じ一覧を返すことを確かめている）。
変異を落とすテストは、(1) これらの FLAG の挙動の違いを確かめるもの（計画の止める条件に当たる）か、(2) 試験サーバが受けたコマンドで部分走査がエージェントの経路を通ったことを確かめるもの、になる。(2) は、走査の一覧のテストで「経路の取り違えを防ぐ前提の確認で、要件の観測ではない」とした確認で変異を落とすことになり、IR に部分走査がエージェントを使うことの要件はない。
そのため、既存の FLAG の範囲として記録し、テストは足さない案とした。部分走査の経路を要件にするかどうか（(2) のテストを足すか）を利用者の判断に挙げる。

同等変異の登録はしていない。この後にテストを足していないため、変異テストは回し直していない。

## 要件の verification の見直し

REQ-scan-002・003・004・005・008・009 の verification は全て unit で、いずれも件数と上限の大小、指定したパスの形、symlink と循環の有無、三つの経路という有限の場面の組で結果（エラーか一覧か、走査の範囲）が決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-scan-002 | unit | ディレクトリを指す symlink の先に具体的なファイルを置いた場面で、配下が上限の内で一覧に載るかと、循環や上限の超過を報告するかが決まる |
| REQ-scan-003 | unit | 上限を指定した場面と指定しない場面で、どの上限が当たるかが決まる |
| REQ-scan-004 | unit | 上限を超える場面と超えない場面で、上限の超過の報告か完全な一覧かが決まる |
| REQ-scan-005 | unit | 元または先の一覧が上限や循環で不完全な場面で、書き込みを始めないことが決まる |
| REQ-scan-008 | unit | 上限を超えた場面で、エラーの文の書き出しと三つの案内が決まる |
| REQ-scan-009 | unit | 判定表 TBL-scan-001 の行ごとの場面で走査の範囲が決まる。行は有限で、行ごとに具体的な構成で確かめられる |

## 利用者の判断

次の二件は決着の対象の見逃しで、記録だけにする案とした。区分の当て方の確認を求める。

1. src/agent/dispatch.rs:104:13（エージェントの走査に max_entries が渡らない）: CLI の上限の超過の検出と報告は変わらず、違いは `fail_on_truncation` が偽の TUI の走査が受け取る一覧だけのため、TUI の範囲として記録だけにする案。
2. src/runtime/side_io.rs:783:9（エージェントを有効にしたサーバの部分走査が SSH の経路に切り替わる）: 二つの経路の一覧の違いは既存の FLAG-scan-010・011・013・001 の挙動だけのため、その範囲として記録し、テストを足さない案。代わりに、部分走査がエージェントの経路を通ることを試験サーバのコマンドで確かめるテストを足す選び方もある（IR に経路の要件はない）。

利用者の判断（2026-09-29）:

1. src/agent/dispatch.rs:104:13（エージェントの走査に max_entries が渡らない）は、案どおり TUI の範囲（`fail_on_truncation` が偽の走査が受け取る一覧）として記録だけにする。引き継ぎ先は凍結中の TUI。
2. src/runtime/side_io.rs:783:9（エージェントを有効にしたサーバの部分走査が SSH の経路に切り替わる）は、案どおり既存の FLAG（FLAG-scan-010・011・013・001）の範囲として記録し、経路を確かめるだけのテストは足さない。

テストを変えていないため、変異テストは回し直していない。決着の対象の見逃しは残っていない。
