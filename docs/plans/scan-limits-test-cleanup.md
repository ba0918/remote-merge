# Plan: 走査の上限と不完全な一覧のテストを要件の根拠に整理する

## Goal

走査の上限と不完全な一覧の取り込みで加えた要件と判定表に要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、既存の上限の要件がリモートの経路でも確かめられ、変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/scan/limits.md#REQ-scan-008`、`docs/ir/scan/limits.md#REQ-scan-009`（判定表 TBL-scan-001 を含む）
- 既存の `docs/ir/scan/limits.md#REQ-scan-004` の例 EX-scan-008 と EX-scan-009（SSH とエージェントの経路の根拠を足すことだけ。要件の文は変えない）
- verification の見直しの対象: `docs/ir/scan/directory-links.md#REQ-scan-002`、`docs/ir/scan/limits.md#REQ-scan-003`、`#REQ-scan-004`、`#REQ-scan-005`、`#REQ-scan-008`、`#REQ-scan-009`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`（いずれも review）

判断の出典は `docs/decision/records/2026-09-29-adopt-scan-limits.md`（取り込み、FLAG-scan-010 から 014、テストの仕分けの件数）、`docs/decision/records/2026-09-27-test-method-selection.md`、`docs/decision/records/2026-09-28-mutation-scope.md`。要件の本文は `kotowari query REQ-scan-009` のように読む。前の回の記録 `docs/testing/scan-listing-test-cleanup.md` と、そのテスト `tests/contract/scan_listing_cli.rs`（試験 SSH サーバとエージェントを使う実行ファイルのテストの組み方）、既存の `tests/contract/scan_limits.rs`（関数呼び出しで status と sync を実行する組み方）が手本になる。新しいテストの印は要件の ID（`@kotowari[REQ-scan-009]`）で付け、EX-scan-008・009 のリモートの経路のテストは例の ID で付ける。

## Approach and why

取り込みの仕分けでは、この範囲のテスト 63 件に実装の中身をなぞるだけのテストはなかった。そのため削除は行わず、最初の変異テストは省いて最後に一度だけ回す。既存のテストは消さず、書き換えない。製品コードは変えない。

REQ-scan-008 と REQ-scan-009 は走査の上限の超過で観測する。上限の超過のエラーは三つの経路の走査が共通に出すもので、どの範囲を走査したかは、上限を超える範囲を走査したときだけエラーになることで見分けられる。そのため、`tests/contract/scan_limits.rs` の `scan_fixture` と同じく設定を `load_config_from_paths` で読み、`RuntimeTargets::with_local` でサーバをローカルの一時ディレクトリに差し替え、`remote_merge::cli::status::execute_status`・`remote_merge::cli::diff::execute_diff`・`remote_merge::cli::merge::execute_merge`・`remote_merge::cli::sync::execute_sync` を関数呼び出しで実行する。merge と sync は必ず --dry-run（`dry_run: true`）で実行し、書き込みを起こさない。

件数の数え方（FLAG-scan-010）とちょうど上限の件数（FLAG-scan-011）に触れないよう、上限と件数には十分な差を取る。左右の root_dir に、ファイル 10 件を置く "big/"、ファイル 3 件ずつを置く "small/" と "small2/"、root_dir の直下のファイル "top.txt" を置き、左右で中身を変えて差分があるようにし、上限を 5 にする。全体の走査は 5 を大きく超え、"small/" だけ、"small2/" だけの走査は 5 を大きく下回り、"small/" と "small2/" を合わせると 5 を超える。ディレクトリを数えるかどうかで結果が変わらない件数にする。

- REQ-scan-008: 上限 5 で、status と diff はパスなし、merge と sync はパス "."（merge・sync はパスを一つ以上必須とし、パスなしは走査の前に別のエラーで止まるため）で実行し、どれもエラーになり、そのエラーの文が "Tree scan truncated at 5 entries." で始まり、"--max-entries"、"max_scan_entries"、"specify file paths" を含むことを確かめる。
- REQ-scan-009 の status と diff の部分: status と、パスなしの diff は上限の超過のエラーになる（REQ-scan-008 の場合と共有してよい）。diff はパスの末尾の "/" を範囲を選ぶ前に取り除き、パスを指定した diff の範囲と上限は cli diff の話題（REQ-cli-021 など）のため、パスを指定した diff は確かめない。
- REQ-scan-009 の merge・sync の部分（TBL-scan-001 の行ごと。merge と sync のそれぞれで確かめる）: 上限 5 のまま次を確かめる。
  - 全体を走査する行: "."・"./"・空の値 ""、glob 文字を含むパス（例 "small/*.txt"）、21 個のパス（"small/a.txt" など "small/" の下の実在するファイルのパスを重ねて 21 個にする。判定は重複を除かずに数を数えるため。同じパスで 20 個のときはエラーにならないことも確かめ、エラーになるかどうかが個数だけで決まることを示す）、末尾が "/" のものとそうでないものの混在（"small/" と "small/a.txt"）、--delete を付けて "small/" を指定、のそれぞれが上限の超過のエラーになる。
  - 全て末尾が "/" の行: "small/" を指定するとエラーにならず "small/" の下のファイルだけを扱い、"small/" と "small2/" を指定しても（合わせると 5 を超えるが）エラーにならない（上限をディレクトリごとの走査に当てる）。"small/" と "small2/" は最上位のディレクトリが違うため FLAG-scan-014 に触れない。
  - 全て末尾が "/" でない行: "small/a.txt" を指定するとエラーにならず、"top.txt" を指定すると（root_dir の直下のファイルを含むため）上限の超過のエラーになる。
  - 「エラーにならない」の確かめ方: merge は関数が Ok を返し、merged に期待したパスが揃う。sync は右の書き込み先の走査の失敗を関数のエラーにせず結果の中の失敗として返すため、Ok であることに加え、書き込み先の失敗（接続の失敗を含む）がなく、終了コードが 0 で、merged に期待したパスが揃うことを確かめる。「エラーになる」の確かめ方は、merge と sync の左（読み込み元）の走査が上限を超えて関数がエラーを返し、その文が "Tree scan truncated" を含むことにする。
- EX-scan-008・009（SSH とエージェントの経路）: 前の回の `tests/contract/scan_listing_cli.rs` と同じ組み方で実行ファイルを試験 SSH サーバに対して `status --left local --right <サーバ> --all --format json --max-entries <上限>` で起動し、右の root_dir に上限を大きく超えるファイル（例 上限 3 でファイル 10 件）を置いたときは上限の超過のエラーになり、上限を大きく下回るファイル（例 上限 50 でファイル 3 件）のときは全てのファイルが "equal" で出ることを確かめる。起動の前後の確認（`--config` と一時ディレクトリの `current_dir`、env_clear、隔離の確認、エージェントの経路の確認）は前の回のテストと同じにし、共有できる補助は共有してよい。上限の超過の場合は、左の local の root_dir を上限を大きく下回る構成（例 ファイル 1 件）にし、右だけを上限を大きく超える構成にする（上限の超過のエラーの文は三つの経路で同じで、どちらの側かを示さないため、左の走査が先にエラーになると右の経路を通らずに通ってしまう）。上限の内の場合だけ左右を同じ構成にする。上限の超過のときは標準出力が JSON にならないため、終了コードが 0 以外であることと、標準出力と標準エラーをつないだものに "Tree scan truncated" が含まれることで確かめる。新しい SSH とエージェントのモジュールは、`tests/contract.rs` で `#[cfg(feature = "test-utils")]` の下に宣言し、ファイルの先頭に `#![cfg(unix)]` を置く。前の回の起動の補助に `--max-entries` などの引数を渡せるようにする変更は、共有のための変更に含める。

