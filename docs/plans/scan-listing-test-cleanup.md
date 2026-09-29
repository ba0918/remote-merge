# Plan: 走査の一覧に載るもののテストを要件の根拠に整理する

## Goal

走査の一覧の取り込みで加えた要件に、ローカル・SSH・エージェントの各経路で要件を確かめる印付きのテストが kotowari の検査範囲にあり、変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/scan/listing.md#REQ-scan-006`、`docs/ir/scan/listing.md#REQ-scan-007`
- 既存の `docs/ir/scan/directory-links.md#REQ-scan-002`（SSH の経路の根拠を足すことと、verification の見直しだけ。要件の文は変えない）
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`（いずれも review）

判断の出典は `docs/decision/records/2026-09-29-adopt-scan-listing.md`（取り込み、FLAG-scan-001 から 007、テストの仕分けの件数）、`docs/decision/records/2026-09-27-test-method-selection.md`、`docs/decision/records/2026-09-28-mutation-scope.md`。要件の本文は `kotowari query REQ-scan-006` のように読む。前の回の記録 `docs/testing/config-filters-test-cleanup.md`（実行ファイルを試験 SSH サーバに対して起動する組み方と、変異テストの記録の形）が手本になる。新しいテストの印は要件の ID（`@kotowari[REQ-scan-006]`）で付け、REQ-scan-002 の SSH の経路のテストは既存の例に合わせて `@kotowari[EX-scan-003]` を付ける。

## Approach and why

取り込みの仕分けでは、この範囲のテスト 129 件のうち実装の中身をなぞるだけのテストは 6 件（`src/ssh/tree_parser.rs` の `test_build_find_command_*` 5 件と `src/local/mod.rs` の `test_include_empty_scans_all`）だった。これまでの回と同じく元の単体テストは消さないため、削除は行わず、最初の変異テストは省いて最後に一度だけ回す。既存のテストは消さず、書き換えない。製品コードは変えない。

要件は三つの経路のそれぞれで確かめる必要があり（REQ-scan-006 は「どの経路でも」、REQ-scan-007 は旧資料が経路ごとの確認を求めていた）、関数呼び出しで status を実行する既存の組み方（`RuntimeTargets::with_local`）はリモートの経路を通らない。そのため実行ファイルを試験 SSH サーバに対して `status --left local --right <サーバ> --all --format json` で起動し、標準出力の JSON の "files" の "path" と "status" で観測する。左の local と右のリモートの root_dir に同じ構成を置き、確かめたい項目が "equal" で出ることを、左右の両方の経路がその項目を一覧に載せた証拠にする（片側の経路が載せなければ "left_only" になる）。一つの起動でローカルの経路と右の経路を同時に確かめられる。

- どちらの経路でも、起動には必ず `--config <テストが一時ディレクトリに書いた設定ファイル>` を渡し、`current_dir` を一時ディレクトリの下にする。`--config` がないと実行ファイルは作業ディレクトリの ".remote-merge.toml" を読み、リポジトリ直下のそのファイルは開発者の実際の SSH 接続先と鍵を指しているため、隔離の確認（テストが書いた設定ファイルだけを見る）では防げない。環境変数は env_clear のうえ HOME・XDG_CONFIG_HOME・XDG_DATA_HOME を一時ディレクトリの下に向け、PATH だけを引き継ぎ、標準入力を null にする（`tests/contract/config_filters_cli.rs` の `status` の補助と同じ）。
- SSH の経路: `tests/common/mod.rs` の `TestDirs::new_2way` と `gen_config`（エージェントは無効）で設定を作り、起動の前に `TestDirs::assert_isolated_config_at` で隔離を確かめる。gen_config が書く既定の exclude（".git"・"target"）はそのまま使ってよい。
- エージェントの経路: `tests/contract/ssh_fallback.rs` の `an_available_remote_agent_completes_comparison_and_merge` と同じく、`TestServer::filesystem_with_agent` と `[agent] enabled = true` の設定で起動する。実行ファイルへの symlink は、設定の user を使った "{deploy_dir}/remote-merge-{user}/remote-merge" に置く（配置先はユーザー名で決まるため、設定の user と symlink のパスの user を揃える）。既存の `assert_isolated_config_at` はエージェントが無効であることを求めるため、起動の前に、host が "127.0.0.1"、port がそのサーバのもので 22 でも 0 でもない、auth が "password"、key がない、sudo がないか false、strict_host_key_checking が "no"、左右の root_dir と deploy_dir が一時ディレクトリの下、であることを確かめる隔離の確認を新しく置く（試験サーバはコマンドを実際の `sh -c` で実行するため、sudo が有効だとホストで sudo が走る）。起動の後、`TestServer::commands()` に " agent --root " を含むコマンドがあり、"find -L" で始まるコマンド（SSH の経路の走査）がないことを確かめ、エージェントの経路で走査したことを保証する（経路の取り違えを防ぐ前提の確認で、要件の観測ではない）。
- 新しいモジュールは、試験サーバと `common` を使う既存のモジュールと同じく `tests/contract.rs` で `#[cfg(feature = "test-utils")]` の下に宣言する。root_dir の外に置く共有ディレクトリなどの一時的な置き場は、試験サーバの home（`TestDirs` の `temp`）の下、root_dir の外に作る。

