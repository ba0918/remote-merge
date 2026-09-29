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
