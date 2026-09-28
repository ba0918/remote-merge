# Plan: 設定のフィルターのテストを要件の根拠に整理する

## Goal

設定のフィルターの取り込みで加えた各要件に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/config/filters.md#REQ-config-021`、`#REQ-config-022`、`#REQ-config-023`、`#REQ-config-024`、`#REQ-config-025`、`#REQ-config-026`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`（いずれも review）

判断の出典は `docs/decision/records/2026-09-29-adopt-config-filters.md`（取り込み、FLAG-config-013 から 015、テストの仕分けの件数）、`docs/decision/records/2026-09-27-test-method-selection.md`、`docs/decision/records/2026-09-28-mutation-scope.md`。要件の本文は `kotowari query REQ-config-0nn` で読む。前の二回の記録 `docs/testing/config-loading-test-cleanup.md`・`docs/testing/config-values-test-cleanup.md` と、既存の `tests/contract/filters.rs`（REQ-config-003・004 の根拠）が手本になる。REQ-config-003・004 は例（EX-config-005 から 008）に印が付いており、対象にしない。新しいテストの印は要件の ID（`@kotowari[REQ-config-0nn]`）で付ける（S4 はそれを見る）。

## Approach and why

取り込みの仕分けでは、この範囲のテスト 52 件のうち実装の中身をなぞるだけのテストは 1 件（`src/config.rs` の `test_include_default_empty`）だった。これまでと同じく元の単体テストは消さず、削除は行わない。そのため最初の変異テストは省き、最後に一度だけ回す。既存のテストは消さず、書き換えない。製品コードは変えない。

REQ-config-021 から 023、025、026 は、`tests/contract/filters.rs` の `fixture`（設定を `load_config_from_paths` で読み、`RuntimeTargets::with_local` でサーバをローカルのディレクトリに差し替える）と同じ組み方で、status を関数呼び出し（`remote_merge::cli::status::execute_status`）で実行し、結果の一覧にどのパスが出るかで確かめる。`--all` を付けるか、左右の中身を変えて、フィルターで外れなかったファイルが一覧に出るようにする。一覧の出し方は `docs/ir/cli/status-output.md` の要件が契約にしている JSON（`remote_merge::service::output::format_json` を通したもの）の "files" の "path" で観測し、REQ-config-026 はその "sensitive" で観測する。設定は `load_config_from_paths` で読んだものを使い、合成と include の整え方を通す。

- REQ-config-021: "/" を含まないパターンが、ファイル名に当たる場合（例 "*.log"）、途中のディレクトリ名に当たる場合（例 "node_modules" で "a/node_modules/b/c.txt"）、どちらにも当たらない同じ階層のファイルが残る場合を確かめる。
- REQ-config-022: "/" を含むパターンが、パス全体に当たる場合（例 "config/*.toml" で "config/a.toml"）と、ファイル名だけが同じで別の場所にある場合（"other/a.toml"）は外れないことと、"vendor/legacy/**" でその下が外れることを確かめる。
- REQ-config-023: include "src" で "src" の下が対象になり、"srcx" の下は対象にならないことと、入れ子の include（例 "a/b"）でその下だけが対象になることを確かめる。include の値はディレクトリに限る。ファイルを include したときの扱いは、ローカルの走査が起点の下だけを返すため何も出ない恐れがあり（実装を読んだ推測で未確認）、この計画では確かめない。
- REQ-config-025: include の対象の中で exclude に当たるものが外れ、当たらないものが残ることを確かめる。
- REQ-config-026: 設定に sensitive を書かないとき、既定の六つのパターンのそれぞれに当たるファイルが "sensitive": true になり、当たらないファイルがならないことと、設定に書いたパターン（既定に当たらないもの、例 "*.secretish" のように "*secret*" にも当たらない名前にする）に当たるファイルも "sensitive": true になることを確かめる。設定に書いたパターンは、グローバル設定だけに書いた場合と、プロジェクト設定（`load_config_from_paths` の第 2 引数）だけに書いた場合の両方で確かめる（合成の経路が別で、手本の fixture はグローバル設定だけを読むため）。例のファイルは名前自体がパターンに当たるものにし（"*" は "/" をまたがないため "secret/a.txt" は当たらない）、サブディレクトリに置いた例も含める。既定のパターンを外そうとする書き方（FLAG-config-014）は使わない。