新しく書くテストは FLAG-scan-010 から 014 の挙動を確かめない。具体的には、件数の数え方の違い、ちょうど上限の件数、find のタイムアウト、読めないディレクトリ、同じ最上位のディレクトリの下を複数指定した sync を検証の対象に含めない。存在しないディレクトリを指定した sync（sync の話題）、merge の --hunks と --max-entries（FLAG-merge-018）、diff のファイルのパスの扱いとディレクトリ symlink の上限（REQ-cli-021）、TUI の走査も確かめない。

変異テストは決着の対象の関数に絞って一度だけ回す: `scripts/mutants.sh --re '(check_truncation|resolve_max_entries|resolve_scan_strategy|fast_path_to_parent_dirs|has_root_parent_dir|is_root_marker|has_glob_chars|fetch_tree_by_strategy|fetch_partial_tree|fetch_trees_and_statuses_for_merge|fetch_partial_trees|run_diff_full_scan|fetch_tree_recursive|fetch_tree_for_subpath|walk_single_root|scan_local_tree_recursive_with_include|list_tree_recursive|list_tree\b|handle_list_tree|ScanIterator.*::next)' src/runtime/side_io.rs src/runtime/target_io.rs src/runtime/core.rs src/config.rs src/service/fast_path.rs src/cli/sync.rs src/cli/merge.rs src/cli/diff.rs src/local/mod.rs src/ssh/client.rs src/agent/client.rs src/agent/dispatch.rs src/agent/tree_scan.rs`。`fetch_tree_recursive|fetch_tree_for_subpath` は `src/runtime/side_io.rs`・`src/runtime/target_io.rs`（ローカルとリモートの実装。with_local で差し替えたサーバもこれを通る）の同名の関数と、`src/runtime/core.rs`・`src/runtime/side_io.rs` の `fetch_remote_tree_*`・`try_agent_fetch_tree_*` に当たる。`list_tree\b` は `src/agent/client.rs` の list_tree（打ち切りの印を受け取る）に当てる。`ScanIterator.*::next` はエージェントの走査の打ち切り（`src/agent/tree_scan.rs` の Iterator の next）に当てる。`run_diff_partial_scan` は diff が末尾の "/" を範囲を選ぶ前に取り除くため到達できず、対象に含めない（到達できないことは記録に書く）。回す前に、同じ `--re` とファイルで `cargo mutants --list`（テストを走らせない一覧の表示。`scripts/mutants.sh` と同じく systemd-run のメモリ上限の中で実行してよい）を実行し、関数ごとの変異の件数を記録に残して、上の関数が全て変異の名前に現れることを確かめる（現れない関数があれば名前を実装に合わせて直し、直したことを記録する）。