要件ごとの場合:

- REQ-scan-006: 左右の root_dir に、通常のファイル "target.txt"、それを指す symlink "link"、存在しない "missing.txt" を指す symlink "dangling"、中身のない同じ root_dir の中のディレクトリを指す symlink "dirlink" を置き、"link"・"dangling"・"dirlink" が "equal" で出ることを確かめる。加えて、左右でリンク先の文字列が違う symlink "retarget"（左は "a.txt"、右は "b.txt"、どちらも同じ中身の実在するファイル）が "modified" で出ることで、各経路がリンク先の文字列を一覧に持つことを確かめる。"dirlink" の先を中身のないディレクトリにするのは、エージェントの経路がディレクトリ symlink の配下を載せない FLAG-scan-001 の挙動に触れないためで、配下のファイルについては何も確かめない。
- REQ-scan-007: 左右の root_dir を、それぞれ実在するディレクトリ（"f.txt" と "sub/g.txt" を置く）を指す一時ディレクトリの中の symlink にして、設定の root_dir を末尾の "/" なしで書いた起動と、末尾の "/" 付きで書いた起動のそれぞれで、"f.txt" と "sub/g.txt" がどちらも "equal" で出て、それ以外の項目が出ないことを確かめる。要件は status・diff・merge・sync を名指しするが、パスを指定しない diff・merge・sync のツリーの取得は status と同じ関数（`CoreRuntime::fetch_tree_recursive`）を通るため、status で代表させる。この代表させ方は S3 の verification の見直しに一行残す。
- REQ-scan-002（SSH の経路だけ）: 既存の `tests/contract/scan_limits.rs` の `status_lists_files_inside_a_directory_link_within_the_scan_limit`（EX-scan-003、関数呼び出しでローカルの経路だけ）と同じ場合を、左右の root_dir の中に root_dir の外の共有ディレクトリを指す symlink "linked" を置いて SSH の経路で起動し、"linked/alpha.txt" と "linked/beta.txt" が "equal" で出ることで確かめる。エージェントの経路は FLAG-scan-001 のため確かめない。

新しく書くテストは FLAG-scan-001 から 007 の挙動を確かめない。具体的には、エージェントの経路でのディレクトリ symlink の配下、merge と sync での途中のディレクトリ symlink、FLAG に当たる exclude や include を設定に足すこと、ディレクトリを指定した merge と sync を検証の対象に含めない。件数の上限・循環・読めないディレクトリ（scan の次の回）と TUI の走査も確かめない。

変異テストは決着の対象の関数に絞って一度だけ回す: `scripts/mutants.sh --re '(walk_single_root|scan_local_tree_recursive_with_include|resolve_scan_roots?\b|build_local_tree_from_flat|parse_find_line|build_find_command|build_tree_from_flat|list_tree_recursive|resolve_include_roots|advance_one|next_valid_path|process_entry|convert_agent_entries_to_nodes)' src/local/mod.rs src/ssh/tree_parser.rs src/ssh/client.rs src/agent/tree_scan.rs src/agent/dispatch.rs`。`resolve_scan_roots?\b` は `src/local/mod.rs` の `resolve_scan_roots`（include が空のとき root_dir の symlink を実パスに直す）と `src/agent/dispatch.rs` の `resolve_scan_root`（root が空のとき root_dir をそのまま返す）の両方に当てる。エージェントの経路では root_dir を実パスに直さず、`resolve_include_roots` が include の空のとき root_dir を返し、`advance_one` の read_dir が root_dir の symlink を辿る。回す前に、同じ `--re` とファイルで `cargo mutants --list`（テストを走らせない一覧の表示。`scripts/mutants.sh` と同じく systemd-run のメモリ上限の中で実行してよい）を実行し、関数ごとの変異の件数を記録に残して、上の関数が全て変異の名前に現れることを確かめる。