REQ-config-024 は二つに分けて確かめる。整え方（先頭の "./"・末尾の "/"・空の値）は上と同じ関数呼び出しで、"./src/" を書いた include と、空の値を "src" と一緒に書いた include（`["", "src"]`）が、"src" の外にもファイルがある構成で "src" と同じ対象になることで確かめる（空の値だけの include は使わない。空の値は整えられなければ root_dir 全体を指すため、"src" の外のファイルが出ないことで整え方を見分けられる）。無効な値の警告は、設定の読み込み時に標準エラーに出るため、前の回の `tests/contract/config_values_cli.rs` と同じく実行ファイルを SSH の試験サーバに対して `status --left local --right develop` で起動し、出力（標準出力と標準エラーをつないだもの）に三つの警告の文言が含まれることで確かめる。その起動は公開されている隔離の確認（`TestDirs::assert_isolated_config_at` か、前の回に足した `TestDirs::assert_isolated_values_config`）を通してから行う。`gen_config` はすでに `[filter]` の表と exclude の行を出すため、include は新しい `[filter]` の表を足すのではなく既存の exclude の行を置き換える形で足し、置き換えが起きたことを assert で確かめる（`tests/contract/config_values_cli.rs` の with_password と同じ形。表を重ねると TOML として読めず、末尾に足すと別の表に入って黙って無視される）。無効な値は必ず有効な値（例 "src"）と一緒に書き、絶対パスと ".." の値は試験用の一時ディレクトリの中を指すものにする（変異の下で整え方が壊れても走査が一時ディレクトリの外に向かわないようにするため）。全ての値が無効な include（FLAG-config-013）は使わない。無効な値が無視されたことは関数呼び出しで確かめるが、正規化がなくても走査の側が別の理由で捨てる値（root_dir の外や存在しない値）では見分けられないため、整えられなければ走査されてしまう値を使う: root_dir の中の実在するディレクトリを指す絶対パスと、名前に glob 文字を含む実在するディレクトリ（例 "lib[1]"）を "src" と一緒に書き、それらの下のファイルが出ないことを確かめる。

新しく書くテストは FLAG-config-013 から 015 の挙動を確かめない。具体的には、全て無効な include（FLAG-config-013）、既定の sensitive を外す書き方（FLAG-config-014）、"../" を含む exclude のパターンとその警告（FLAG-config-015）を検証の対象に含めない。存在しない include、root_dir の外を指す include、TUI のツリーの祖先ディレクトリの表示は範囲の外なので確かめない。

変異テストは決着の対象の関数に絞って一度だけ回す: `scripts/mutants.sh --re '(should_exclude|is_path_excluded|normalize_include_paths|is_path_included|is_sensitive|resolve_scan_roots|merge_configs)' src/filter.rs src/service/status.rs src/local/mod.rs src/config.rs`。このうち次の見逃しは範囲として記録だけする: `is_path_excluded` の "../" を含むパターンの扱いは FLAG-config-015、`normalize_include_paths` の全て無効なときの結果は FLAG-config-013、`is_path_included` は関数全体が TUI のツリーの経路で使われ status の走査では `resolve_scan_roots` が同じ役を担うため TUI のツリーの表示（凍結中）の範囲、`should_exclude` はエージェントの走査だけで使われるため scan の話題、`is_path_excluded` の "dir/**" のディレクトリの枝刈り（`path == prefix || glob_match(prefix, path)`）はファイルの一覧に差が出ず TUI のディレクトリの表示でだけ見えるため TUI の範囲、`resolve_scan_roots` の存在しない include と root_dir の外を指す include の扱いは scan の話題、`merge_configs` のフィルターの合成（552 行から 613 行付近）以外の部分は前の回の `docs/testing/config-loading-test-cleanup.md` の記録と比べる。これらの範囲の見逃しは、既存の単体テストで落ちているかどうかも記録する。前の回から引き継いだ `merge_configs` の二件は次のとおり扱う: sensitive の重複除き（前の回の 585:24）は、変異でプロジェクト設定だけに書いた新しいパターンが足されなくなり REQ-config-026 に反するため、REQ-config-026 のプロジェクト設定の場合のテストで落ちることを確かめる。include を整える分岐（前の回の 607:8）は REQ-config-024 の空の値を "src" と一緒に書く関数呼び出しのテストと S2 の警告のテストで落ちることを確かめる。それ以外の見逃しは全て決着の対象にする。cargo-mutants 27.1.0 では構造体のフィールドを消す変異が `--re` で除かれないため、対象の関数の中のものは決着の対象に含め、他の関数のものは記録だけする。負荷の下で落ちるテスト（tui_merge や agent_ssh）だけに検知された変異は、名前とともに記録する。