このうち次の見逃しは範囲として記録だけし、引き継ぎ先（FLAG の ID か話題の名前）を書く: FLAG-scan-010 から 014 に当たるもの、パスを指定した diff の範囲と上限（cli diff の話題、REQ-cli-021 など）、exclude と include の当て方（REQ-config-003・004・021 から 025、FLAG-scan-003 から 007 の範囲）、symlink の載せ方と root_dir の symlink（`docs/testing/scan-listing-test-cleanup.md` の範囲）、サイズ・更新時刻・パーミッションの値（REQ-cli-027 の範囲）、`fail_on_truncation` が偽のときの扱い（TUI だけが使う、TUI の範囲）、diff のファイルのパスの扱い（cli diff の範囲）、存在しないパスや ".."・絶対パスの扱い（FLAG-merge-011 と sync の話題）。これらの範囲の見逃しは、既存の単体テストで落ちているかどうかも記録する。それ以外の見逃し（上限の超過の検出と報告、エラーの文の案内、走査の範囲の選び方と親ディレクトリの求め方、ディレクトリごとの上限、不完全な一覧で merge と sync が書き込まないこと、ローカルと SSH の経路の循環の報告）は全て決着の対象にする。cargo-mutants 27.1.0 では構造体のフィールドを消す変異が `--re` で除かれないため、対象の関数の中のものはそのフィールドの値の区分に従い、他の関数のものは記録だけする。負荷の下で落ちるテスト（tui_merge や agent_ssh）だけに検知された変異は、変異の下でそのテストだけを回し直して確かめ、名前とともに記録する。