このうち次の見逃しは範囲として記録だけする: exclude と include の当て方（REQ-config-003・004・021 から 025 の範囲。FLAG-scan-003 から 007 に当たるものはその FLAG の範囲）、件数の上限・循環・読めないディレクトリの扱い（scan の次の回の範囲）、パスを指定した部分走査（`resolve_scan_root` の root が空でない分岐など、scan の次の回の範囲）、サイズ・更新時刻・パーミッションの値（`docs/testing/status-test-cleanup.md` の REQ-cli-027 の範囲）、エージェントの経路でのディレクトリ symlink の配下（FLAG-scan-001 の範囲）、TUI だけが使う値（ディレクトリの children を未取得にする扱いなど、TUI の範囲）。`resolve_scan_roots` の include の分岐は前の回の `docs/testing/config-filters-test-cleanup.md` の記録（7 件全て caught）と比べる。これらの範囲の見逃しは、既存の単体テストで落ちているかどうかも記録する。それ以外の見逃し（symlink を項目にしてリンク先の文字列を持たせること、リンク先がない symlink を載せること、root_dir の symlink を辿ること、末尾の "/" の扱い、SSH とローカルの経路でディレクトリ symlink の配下を列挙すること）は全て決着の対象にする。cargo-mutants 27.1.0 では構造体のフィールドを消す変異が `--re` で除かれないため、対象の関数の中のものはそのフィールドの値の区分に従う: サイズ・更新時刻・パーミッションは記録だけ、リンク先の文字列と種類（file・directory・symlink）は決着の対象、ディレクトリを指す symlink の印（link_is_dir）はローカルと SSH の経路では配下の列挙として決着の対象でエージェントの経路では FLAG-scan-001 と TUI の範囲として記録だけにする。他の関数のものは記録だけする。負荷の下で落ちるテスト（tui_merge や agent_ssh）だけに検知された変異は、名前とともに記録する。