最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG の範囲としての記録（前の二回の記録と同じ扱い）、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。見逃しを落とすためにテストを足したら、その状態で変異テストを回し直してから記録する。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。変異の一時的な書き換えで確かめるときは本体の作業ツリーで行い、確かめた後に `git diff --stat src/` が空に戻ることを確かめる。作業ツリーやブランチは作らない。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の設定用のモジュール（新規作成。関数呼び出しのものと、警告を確かめる実行ファイルのもの。前の回の補助を共有するためにその可視性や置き場所を変えることは含む。既存のテストの中身は変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/config-filters-test-cleanup.md`（新規。根拠にしたテスト、変異テストの結果と見逃しの決着の記録）

## Step order and prerequisites

S1 は関数呼び出しの根拠テスト、S2 は警告の実行ファイルの根拠テストで、同じ記録の文書を書き換えるため順に行う。S3 で変異テストを一度だけ回して見逃しを決着させる。S4 で全体を確かめる。

S3 には利用者の判断を待つ区切りがある。新しい FLAG の候補、verification の見直しの候補、決着の対象でない見逃しで範囲の区分に当てはまらないものがあれば、`docs/testing/config-filters-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/config-filters`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-config-021, REQ-config-022, REQ-config-023, REQ-config-024, REQ-config-025, REQ-config-026 | — |
| S2 | REQ-config-024 | — |
| S3 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S4 | 上記すべて | — |

## Left to the implementer

- 新しいテストのモジュールの名前と、テストの分け方
- 実行ファイルの起動に既存の補助を共有するか、同じ形の補助を新しいモジュールに置くか

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 要件を確かめるために FLAG-config-001 から 015 の挙動を確かめる必要が生じた
- 要件の verification が要件の性質に合わないと判断した（IR は直さず、S3 で候補として手渡す）
- 要件を確かめるために製品コードの挙動や公開範囲を変える必要が生じた
- 変異テストの見逃しや新しいテストが不具合の疑いを示した（見逃しは未決着のまま報告する）
- 変異テストが WSL のメモリを使い切りそうな兆候（スクリプトの上限に当たって失敗し続ける）があり、並列数を下げても解消しない

## Test command

```sh
cargo nextest run --all-features
```

変異テストは `scripts/mutants.sh` から実行し、cargo-mutants を直接実行しない。並列数を 2 より上げない。

## Out of scope

- FLAG-config-001 から FLAG-config-015 の決着と、その挙動の修正
- SSH とエージェントの走査がローカルと同じ規則で絞り込むこと、存在しない include、root_dir の外を指す include（scan の話題）
- TUI のツリーの表示とフィルターの切り替え
- 既存の単体テストの書き換えと削除
- 面の検査（`surface`）の導入

## Steps

### S1: パターンの当て方・include・既定の sensitive の根拠テストを関数呼び出しで書いて印を付ける

- Purpose: exclude・include・sensitive のパターンがどのパスに当たるかに、status の関数呼び出しを通す根拠テストを置く
- Specification: docs/ir/config/filters.md#REQ-config-021, docs/ir/config/filters.md#REQ-config-022, docs/ir/config/filters.md#REQ-config-023, docs/ir/config/filters.md#REQ-config-024, docs/ir/config/filters.md#REQ-config-025, docs/ir/config/filters.md#REQ-config-026
- Prerequisites: none
- May change: tests/contract.rs, tests/contract/ の設定用モジュール, docs/testing/config-filters-test-cleanup.md
- Done when: REQ-config-021・022・023・025・026 の `kotowari query` の tests が空でなく、REQ-config-024 には整え方と無効な値が無視されることを確かめる印付きのテストがある。各テストは Approach and why の要件ごとの場合を、status の JSON の "files" の "path"（REQ-config-026 は "sensitive"）で確かめる。どのテストを根拠にし、どの既存テストの組み方を手本にしたかが docs/testing/config-filters-test-cleanup.md にある
- Shown by: test — tests/contract/filters.rs の fixture と status の呼び出し、src/filter.rs の is_path_excluded・normalize_include_paths・is_path_included の単体テスト、src/service/status.rs の test_sensitive_* を手本にし、load_config_from_paths と execute_status を通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: 一つのテストで複数の場合を確かめるか分けるか
- Stop and hand back if: status の結果にフィルターで外れたファイルが出る、または include の整え方が設定の読み込みで行われず関数呼び出しで観測できない

### S2: include の無効な値の警告の根拠テストを実行ファイルで書いて印を付ける

- Purpose: 設定の読み込み時に標準エラーに出る include の警告に、実行ファイルを通す根拠テストを置く
- Specification: docs/ir/config/filters.md#REQ-config-024
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の設定用モジュール, docs/testing/config-filters-test-cleanup.md
- Done when: REQ-config-024 の印付きテストに、有効な値と一緒に書いた絶対パス・".." を含む値・glob 文字を含む値のそれぞれについて、出力に "Absolute path is not allowed in include filter: 値"・"Path traversal is not allowed in include filter: 値"・"Glob patterns are not supported in include filter: 値" が含まれることを確かめるものがある。起動は隔離の確認を通してから行う。どのテストを根拠にしたかが docs/testing/config-filters-test-cleanup.md にある
- Shown by: test — tests/contract/config_values_cli.rs の起動の組み方と src/filter.rs の test_normalize_* を手本にし、実行ファイルを通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: none
- Stop and hand back if: 警告が標準エラーにも標準出力にも出ない、または隔離の確認を通る設定では警告を観測できない

### S3: 変異テストを一度回して見逃しを決着させる

- Purpose: 決着の対象の関数の見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S2
- May change: tests/contract/ の設定用モジュール（テストの追加）, tests/contract.rs, .kotowari/mutants-equivalents.yaml, docs/testing/config-filters-test-cleanup.md
- Done when: 最後のテストの状態で回した Approach and why の変異テストの集計、見逃し一件ずつの位置と変異の内容、決着の対象かどうか（外したものはどの範囲か）、決着の対象の見逃しの全てへの決着、前の回から引き継いだ二件の扱いが、実行したコミットとともに docs/testing/config-filters-test-cleanup.md にある。merge_configs のフィルター以外の部分は前の回の記録と比べた結果がある。負荷の下で落ちるテストだけに検知された変異は名前とともに記録されている。REQ-config-021 から 026 の verification とそれが要件の性質に合う理由が一行ずつある
- Shown by: external — `scripts/mutants.sh --re '(should_exclude|is_path_excluded|normalize_include_paths|is_path_included|is_sensitive|resolve_scan_roots|merge_configs)' src/filter.rs src/service/status.rs src/local/mod.rs src/config.rs` をバックグラウンドで実行して完了を待ち（結果の読み方は `target/mutants-run/mutants.out/outcomes.json` と `log/`）、その結果を docs/testing/config-filters-test-cleanup.md に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 見逃しが不具合の疑いを示した、verification の見直しの候補がある、または決着の対象でない見逃しが Approach and why の区分に当てはまらない（文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S3 は完了しない）、実行がメモリ上限で失敗し続ける

### S4: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/config/filters.md#REQ-config-021, docs/ir/config/filters.md#REQ-config-022, docs/ir/config/filters.md#REQ-config-023, docs/ir/config/filters.md#REQ-config-024, docs/ir/config/filters.md#REQ-config-025, docs/ir/config/filters.md#REQ-config-026, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S3
- May change: none
- Done when: REQ-config-021 から REQ-config-026 の全てで `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in $(seq -f 'REQ-config-%03g' 21 26); do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