最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。見逃しを落とすためにテストを足したら、その状態で変異テストを回し直してから記録する。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。変異の一時的な書き換えで確かめるときは本体の作業ツリーで行い、確かめた後に `git diff --stat src/` が空に戻ることを確かめる。作業ツリーやブランチは作らない。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の走査の上限用のモジュール（新規作成。関数呼び出しのものと、SSH とエージェントの経路の実行ファイルのもの。前の回の `tests/contract/scan_listing_cli.rs` の起動の補助を共有するためにその可視性や置き場所を変えることは含む。既存のテストの中身と期待は変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/scan-limits-test-cleanup.md`（新規。根拠にしたテスト、変異テストの結果と見逃しの決着、verification の見直しの記録）

## Step order and prerequisites

S1 は関数呼び出しで REQ-scan-008 と REQ-scan-009 の根拠テスト、S2 は SSH とエージェントの経路の EX-scan-008・009 の根拠テストで、同じ記録の文書を書き換えるため順に行う。S3 で変異テストを一度だけ回して見逃しを決着させる。S4 で全体を確かめる。

S3 には利用者の判断を待つ区切りがある。新しい FLAG の候補、verification の見直しの候補、決着の対象でない見逃しで範囲の区分に当てはまらないものがあれば、`docs/testing/scan-limits-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/scan-limits`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-scan-008, REQ-scan-009（TBL-scan-001） | — |
| S2 | REQ-scan-004（SSH とエージェントの経路） | EX-scan-008, EX-scan-009 |
| S3 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S4 | 上記すべて | — |

## Left to the implementer

- 新しいテストのモジュールの名前と、テストの分け方（判定表の行ごとに分けるか、表を回す一つのテストにするか）
- 前の回の起動の補助を共有するか、同じ形の補助を新しいモジュールに置くか

## Stop conditions

- テストを書く途中で、実装が IR の要件や判定表と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 要件を確かめるために FLAG-scan-010 から 014 の挙動を確かめる必要が生じた
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

- FLAG-scan-010 から FLAG-scan-014 の決着と、その挙動の修正
- 存在しないディレクトリを指定した sync、merge の --hunks と --max-entries（FLAG-merge-018）、diff のファイルのパスとディレクトリ symlink の上限（REQ-cli-021）
- TUI の走査と表示
- 既存の単体テストの書き換えと削除
- 面の検査（`surface`）の導入

## Steps

### S1: 上限の超過の案内と走査の範囲の根拠テストを関数呼び出しで書いて印を付ける

- Purpose: 上限の超過のエラーの案内と、指定したパスによる走査の範囲の選び方に、status・diff・merge・sync の関数呼び出しを通す根拠テストを置く
- Specification: docs/ir/scan/limits.md#REQ-scan-008, docs/ir/scan/limits.md#REQ-scan-009
- Prerequisites: none
- May change: tests/contract.rs, tests/contract/ の走査の上限用モジュール, docs/testing/scan-limits-test-cleanup.md
- Done when: REQ-scan-008 と REQ-scan-009 の `kotowari query` の tests が空でなく、各テストは Approach and why の構成と場合を関数呼び出しの結果（エラーの文、または差分・merged の一覧）で確かめ、merge と sync は --dry-run で実行している。TBL-scan-001 の行ごとにどのテストが確かめるかと、どの既存テストの組み方を手本にしたかが docs/testing/scan-limits-test-cleanup.md にある
- Shown by: test — tests/contract/scan_limits.rs の scan_fixture と status・sync の呼び出し、src/service/fast_path.rs の resolve_scan_strategy と fast_path_to_parent_dirs の単体テストを手本にし、load_config_from_paths と execute_status・execute_diff・execute_merge・execute_sync を通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: 判定表の行ごとに分けるか、表を回す一つのテストにするか
- Stop and hand back if: 判定表の行のとおりにエラーになる・ならないが分かれない、または merge・sync を --dry-run で実行しても書き込みが起きる

### S2: 上限の超過と上限の内の一覧をリモートの経路でも確かめるテストを書いて印を付ける

- Purpose: EX-scan-008・009 を SSH とエージェントの経路でも確かめる根拠テストを置く
- Specification: docs/ir/scan/limits.md#REQ-scan-004
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の走査の上限用モジュール, tests/contract/scan_listing_cli.rs（起動の補助を共有するための可視性や置き場所の変更だけ。テストの中身と期待は変えない）, docs/testing/scan-limits-test-cleanup.md
- Done when: EX-scan-008 と EX-scan-009 の `kotowari query` の tests に SSH の経路とエージェントの経路のテストがあり、各テストは起動の前に隔離の確認を通し、エージェントの経路では起動の後に試験サーバのコマンドの記録でエージェントの経路で走査したことを確かめ、上限を大きく超える右の root_dir で上限の超過のエラーになることと、上限を大きく下回る右の root_dir で全てのファイルが "equal" で出ることを確かめる。エラーが右の経路で起きたことの見分け方と、どのテストを根拠にしたかが docs/testing/scan-limits-test-cleanup.md にある
- Shown by: test — tests/contract/scan_listing_cli.rs の起動の組み方と tests/contract/scan_limits.rs の status_reports_a_scan_limit_instead_of_returning_a_partial_file_list・status_uses_the_explicit_limit_and_lists_every_file_when_it_fits を手本にし、実行ファイルを試験 SSH サーバに対して起動するテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: none
- Stop and hand back if: エージェントが起動せず SSH の経路に切り替わる、上限を大きく超えても右の経路でエラーにならない、または上限を大きく下回っても打ち切られる

### S3: 変異テストを一度回して見逃しを決着させる

- Purpose: 決着の対象の関数の見逃しを一件ずつ決着させ、対象の要件の verification を見直す
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S2
- May change: tests/contract/ の走査の上限用モジュール（テストの追加）, tests/contract.rs, .kotowari/mutants-equivalents.yaml, docs/testing/scan-limits-test-cleanup.md
- Done when: 実行前の `cargo mutants --list` の関数ごとの件数、最後のテストの状態で回した Approach and why の変異テストの集計、見逃し一件ずつの位置と変異の内容、決着の対象かどうか（外したものはどの範囲で、引き継ぎ先はどこか）、決着の対象の見逃しの全てへの決着が、実行したコミットとともに docs/testing/scan-limits-test-cleanup.md にある。負荷の下で落ちるテストだけに検知された変異は名前とともに記録されている。REQ-scan-002・003・004・005・008・009 の verification とそれが要件の性質に合う理由が一行ずつある
- Shown by: external — Approach and why の `scripts/mutants.sh --re ...` をバックグラウンドで実行して完了を待ち（結果の読み方は `target/mutants-run/mutants.out/outcomes.json` と `log/`）、その結果を docs/testing/scan-limits-test-cleanup.md に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 見逃しが不具合の疑いを示した、verification の見直しの候補がある、または決着の対象でない見逃しが Approach and why の区分に当てはまらない（文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S3 は完了しない）、実行がメモリ上限で失敗し続ける

### S4: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件と例に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/scan/limits.md#REQ-scan-008, docs/ir/scan/limits.md#REQ-scan-009, docs/ir/scan/limits.md#REQ-scan-004, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S3
- May change: none
- Done when: REQ-scan-008 と REQ-scan-009 の `kotowari query` の tests が空でなく、EX-scan-008 と EX-scan-009 の tests に SSH の経路とエージェントの経路のテストがそれぞれ一件以上あり、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in REQ-scan-008 REQ-scan-009; do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`for id in EX-scan-008 EX-scan-009; do for route in ssh agent; do kotowari query $id | jq -e --arg r "$route" '[.items[0].tests[].name | select(test($r))] | length > 0' > /dev/null || echo "missing $id $route"; done; done`（SSH の経路のテストの名前に "ssh"、エージェントの経路のテストの名前に "agent" を含める）、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