最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。見逃しを落とすためにテストを足したら、その状態で変異テストを回し直してから記録する。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。変異の一時的な書き換えで確かめるときは本体の作業ツリーで行い、確かめた後に `git diff --stat src/` が空に戻ることを確かめる。作業ツリーやブランチは作らない。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の走査用のモジュール（新規作成。SSH の経路とエージェントの経路の実行ファイルのテスト。既存のテストの中身は変えない）
- `tests/common/mod.rs`（エージェントを有効にした設定の隔離の確認を足すことだけ。既存の関数の挙動は変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/scan-listing-test-cleanup.md`（新規。根拠にしたテスト、変異テストの結果と見逃しの決着、verification の見直しの記録）

## Step order and prerequisites

S1 は SSH の経路、S2 はエージェントの経路の根拠テストで、同じモジュールと記録の文書を書き換えるため順に行う。S3 で変異テストを一度だけ回して見逃しを決着させる。S4 で全体を確かめる。

S3 には利用者の判断を待つ区切りがある。新しい FLAG の候補、verification の見直しの候補、決着の対象でない見逃しで範囲の区分に当てはまらないものがあれば、`docs/testing/scan-listing-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/scan`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-scan-006, REQ-scan-007, REQ-scan-002（SSH の経路） | EX-scan-003（SSH の経路） |
| S2 | REQ-scan-006, REQ-scan-007 | — |
| S3 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S4 | 上記すべて | — |

## Left to the implementer

- 新しいテストのモジュールの名前と、テストの分け方
- SSH の経路とエージェントの経路の起動の補助を共有するか、別々に置くか
- エージェントの経路の隔離の確認を `tests/common/mod.rs` に置くか、新しいモジュールに置くか

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 要件を確かめるために FLAG-scan-001 から 007 の挙動を確かめる必要が生じた
- 要件の verification が要件の性質に合わないと判断した（IR は直さず、S3 で候補として手渡す）
- 要件を確かめるために製品コードの挙動や公開範囲を変える必要が生じた
- 変異テストの見逃しや新しいテストが不具合の疑いを示した（見逃しは未決着のまま報告する）
- 変異テストが WSL のメモリを使い切りそうな兆候（スクリプトの上限に当たって失敗し続ける）があり、並列数を下げても解消しない

## Test command

```sh
cargo nextest run --all-features
```

変異テストは `scripts/mutants.sh` から実行し、cargo-mutants を直接実行しない（例外は Approach and why の `cargo mutants --list` だけ）。並列数を 2 より上げない。

## Out of scope

- FLAG-scan-001 から FLAG-scan-007 の決着と、その挙動の修正
- 件数の上限・循環・読めないディレクトリ・find のタイムアウト（scan の次の回）
- TUI の走査と表示（REQ-scan-001）
- 既存の単体テストの書き換えと削除（実装の中身をなぞるだけの 6 件も消さない）
- 面の検査（`surface`）の導入

## Steps

### S1: SSH の経路の根拠テストを実行ファイルで書いて印を付ける

- Purpose: symlink の項目・root_dir の symlink・ディレクトリ symlink の配下を、ローカルと SSH の経路で一覧に載せることに、実行ファイルを通す根拠テストを置く
- Specification: docs/ir/scan/listing.md#REQ-scan-006, docs/ir/scan/listing.md#REQ-scan-007, docs/ir/scan/directory-links.md#REQ-scan-002
- Prerequisites: none
- May change: tests/contract.rs, tests/contract/ の走査用モジュール, docs/testing/scan-listing-test-cleanup.md
- Done when: REQ-scan-006 と REQ-scan-007 の `kotowari query` の tests に SSH の経路の印付きテストがあり、EX-scan-003 の tests に SSH の経路のテストが足されている。各テストは Approach and why の場合を status の JSON の "files" の "path" と "status" で確かめ、起動は隔離の確認を通してから行う。どのテストを根拠にし、どの既存テストの組み方を手本にしたかが docs/testing/scan-listing-test-cleanup.md にある
- Shown by: test — tests/contract/config_filters_cli.rs の status の補助、tests/contract/scan_limits.rs の status_lists_files_inside_a_directory_link_within_the_scan_limit、tests/contract/status_judgement.rs の two_symlinks_are_compared_by_their_target_text_without_reading_the_content を手本にし、実行ファイルを試験 SSH サーバに対して起動するテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: 一つのテストで複数の場合を確かめるか分けるか
- Stop and hand back if: 同じ構成の左右で確かめたい項目が "equal" 以外で出る、または root_dir を symlink にした設定が隔離の確認を通らない

### S2: エージェントの経路の根拠テストを実行ファイルで書いて印を付ける

- Purpose: symlink の項目と root_dir の symlink を、エージェントの経路でも一覧に載せることに根拠テストを置く
- Specification: docs/ir/scan/listing.md#REQ-scan-006, docs/ir/scan/listing.md#REQ-scan-007
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の走査用モジュール, tests/common/mod.rs, docs/testing/scan-listing-test-cleanup.md
- Done when: REQ-scan-006 と REQ-scan-007 の `kotowari query` の tests にエージェントの経路の印付きテストがあり、各テストは起動の前にエージェントを有効にした設定の隔離の確認を通し、起動の後に試験サーバのコマンドの記録でエージェントの経路で走査したことを確かめたうえで、Approach and why の場合を status の JSON で確かめる。どのテストを根拠にしたかが docs/testing/scan-listing-test-cleanup.md にある
- Shown by: test — tests/contract/ssh_fallback.rs の an_available_remote_agent_completes_comparison_and_merge の起動の組み方と S1 のテストを手本にし、エージェントを有効にした実行ファイルのテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: none
- Stop and hand back if: エージェントが起動せず SSH の経路に切り替わる、root_dir を symlink にするとエージェントの配置や起動が失敗する、またはエージェントの経路で確かめたい項目が "equal" 以外で出る

### S3: 変異テストを一度回して見逃しを決着させる

- Purpose: 決着の対象の関数の見逃しを一件ずつ決着させ、対象の要件の verification を見直す
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S2
- May change: tests/contract/ の走査用モジュール（テストの追加）, tests/contract.rs, .kotowari/mutants-equivalents.yaml, docs/testing/scan-listing-test-cleanup.md
- Done when: 最後のテストの状態で回した Approach and why の変異テストの集計、見逃し一件ずつの位置と変異の内容、決着の対象かどうか（外したものはどの範囲か）、決着の対象の見逃しの全てへの決着が、実行したコミットとともに docs/testing/scan-listing-test-cleanup.md にある。負荷の下で落ちるテストだけに検知された変異は名前とともに記録されている。REQ-scan-002・006・007 の verification とそれが要件の性質に合う理由が一行ずつある
- Shown by: external — Approach and why の `scripts/mutants.sh --re ...` をバックグラウンドで実行して完了を待ち（結果の読み方は `target/mutants-run/mutants.out/outcomes.json` と `log/`）、その結果を docs/testing/scan-listing-test-cleanup.md に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 見逃しが不具合の疑いを示した、verification の見直しの候補がある、または決着の対象でない見逃しが Approach and why の区分に当てはまらない（文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S3 は完了しない）、実行がメモリ上限で失敗し続ける

### S4: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/scan/listing.md#REQ-scan-006, docs/ir/scan/listing.md#REQ-scan-007, docs/ir/scan/directory-links.md#REQ-scan-002, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S3
- May change: none
- Done when: REQ-scan-006 と REQ-scan-007 の `kotowari query` の tests が空でなく、EX-scan-003 の tests に SSH の経路のテストがあり、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in REQ-scan-006 REQ-scan-007; do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari query EX-scan-003 | jq -r '.items[0].tests[].path'`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
