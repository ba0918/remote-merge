# 走査の一覧に載るもののテスト整理の記録

走査の一覧の取り込み（[決定記録](../decision/records/2026-09-29-adopt-scan-listing.md)）で加えた要件（REQ-scan-006・REQ-scan-007）と、既存の要件 REQ-scan-002 の SSH の経路の根拠テストを整えた過程の記録。
各要件の根拠にしたテスト、変異テストの結果、見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テスト 129 件のうち 6 件（src/ssh/tree_parser.rs の test_build_find_command_* 5 件と src/local/mod.rs の test_include_empty_scans_all）が実装の中身をなぞるだけのものだったが、元の単体テストは消さない方針のため、この整理ではテストを消していない。
テストを削除しない整理のため、整理前の変異テストは回していない（REQ-testing-012、[決定記録 A2](../decision/records/2026-09-28-mutation-scope.md#A2)）。

## 要件ごとの根拠テスト

手本にした元のテストは消さず、書き換えていない。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた（既存の挙動を確かめるテストのため、失敗する段階はない）。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。
FLAG-scan-001 から 007 の挙動（エージェントの経路でのディレクトリ symlink の配下、merge と sync での途中のディレクトリ symlink、FLAG に当たる exclude と include、ディレクトリを指定した merge と sync）は確かめない。件数の上限・循環・読めないディレクトリ（scan の次の回）と TUI の走査も確かめない。

### ローカルと SSH の経路（REQ-scan-006・007、REQ-scan-002）

根拠テストは `tests/contract/scan_listing_cli.rs` にある（`test-utils` の feature が要る。SSH の試験サーバを使うため）。
関数呼び出しで status を実行する既存の組み方（`RuntimeTargets::with_local`）はリモートの経路を通らないため、前の回の `tests/contract/config_filters_cli.rs` の `status` の補助と同じく、実行ファイルを試験 SSH サーバに対して起動する。

- `tests/common/mod.rs` の `TestDirs::new_2way` で一時ディレクトリと SSH の試験サーバを用意し、`gen_config`（エージェントは無効、既定の exclude ".git"・"target" のまま）で左右の root_dir を書いた設定を一時ディレクトリに書く。
- 起動の前に既存の隔離の確認 `TestDirs::assert_isolated_config_at` を通す。
- 起動には必ず `--config` でテストが書いた設定ファイルを渡し、作業ディレクトリを一時ディレクトリの下にする。渡さないと実行ファイルは作業ディレクトリの ".remote-merge.toml" を読むため。環境変数は `env_clear` のうえ `HOME`・`XDG_CONFIG_HOME`・`XDG_DATA_HOME` を一時ディレクトリの下に向け、`PATH` だけを引き継ぎ、標準入力を `Stdio::null()` にする。
- `status --left local --right develop --all --format json` の標準出力の JSON の "files" の "path" と "status" で確かめる。左の local と右の develop の root_dir に同じ構成を置き、確かめたい項目が "equal" で出ることを、左のローカルの経路と右の SSH の経路の両方がその項目を一覧に載せた証拠にする（片側の経路が載せなければ "left_only" か "right_only" になる）。

| 要件 | 根拠テスト | 元にしたテストと確かめること |
|---|---|---|
| REQ-scan-006 | status_lists_symlinks_themselves_with_their_target_text_over_ssh | tests/contract/status_judgement.rs の two_symlinks_are_compared_by_their_target_text_without_reading_the_content（関数呼び出し）。左右の root_dir に通常のファイル "target.txt"、それを指す "link"、存在しない "missing.txt" を指す "dangling"、root_dir の中の中身のないディレクトリ "emptydir" を指す "dirlink" を置き、"link"・"dangling"・"dirlink" が "equal" で出る。左右でリンク先の文字列が違う "retarget"（左は "a.txt"、右は "b.txt"、どちらも同じ中身の実在するファイル）が "modified" で出ることで、各経路がリンク先の文字列を一覧に持つことを確かめる |
| REQ-scan-007（末尾の "/" なし） | status_lists_the_files_below_a_root_dir_that_is_a_directory_symlink_over_ssh | 同じ組み方。左右の root_dir を、一時ディレクトリの中の実在するディレクトリ（"f.txt" と "sub/g.txt" を置く）を指す symlink にし、設定の root_dir を末尾の "/" なしで書く。一覧が "f.txt" と "sub/g.txt" の "equal" だけに一致する（`assert_eq!`） |
| REQ-scan-007（末尾の "/" 付き） | a_trailing_slash_on_a_symlinked_root_dir_does_not_change_the_list_over_ssh | 同じ構成で設定の root_dir を末尾の "/" 付きで書き、一覧が上と同じものに一致する |
| REQ-scan-002（EX-scan-003、SSH の経路） | status_lists_files_inside_a_directory_link_over_ssh | tests/contract/scan_limits.rs の status_lists_files_inside_a_directory_link_within_the_scan_limit（関数呼び出しでローカルの経路だけ）。左右の root_dir に、試験サーバの home の下で root_dir の外に置いた共有ディレクトリ（"alpha.txt" と "beta.txt"）を指す symlink "linked" を置き、"linked/alpha.txt" と "linked/beta.txt" が "equal" で出る。印は既存の例に合わせて EX-scan-003 |

- "dirlink" の先を中身のないディレクトリにしたのは、エージェントの経路がディレクトリ symlink の配下を載せない FLAG-scan-001 の挙動に触れないためで、配下のファイルについては何も確かめない。
- REQ-scan-007 は status・diff・merge・sync を名指しするが、パスを指定しない diff・merge・sync のツリーの取得は status と同じ関数（`CoreRuntime::fetch_tree_recursive`）を通るため、status で代表させた。
- REQ-scan-002 の件数の上限と循環の報告は scan の次の回の範囲のため、ここでは確かめない。

### エージェントの経路（REQ-scan-006・007）

根拠テストは同じ `tests/contract/scan_listing_cli.rs` にある。
起動の組み方は `tests/contract/ssh_fallback.rs` の an_available_remote_agent_completes_comparison_and_merge と同じで、`TestServer::filesystem_with_agent` で試験サーバを起動し、`[agent] enabled = true` と一時ディレクトリの下の deploy_dir を書いた設定で実行ファイルを起動する。
`TestDirs` の試験サーバはエージェントを起動せず、受けたコマンドも読めないため、試験サーバを直接使う補助 `AgentFixture` を新しいモジュールに置いた。

- 実行ファイルへの symlink を "{deploy_dir}/remote-merge-{user}/remote-merge" に置く。配置先はユーザー名で決まるため、設定の user と symlink のパスの user を同じ定数から作る。
- 既存の `assert_isolated_config_at` はエージェントが無効であることを求めるため、起動の前に、host が "127.0.0.1"、port がそのサーバのもので 22 でも 0 でもない、auth が "password"、key がない、sudo がないか false、strict_host_key_checking が "no"、左右の root_dir と deploy_dir が一時ディレクトリの下、であることを確かめる隔離の確認 `assert_isolated_agent_config` を同じモジュールに置いた（試験サーバはコマンドを実際の `sh -c` で実行するため、sudo が有効だとホストで sudo が走る）。
- 起動の補助（`--config`、作業ディレクトリ、環境変数、標準入力）は SSH の経路と共有する。手本の ssh_fallback のテストは `env_clear` をしていないが、ここでは SSH の経路と同じく環境変数を全て消す。
- 起動の後、`TestServer::commands()` に " agent --root " を含むコマンドがあり、"find -L" で始まるコマンド（SSH の経路の走査）がないことを確かめる。経路の取り違えを防ぐ前提の確認で、要件の観測ではない。使い捨ての出力で確かめたところ、試験サーバが受けたコマンドはエージェントの版の確認とエージェントの起動の二つだけだった。

| 要件 | 根拠テスト | 確かめること |
|---|---|---|
| REQ-scan-006 | status_lists_symlinks_themselves_with_their_target_text_via_the_agent | SSH の経路の REQ-scan-006 のテストと同じ構成と期待（"link"・"dangling"・"dirlink" が "equal"、"retarget" が "modified"） |
| REQ-scan-007（末尾の "/" なし） | status_lists_the_files_below_a_root_dir_that_is_a_directory_symlink_via_the_agent | SSH の経路と同じ構成と期待（一覧が "f.txt" と "sub/g.txt" の "equal" だけ） |
| REQ-scan-007（末尾の "/" 付き） | a_trailing_slash_on_a_symlinked_root_dir_does_not_change_the_list_via_the_agent | 同じ構成で root_dir を末尾の "/" 付きで書き、一覧が同じものに一致する |

- エージェントの経路では REQ-scan-002 を確かめない（FLAG-scan-001 の範囲）。

## 整理後の変異テスト

変異テストは決着の対象の関数に絞って一度だけ実行した。並列数は既定の 2 である。
実行したのはテストと記録を足した後のコミット ab8b22a で、作業ツリーに変更のない状態で行い、実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh --re '(walk_single_root|scan_local_tree_recursive_with_include|resolve_scan_roots?\b|build_local_tree_from_flat|parse_find_line|build_find_command|build_tree_from_flat|list_tree_recursive|resolve_include_roots|advance_one|next_valid_path|process_entry|convert_agent_entries_to_nodes)' src/local/mod.rs src/ssh/tree_parser.rs src/ssh/client.rs src/agent/tree_scan.rs src/agent/dispatch.rs
```

結果は `mutants: caught=64 survived=12 timeout=2 unviable=14 equivalent=0`（92 件）。スクリプトの終了コードは 1 で、kotowari mutants が見逃しを error として報告したためである（メモリ上限での停止ではない）。

### 実行の前の一覧

実行の前に、同じ `--re` とファイルで `cargo mutants --list --all-features`（テストを走らせない一覧の表示）をメモリ上限の中で実行し、92 件の変異を確かめた。計画に挙げた関数は全て変異の名前に現れた。
`resolve_scan_roots?\b` は src/local/mod.rs の `resolve_scan_roots`（7 件）と src/agent/dispatch.rs の `resolve_scan_root`（4 件）の両方に当たった。
正規表現に名前の一致する関数の外の変異として、構造体のフィールドを消す変異が 7 件混ざった（`Dispatcher::handle_list_tree` の `ScanOptions` の root・exclude・include・max_entries の 4 件と、`SshClient::build_client_config` の `client::Config` の inactivity_timeout・keepalive_interval・keepalive_max の 3 件）。cargo-mutants 27.1.0 がフィールドを消す変異を `--re` で除かないためで、計画の区分により他の関数のものとして記録だけする。

### 関数ごとの内訳

`outcomes.json` から数えた。括弧の中は、下の「負荷の下で落ちるテストだけに検知された変異」で見逃しとして扱う件数で、caught の数に含まれる。

| ファイル | 関数 | 一覧の件数 | caught | survived | timeout | unviable |
|---|---|---|---|---|---|---|
| src/local/mod.rs | resolve_scan_roots | 7 | 7 | 0 | 0 | 0 |
| src/local/mod.rs | scan_local_tree_recursive_with_include | 7 | 5 | 0 | 0 | 2 |
| src/local/mod.rs | walk_single_root | 12 | 8 | 0 | 0 | 4 |
| src/local/mod.rs | build_local_tree_from_flat（中の insert_into_tree を含む） | 6 | 4（1） | 1 | 0 | 1 |
| src/ssh/tree_parser.rs | parse_find_line | 13 | 10（1） | 2 | 0 | 1 |
| src/ssh/tree_parser.rs | build_tree_from_flat（中の dir_first_sort・into_file_node・insert_into_tree を含む） | 10 | 6（1） | 1 | 0 | 3 |
| src/ssh/tree_parser.rs | build_find_command | 2 | 2 | 0 | 0 | 0 |
| src/ssh/client.rs | SshClient::list_tree_recursive | 5 | 3 | 0 | 0 | 2 |
| src/ssh/client.rs | SshClient::build_client_config（フィールドを消す変異） | 3 | 0 | 3 | 0 | 0 |
| src/agent/tree_scan.rs | advance_one | 2 | 1 | 0 | 1 | 0 |
| src/agent/tree_scan.rs | next_valid_path | 2 | 1 | 0 | 1 | 0 |
| src/agent/tree_scan.rs | process_entry | 6 | 5 | 1 | 0 | 0 |
| src/agent/tree_scan.rs | resolve_include_roots | 7 | 7 | 0 | 0 | 0 |
| src/agent/tree_scan.rs | convert_agent_entries_to_nodes | 2 | 1 | 0 | 0 | 1 |
| src/agent/dispatch.rs | resolve_scan_root | 4 | 2 | 2 | 0 | 0 |
| src/agent/dispatch.rs | Dispatcher::handle_list_tree（フィールドを消す変異） | 4 | 2 | 2 | 0 | 0 |

unviable の 14 件は全て、関数の戻り値を `Default::default()` を含む値にする変異で、`FileNode` が `Default` を実装しないため組み立てられない。
timeout の 2 件（src/agent/tree_scan.rs:89:9 の advance_one を `Some(())` にする変異と、123:9 の next_valid_path を `Some(Default::default())` にする変異）は、エージェントの走査が終わらなくなる変異で、テストが時間切れで止まった。kotowari mutants では notice で、見逃しとしては数えない。

`resolve_scan_roots` の include の分岐は、前の回の[記録](./config-filters-test-cleanup.md)の 7 件全て caught と同じく、この回も 7 件全て caught だった。

### 検知したテスト

caught と数えられた 64 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗から少し進んで止まるため、集めた名前は先に失敗したものだけで、この回に足したテストの名前は一件も現れなかった（先に src/ の既存の単体テストや既存の契約テストが失敗した）。
そのため、足したテストが要件の変異で落ちることを、変異を一時的に書き入れて `cargo nextest run --all-features --no-fail-fast --test contract -E 'test(/scan_listing/)'` を回して確かめた。どれも確かめた後に `git checkout` で戻し、`git diff --stat src/` が空に戻ることを確かめた。

| 変異 | 落ちた足したテスト（7 件中） |
|---|---|
| src/ssh/tree_parser.rs:56:9 delete match arm "l" in parse_find_line（SSH の経路で symlink をファイルとして載せる） | status_lists_symlinks_themselves_with_their_target_text_over_ssh、status_lists_files_inside_a_directory_link_over_ssh（2 件） |
| src/ssh/client.rs:615:9 list_tree_recursive を `Ok((vec![], false))` にする（SSH の経路が何も載せない） | SSH の経路の 4 件全て |
| src/local/mod.rs:145:5 resolve_scan_roots を `vec![]` にする（ローカルの経路が何も載せない） | 7 件全て（左は全ての起動でローカルの経路のため） |
| src/agent/tree_scan.rs:273:5 resolve_include_roots を `vec![]` にする（エージェントの経路が何も載せない） | エージェントの経路の 3 件全て |
| src/local/mod.rs:306:24 walk_single_root の match guard を false にする（ローカルの経路でリンク先がない symlink を載せず走査を失敗させる） | status_lists_symlinks_themselves_with_their_target_text_over_ssh、status_lists_symlinks_themselves_with_their_target_text_via_the_agent（2 件） |

### 負荷の下で落ちるテストだけに検知された変異

次の 3 件は caught と数えられたが、失敗したのは tests/tui_merge.rs のテストだけだった。
変異を一時的に書き入れて、そのテストだけを `cargo nextest run --all-features --no-fail-fast --test tui_merge -E '…'` で回し直すと全て通ったため、負荷の下で落ちるテストが落ちたものと判断し、見逃しとして扱う（下の「見逃しと決着」）。どれも確かめた後に `git checkout` で戻し、`git diff --stat src/` が空に戻ることを確かめた。

| 変異 | ログで失敗したテスト | 回し直しの結果 |
|---|---|---|
| src/local/mod.rs:414:9 delete match arm (false, true) in build_local_tree_from_flat | test_hunk_merge_left_to_right_with_l、test_hunk_merge_right_to_left_with_h_key | 2 件とも通った |
| src/ssh/tree_parser.rs:50:52 replace * with + in parse_find_line | test_hunk_merge_left_to_right_with_l、test_hunk_merge_right_to_left_with_h_key、test_merge_cancel_with_n、test_sensitive_file_merge_requires_confirmation | 4 件とも通った |
| src/ssh/tree_parser.rs:80:13 delete match arm (false, true) in build_tree_from_flat::dir_first_sort | test_sensitive_file_merge_requires_confirmation | 通った |

agent_ssh のテストだけに検知された変異はなかった。

### 見逃しと決着

見逃しは、survived の 12 件と、上の負荷の下で落ちるテストだけに検知された 3 件の、合わせて 15 件である。
計画の区分により、exclude と include の当て方、件数の上限・循環・読めないディレクトリ、パスを指定した部分走査、サイズ・更新時刻・パーミッションの値、エージェントの経路でのディレクトリ symlink の配下、TUI だけが使う値、対象の関数の外のものは決着の対象から外して記録だけする。
ここに挙げた見逃しは全て survived（または負荷の下の失敗だけ）のため、既存の単体テストでも落ちていない。

#### 記録だけするもの（計画の区分に当てはまる）

| 位置 | 変異 | 行の中身 | 区分 | 記録 |
|---|---|---|---|---|
| src/agent/dispatch.rs:103:13 | delete field include from struct ScanOptions expression in Dispatcher::handle_list_tree | `include: include.to_vec(),` | 対象の関数の外（include の当て方、REQ-config-004・023 の範囲） | エージェントの走査に include が渡らなくなる |
| src/agent/dispatch.rs:104:13 | delete field max_entries from struct ScanOptions expression in Dispatcher::handle_list_tree | `max_entries,` | 対象の関数の外（件数の上限、scan の次の回） | エージェントの走査の件数の上限が既定値になる |
| src/agent/dispatch.rs:383:12 | delete ! in resolve_scan_root | `if !canonical.starts_with(&root_canonical) {` | パスを指定した部分走査（scan の次の回） | 起点のパスが空でないときの root_dir の外の判定 |
| src/agent/tree_scan.rs:203:49 | replace && with \|\| in ScanIterator<'a>::process_entry | `link_is_dir: file_type.is_symlink() && path.is_dir(),` | エージェントの経路の link_is_dir（FLAG-scan-001 と TUI の範囲） | この行に来るのは symlink とファイルだけ（ディレクトリは前で走査の待ち行列に入れて戻る）で、ファイルでは `path.is_dir()` が偽のため、変異の前後で値は変わらないと読める（実装を読んだ判断） |
| src/ssh/client.rs:213:13・214:13・215:13 | delete field inactivity_timeout / keepalive_interval / keepalive_max from struct client::Config expression in SshClient::build_client_config | `inactivity_timeout: Some(Duration::from_secs(inactivity_secs)),` ほか | 対象の関数の外（SSH の接続の設定で、走査の一覧とは関係しない） | 接続の無通信時間と keepalive が既定値になる |
| src/ssh/tree_parser.rs:50:52 | replace * with + in parse_find_line（負荷の下の失敗だけ） | `Utc.timestamp_opt(ts as i64, ((ts.fract()) * 1_000_000_000.0) as u32)` | 更新時刻の値（`docs/testing/status-test-cleanup.md` の REQ-cli-027 の範囲） | 更新時刻のナノ秒の部分が変わる |

#### 決着の対象の見逃しと、別の文脈のエージェントによる試み

次の 6 件は計画の記録だけの区分に直接は当てはまらないため決着の対象にし、同等変異の登録の前に、別の文脈のエージェントに「変異を入れると失敗し、元のコードでは通るテスト」を書かせた。
観測は利用者に見える振る舞い（実行ファイルの status・diff・merge・sync の標準出力・標準エラー・終了コードと書き込み結果、公開関数の結果）に限った。
エージェントは、試験 SSH サーバ（エージェント無効）に対して、トップレベルにディレクトリとファイルを名前順で交互に 30 件以上置いた構成と、左右で種類の食い違う項目（ディレクトリと、ファイルを指す symlink など）を置いた構成で、`status`・`status --all --format json`・`diff . --max-files 0`（テキストと JSON）・`diff`（項目を指定）・`merge . --dry-run --force --delete --format json`・`merge`（項目を指定、--dry-run）・`sync . --dry-run --force --delete --format json` を左右の向きの両方で実行し、元のコードで記録した出力（一時パスを置き換え、時刻の入る WARN の行を除いたもの）と一件ずつ変異を入れたときの出力を比べた。

| 位置 | 変異 | 行の中身 | 試みの結果 | 決着 |
|---|---|---|---|---|
| src/local/mod.rs:413:9 | delete match arm (true, false) in build_local_tree_from_flat | `(true, false) => std::cmp::Ordering::Less,` | 書けなかった。全てのコマンドで出力が一致した。ローカルの走査の結果は CLI の経路では必ず `FileTree::sort()`（src/tree.rs の `sort_nodes`）にかかり（src/runtime/target_io.rs、src/cli/merge.rs・diff.rs・sync.rs など）、status も比べるパスを並べ直す（src/service/status.rs の `collect_all_file_paths`）。並べ直さずに使うのは TUI の走査（src/runtime/scanner.rs）だけ | 記録だけする（TUI だけが使う値。TUI の表示の並びでだけ違いうる。TUI での確認はしていない） |
| src/local/mod.rs:414:9 | delete match arm (false, true) in build_local_tree_from_flat（負荷の下の失敗だけ） | `(false, true) => std::cmp::Ordering::Greater,` | 同じ | 同じ |
| src/ssh/tree_parser.rs:80:13 | delete match arm (false, true) in build_tree_from_flat::dir_first_sort（負荷の下の失敗だけ） | `(false, true) => std::cmp::Ordering::Greater,` | 同じ。SSH とエージェントの経路でも `build_tree_from_flat` の後に `tree.sort()` がかかる（src/runtime/core.rs、src/runtime/side_io.rs） | 同じ |
| src/ssh/tree_parser.rs:131:34 | replace && with \|\| in build_tree_from_flat::insert_into_tree | `if node.is_dir() && node.children.is_none() {` | 書けなかった。末端のファイルと symlink の children が未取得（None）から空（Some）になるが、status の `record_node` はどちらでもそのパスを一覧に積むだけで、他の CLI の経路も種類で判定するか children を運ぶだけだった。空と未取得を見分ける `find_node_or_unloaded` を使うのは src/app/badge.rs と src/handler/reconnect.rs（TUI）だけ | 記録だけする（TUI だけが使う値。ディレクトリの children を未取得にする扱いと同じ区分） |
| src/ssh/tree_parser.rs:61:41 | replace && with \|\| in parse_find_line | `node.link_is_dir = file_type == "l" && parts.get(6).is_some_and(\|kind\| *kind == "d");` | 書けなかった。変異で SSH の経路のファイルを指す symlink とリンク先のない symlink も link_is_dir が真になり、`into_file_node` でその children が空（Some）になるが、上と同じ理由で CLI の出力は変わらなかった。link_is_dir を読むのは `into_file_node` と TUI（src/app/tree_ops.rs、src/handler/tree_keys.rs）だけ。ディレクトリを指す symlink はもともと真のため、配下の列挙は変わらない | 記録だけする（TUI だけが使う値）。計画は SSH の経路の link_is_dir を「配下の列挙として決着の対象」としていたが、この変異は配下の列挙を変えないため TUI の範囲に入れた。この区分の当て方を利用者の判断に挙げる |
| src/agent/dispatch.rs:357:24 | replace \|\| with && in resolve_scan_root | `if root.is_empty() \|\| root == "." {` | 書けた。ただし元のコードの振る舞いが不具合の疑いを示した（下） | FLAG-scan-008 の範囲として決着（[決定記録 A11](../decision/records/2026-09-29-adopt-scan-listing.md#A11)）。テストは足さない |

src/agent/dispatch.rs:357:24 について: エージェントは、左右の root_dir をディレクトリを指す symlink にし、`[filter] include = ["sub"]` を書いた設定で、エージェントを有効にした status を実行するテストを書いた。元のコードでは右の一覧に何も載らず、"sub/g.txt" が "left_only" になり、変異の下では "equal" になった。
この回でも、そのテストの本文を一時的なモジュールに置いて `cargo nextest run --all-features --no-fail-fast --test contract -E 'test(/zz_probe/)'` で回し、元のコードで通り（"left_only" を期待する形）、変異を書き入れると `left: [("sub/g.txt", "equal")] right: [("sub/g.txt", "left_only")]` で落ちることを確かめた。確かめた後にモジュールを消して `git checkout` で戻し、`git status --short` と `git diff --stat src/` が空に戻ることを確かめた。
元のコードでは、エージェントは走査の起点を root_dir の symlink のパスのままにし、include の起点は `resolve_include_roots` が実パスに直すため、src/agent/tree_scan.rs の `process_entry` で実パスの下の項目を起点からの相対パスにできず（`strip_prefix` の失敗）、全ての項目を捨てる（実装を読んだ判断）。エージェントの報告では、同じ構成をエージェントを無効にした SSH の経路で実行すると "sub/g.txt" は "equal" になった（この回では確かめていない）。
変異は起点を実パスに直すため、この食い違いを直す方向に働く。変異を落とすテストは、この不具合らしい振る舞いを固定するテストになるため、足していない。利用者の判断で FLAG-scan-008 として残し、修正はこの整理の後に回す。

#### 不具合の疑いとして報告するもの

| 位置 | 変異 | 行の中身 | 記録 |
|---|---|---|---|
| src/ssh/tree_parser.rs:19:20 | replace < with <= in parse_find_line | `if parts.len() < 5 {` | FLAG-scan-009 の範囲として決着（[決定記録 A12](../decision/records/2026-09-29-adopt-scan-listing.md#A12)）。build_find_command の出力は、通常の項目で 6 列、symlink で 7 列のため、ちょうど 5 列の行は、名前に改行を含むファイルの出力が行の途中で分かれたときにだけできる（実装を読んだ判断で、実行はしていない）。元のコードはその断片を名前の途中までのファイルとして一覧に載せ、変異はその断片を捨てる。名前に改行を含むファイルの扱いは IR にも旧資料にもなく、どちらの振る舞いを確かめるテストも、要件のない振る舞いを固定することになるため、テストを足していない |

同等変異の登録はしていない（決着の対象の見逃しのうち、試みで書けなかった 5 件は、CLI の出力では違いが出ないが TUI では違いうるため、「全ての観測で同等」とは言えず、記録だけする区分に入れた）。
計画に従い、この後にテストを足していないため、変異テストは回し直していない。

## 要件の verification の見直し

REQ-scan-002・006・007 の verification は全て unit で、いずれも symlink の種類（ファイルを指す・リンク先がない・ディレクトリを指す）、root_dir の形（symlink か、末尾の "/" の有無）と三つの経路という有限の場面の組で一覧が決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-scan-002 | unit | ディレクトリを指す symlink の先に具体的なファイルを置いた場面で、配下が一覧に載るかが決まる（件数の上限と循環の報告は scan の次の回で確かめる） |
| REQ-scan-006 | unit | symlink の三つの種類とリンク先の文字列の違いという有限の場面と三つの経路の組で、一覧に載るかと比べ方が決まる |
| REQ-scan-007 | unit | root_dir が symlink であることと末尾の "/" の有無の二つの場面で、一覧が決まる。status・diff・merge・sync のうち status で代表させた（パスを指定しない diff・merge・sync のツリーの取得は status と同じ `CoreRuntime::fetch_tree_recursive` を通るため） |

## 利用者の判断

次の二件の見逃しが不具合の疑いを示したため、未決着のまま報告した。どちらも新しい FLAG の候補だった。

1. src/agent/dispatch.rs:357:24: root_dir がディレクトリを指す symlink で include を書いたとき、エージェントの経路は右の一覧に何も載せない（実行で確かめた）。ローカルの経路は載せ、エージェントの報告では SSH の経路も載せる。REQ-scan-007（root_dir の symlink を辿る）と REQ-config-023（include の対象を走査する）の組み合わせに反する疑いがある。既存の FLAG-scan-006（include にディレクトリを指す symlink を書いたとき）とは、symlink が root_dir 自体である点で別の場合である。
2. src/ssh/tree_parser.rs:19:20: 名前に改行を含むファイルの SSH の経路での扱い（find の出力の行の分け方）が IR にない（実装を読んだ判断）。

次の一件は、計画の区分の当て方の確認を求める。

3. src/ssh/tree_parser.rs:61:41: 計画は SSH の経路の link_is_dir を配下の列挙として決着の対象としていたが、この変異は配下の列挙を変えず、CLI の出力でも違いが出なかったため、TUI だけが使う値として記録だけにした。

利用者の判断（2026-09-29）:

1. src/agent/dispatch.rs:357:24（root_dir の symlink と include を併せたエージェントの経路）は、新しい FLAG として残した（[決定記録 A11](../decision/records/2026-09-29-adopt-scan-listing.md#A11)、FLAG-scan-008）。修正はこの整理の後に回す。この見逃しは FLAG-scan-008 の範囲として決着させ、テストは足さない。
2. src/ssh/tree_parser.rs:19:20（名前に改行を含むファイルの SSH の経路）は、欠落の FLAG として残した（[決定記録 A12](../decision/records/2026-09-29-adopt-scan-listing.md#A12)、FLAG-scan-009）。この見逃しは FLAG-scan-009 の範囲として決着させる。
3. src/ssh/tree_parser.rs:61:41（SSH の経路の link_is_dir）を TUI だけが使う値として記録だけにする区分の当て方は了承された。

テストを変えていないため、変異テストは回し直していない。決着の対象の見逃しは残っていない。
