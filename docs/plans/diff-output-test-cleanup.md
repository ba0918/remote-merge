# Plan: CLI diff の差分の出し方のテストを要件の根拠に整理する

## Goal

CLI diff の差分の出し方の取り込みで加えた要件と、テストのなかった既存要件に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/cli/diff-output.md#REQ-cli-052`、`#REQ-cli-053`、`#REQ-cli-054`、`#REQ-cli-056`、`#REQ-cli-057`、`#REQ-cli-058`、`#REQ-cli-059`、`#REQ-cli-060`
- `docs/ir/cli/binary.md#REQ-cli-061`
- テストのない既存要件 `docs/ir/cli/diff-json.md#REQ-cli-001`
- verification の見直しの対象: 上記に加え `docs/ir/cli/diff-output.md#REQ-cli-055`、`docs/ir/cli/safety.md#REQ-cli-005`、`docs/ir/cli/binary.md#REQ-cli-009`、`docs/ir/cli/results.md#REQ-cli-018`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`（いずれも review）

判断の出典は `docs/decision/records/2026-09-29-adopt-diff-output.md`（取り込み、FLAG-cli-028 から 039、テストの仕分けの件数）、`docs/decision/records/2026-09-29-diff-max-files.md`（REQ-cli-055、実装とテストは済み）、`docs/decision/records/2026-09-27-test-method-selection.md`、`docs/decision/records/2026-09-28-mutation-scope.md`、`docs/decision/records/2026-09-29-mutation-rerun-and-load.md`。要件の本文は `kotowari query REQ-cli-053` のように読む。手本は `tests/contract/diff_max_files.rs`（左右をローカルの一時ディレクトリにして `remote_merge::cli::diff::execute_diff` を呼び、`format_multi_diff_text` でテキストを作る組み方）、`tests/contract/cli_results.rs` の既存の diff のテスト（EX-cli-001・002・052。execute_diff を通し、"folder/" と "folder" の結果の一致まで確かめている。書き換えず、同じ場合を新しいテストに REQ の印で書く）と `docs/testing/scan-limits-test-cleanup.md`（記録の形）。

## Approach and why

取り込みの仕分けでは、この範囲のテスト 129 件のうち実装の中身をなぞるだけのテストは 6 件（`src/service/diff.rs` の使われていない `diff_exit_code` のテスト）だった。これまでの回と同じく元の単体テストは消さないため、削除は行わず、最初の変異テストは省いて最後に一度だけ回す。既存のテストは消さず、書き換えない。製品コードは変えない。

新しいテストは `tests/contract/` の新しいモジュールに置き、`tests/contract/diff_max_files.rs` と同じく設定を `remote_merge::config::load_config_from_paths` で読み、`RuntimeTargets::with_local` でサーバをローカルの一時ディレクトリに差し替え、`execute_diff` を関数呼び出しで実行する。JSON は `remote_merge::service::output::format_json` を通した文字列を `serde_json` で読み、テキストは `format_multi_diff_text` の文字列で確かめ、終了コードは `execute_diff` が返す値で確かめる。見つからないパスの警告（標準エラー）と JSON のエラー（標準出力、main.rs が出す）は関数呼び出しでは観測できないため、`TestDirs::assert_isolated_config_at`（tests/common/mod.rs）の隔離の確認を通したうえで実行ファイルを試験 SSH サーバに対して起動して確かめる（`--config` に一時ディレクトリの設定を渡し、`current_dir` を一時ディレクトリの下にし、env_clear のうえ HOME・XDG を一時ディレクトリに向ける）。`tests/contract/scan_listing_cli.rs` の起動の補助は status に固定されているため使わず、diff 用の起動の関数を新しいモジュールに同じ形で書く。関数呼び出しのモジュールと実行ファイルのモジュールは分け、後者は `tests/contract.rs` で `#[cfg(feature = "test-utils")]` の下に宣言し、ファイルの先頭に `#![cfg(unix)]` を置く。

終了コード 1、"N file(s) with changes out of M total" の数、summary.files_with_changes は、バイナリと隠した機密ファイルも数える（FLAG-cli-034）ため、変更のあるテキストファイルだけの構成で確かめ、バイナリと機密ファイルの場合ではこれらを確かめない。

要件ごとの場合（いずれも左右に中身の違うファイルを置いて差分を作る）:

- REQ-cli-052: パスなしで root_dir 全体の変更のあるファイルが、ファイルのパスを二つ指定するとその二つだけが、ディレクトリのパスを "d/" と "d" の二通りで指定すると同じ配下のファイルだけが出る。
- REQ-cli-001: ディレクトリのパスを指定した JSON が files に配下の複数のファイルの項目を持つ。
- REQ-cli-053: JSON の files の項目が path・left と right（label と root）・sensitive・truncated・hunks（index・left_start・right_start・lines の type と content）を持ち、type が "context"・"added"・"removed" のそれぞれになる場合があること、打ち切らないときは上位のオブジェクトに truncated と changed_files_total のキーがないこと（ファイルごとの truncated は常に出るため、JSON の文字列の中の "truncated" の有無では確かめない）、バイナリのときだけ binary・left_hash・right_hash が出ること、機密ファイルのときだけ note が出ることを確かめる。errors は比べられないパスがあるときの出方が diff の次の回（symlink と root_dir の外）の範囲のため、出ないことだけを確かめる。left_start・right_start の値は確かめない（FLAG-cli-032）。
- REQ-cli-054: 変更の行が 5 行あるファイルで --max-lines 3 のとき追加・削除の行が 3 行で止まり truncated が true、テキストに "... (output truncated)" が出る。文脈の行を数えないこと（変更の前の文脈の行が出ても数に入らない）、0 と指定なしで全ての変更の行が出ることも確かめる。
- REQ-cli-056: 差分なしで 0、差分ありで 1 を返す。2 は見つからないパスの場合（REQ-cli-057 と共有してよい）で確かめ、比べられないファイルによる 2 と通常の出力は diff の次の回の範囲のため確かめない。
- REQ-cli-057: 一つが見つからないとき警告を出して残りを比べ、全て見つからないとき "specified path(s) not found on either side" のエラーで終了コード 2（テキストでは標準エラー、JSON では標準出力の {"error": ...}）。見つからないパスの扱いは、パスを 1〜20 個指定する経路（run_diff_fast_path）と、glob 文字を含むパスや 21 個以上のパスの経路（compute_statuses_and_resolve）の二か所にあるため、後者も、glob 文字を含むパスで全て見つからないときに execute_diff がこの文言のエラーを返すことを関数呼び出しで確かめる。
- REQ-cli-058: テキストが "--- a/パス (左のラベル)"・"+++ b/パス (右のラベル)"・"@@" で始まる行・" "・"-"・"+" の接頭辞の行と、最後の "N file(s) with changes out of M total" を持つ。"@@" の行の数字は確かめない（FLAG-cli-032）。
- REQ-cli-059: --force のない機密ファイル（既定の ".env" など）で sensitive が true、hunks が空、note が "Content hidden (sensitive file). Use --force to show."、テキストに中身が出ない。バイナリの機密ファイルで left_hash・right_hash が出ない。変更のある機密ファイルだけを使う（FLAG-cli-037）。
- REQ-cli-060: 片側にだけある中身のあるテキストファイルで全ての行が削除（左だけ）または追加（右だけ）として出る。0 バイトのファイルは使わない（FLAG-cli-038）。
- REQ-cli-061: NUL を含むファイルと、先頭 8,192 バイトに不正な UTF-8 を含むファイルのそれぞれがバイナリになり、8,192 バイトより後にだけ NUL を含むファイルはテキストとして扱われる。テキストに "Binary files differ (left: sha256=…, right: sha256=…)" の形で SHA-256 のハッシュが出て、JSON に binary と left_hash・right_hash が出る。ハッシュの期待値は製品の関数を使わずに作る（既知の入力の固定のハッシュ値を書くか、sha2 crate を直接使う）。片側にだけあるバイナリのない側と、片側で読めない（パーミッション 000。root で動くときは飛ばす）バイナリの読めない側が "missing" になる。中身の同じバイナリを指定する場合は使わない（FLAG-cli-035）。

新しく書くテストは FLAG-cli-028 から 039 の挙動を確かめない。具体的には、旧総合仕様の JSON の例の形、ハッシュを計算する場所、片側で読めないファイル、exclude に当たるファイルのパスの指定、行番号の値、include の外のディレクトリ、files_with_changes がバイナリ・symlink・機密を数えること、中身の同じバイナリ、ディレクトリの配下の変更のない機密ファイル、片側にだけある 0 バイトのファイル、100MB を超えるファイルを検証の対象に含めない。symlink・ディレクトリ symlink・root_dir の外（diff の次の回）と --ref・競合（その次の回）も確かめない。

変異テストは決着の対象の関数に絞って一度だけ回す: `MUTANTS_JOBS=1 scripts/mutants.sh --re '(execute_diff|run_diff_fast_path|run_diff_full_scan|compute_statuses_and_resolve|read_file_bytes_tolerant|build_diff_output|convert_hunks|build_masked_diff_output|format_diff_text|format_binary_hashes|format_multi_diff_text|has_changes|limit_changed_files|omitted_changed_files|changes_without_reading|ChangedFileBudget|is_binary|compute_diff|build_hunks|make_hunk|compute_sha256)' src/cli/diff.rs src/service/diff.rs src/service/output.rs src/service/types.rs src/service/max_files.rs src/diff/engine.rs src/diff/binary.rs`。--max-files の修正（REQ-cli-055）では変異テストを回していないため、`src/service/max_files.rs` の関数と `ChangedFileBudget` のメソッドもこの回の決着の対象にし、見逃しを落とすための REQ-cli-055 のテストの追加を認める。`compute_diff`・`build_hunks`・`make_hunk` は差分の行（REQ-cli-058・060）を作る関数で、merge のハンクと共有するが、この回の決着の対象にする。回す前に、`cargo mutants --list --all-features --file src/cli/diff.rs --file src/service/diff.rs --file src/service/output.rs --file src/service/types.rs --file src/service/max_files.rs --file src/diff/engine.rs --file src/diff/binary.rs --re '<上と同じ正規表現>'`（systemd-run のメモリ上限と低い CPU 優先度の中で実行してよい）を実行し、関数ごとの変異の件数を記録に残して、上の関数が全て変異の名前に現れることを確かめる（現れない関数があれば名前を実装に合わせて直し、直したことを記録する）。

このうち次の見逃しは範囲として記録だけし、引き継ぎ先（FLAG の ID か話題の名前）を書く（`docs/decision/records/2026-09-29-mutation-rerun-and-load.md` の A3）: symlink・ディレクトリ symlink・root_dir の外の扱い（diff の次の回）、--ref・ref_hunks・競合（その次の回）、FLAG-cli-028 から 039 に当たるもの、状態の判定（REQ-cli-027 の範囲）、TUI だけが使う値。これらの見逃しは既存の単体テストで落ちているかどうかも記録する。それ以外の見逃しは全て決着の対象にする。cargo-mutants 27.1.0 では構造体のフィールドを消す変異が `--re` で除かれないため、対象の関数の中のものはそのフィールドの値の区分に従い、他の関数のものは記録だけする。負荷の下で落ちるテスト（tui_merge や agent_ssh）だけに検知された変異は、変異の下でそのテストだけを回し直して確かめ、名前とともに記録する。

最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。見逃しを落とすためにテストを足したら、見逃しのあった関数だけに絞って回し直す（上の決定記録の A2）。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。変異の一時的な書き換えで確かめるときは本体の作業ツリーで行い、確かめた後に `git diff --stat src/` が空に戻ることを確かめる。作業ツリーやブランチは作らない。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の diff の出し方用のモジュール（新規作成。関数呼び出しのものと、実行ファイルのもの。既存の補助を共有するためにその可視性を変えることは含む。既存のテストの中身と期待は変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/diff-output-test-cleanup.md`（新規。根拠にしたテスト、変異テストの結果と見逃しの決着、verification の見直しの記録）

## Step order and prerequisites

S1 は関数呼び出しの根拠テスト、S2 は実行ファイルの根拠テストで、同じ記録の文書を書き換えるため順に行う。S3 で変異テストを一度だけ回して見逃しを決着させる。S4 で全体を確かめる。

S3 には利用者の判断を待つ区切りがある。新しい FLAG の候補、verification の見直しの候補、決着の対象でない見逃しで範囲の区分に当てはまらないものがあれば、`docs/testing/diff-output-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/diff`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。コミットは普通の CPU の優先度で行う（`nice` を付けると pre-commit のうち固定の待ち時間に頼るテストが他の負荷の下で落ちる）。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-cli-001, REQ-cli-052, REQ-cli-053, REQ-cli-054, REQ-cli-056, REQ-cli-057（glob 文字を含むパスの経路）, REQ-cli-058, REQ-cli-059, REQ-cli-060, REQ-cli-061 | — |
| S2 | REQ-cli-057, REQ-cli-056 | — |
| S3 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S4 | 上記すべて | — |

## Left to the implementer

- 新しいテストのモジュールの名前と、テストの分け方
- `tests/contract/diff_max_files.rs` の fixture の組み方を共有するか、同じ形の補助を新しいモジュールに置くか

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 要件を確かめるために FLAG-cli-028 から 039 の挙動を確かめる必要が生じた
- 要件の verification が要件の性質に合わないと判断した（IR は直さず、S3 で候補として手渡す）
- 要件を確かめるために製品コードの挙動や公開範囲を変える必要が生じた
- 変異テストの見逃しや新しいテストが不具合の疑いを示した（見逃しは未決着のまま報告する）
- 変異テストが WSL のメモリを使い切りそうな兆候、または利用者の PC の他の作業を止めるほどの負荷があり、並列数を下げても解消しない

## Test command

```sh
cargo nextest run --all-features
```

cargo は systemd-run のメモリ上限と低い CPU 優先度の中で実行する。変異テストは `scripts/mutants.sh` から実行し、並列数は `MUTANTS_JOBS=1` とし、cargo-mutants を直接実行しない（例外は Approach and why の `cargo mutants --list` だけ）。変異テストの実行中は他の cargo のコマンドを走らせない。

## Out of scope

- FLAG-cli-028 から FLAG-cli-039 の決着と、その挙動の修正
- symlink・ディレクトリ symlink・root_dir の外（diff の次の回）、--ref と競合（その次の回）、TUI の差分表示
- REQ-cli-055 の実装（済み。テストは S3 で見逃しを落とすための追加だけを認める）
- 既存の単体テストの書き換えと削除
- 面の検査（`surface`）の導入

## Steps

### S1: 差分の出し方の根拠テストを関数呼び出しで書いて印を付ける

- Purpose: パスの指定、JSON とテキストの形、--max-lines、終了コード、機密ファイル、片側にだけあるファイル、バイナリに、execute_diff の関数呼び出しを通す根拠テストを置く
- Specification: docs/ir/cli/diff-json.md#REQ-cli-001, docs/ir/cli/diff-output.md#REQ-cli-052, docs/ir/cli/diff-output.md#REQ-cli-053, docs/ir/cli/diff-output.md#REQ-cli-054, docs/ir/cli/diff-output.md#REQ-cli-056, docs/ir/cli/diff-output.md#REQ-cli-057, docs/ir/cli/diff-output.md#REQ-cli-058, docs/ir/cli/diff-output.md#REQ-cli-059, docs/ir/cli/diff-output.md#REQ-cli-060, docs/ir/cli/binary.md#REQ-cli-061
- Prerequisites: none
- May change: tests/contract.rs, tests/contract/ の diff の出し方用モジュール, docs/testing/diff-output-test-cleanup.md
- Done when: S1 の Specification の各要件の `kotowari query` の tests が空でなく、各テストは Approach and why の要件ごとの場合を関数呼び出しの結果（JSON、テキスト、終了コード）で確かめる。どのテストを根拠にし、どの既存テストの組み方を手本にしたかが docs/testing/diff-output-test-cleanup.md にある
- Shown by: test — tests/contract/diff_max_files.rs の組み方、tests/cli_diff.rs と tests/cli_diff_general.rs の該当するテスト、src/service/diff.rs と src/service/output.rs の単体テストを手本にし、execute_diff を通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: 一つのテストで複数の場合を確かめるか分けるか
- Stop and hand back if: 関数呼び出しの結果が要件と食い違う、または要件の場合を FLAG の挙動に触れずに作れない

### S2: 見つからないパスの警告とエラーの根拠テストを実行ファイルで書いて印を付ける

- Purpose: 標準エラーの警告と JSON のエラーの出し方に、実行ファイルを通す根拠テストを置く
- Specification: docs/ir/cli/diff-output.md#REQ-cli-057, docs/ir/cli/diff-output.md#REQ-cli-056
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の diff の出し方用モジュール, docs/testing/diff-output-test-cleanup.md
- Done when: REQ-cli-057 の `kotowari query` の tests が空でなく、各テストは起動の前に隔離の確認を通し、一つが見つからないときの警告と残りの比較、全て見つからないときのテキストの標準エラーと JSON の標準出力のエラーと終了コード 2 を確かめる。どのテストを根拠にしたかが docs/testing/diff-output-test-cleanup.md にある
- Shown by: test — tests/contract/scan_listing_cli.rs の起動の組み方（補助そのものは status に固定されているため使わない）と tests/cli_diff_general.rs の test_diff_nonexistent_file を手本にし、実行ファイルを試験 SSH サーバに対して起動するテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: none
- Stop and hand back if: 警告やエラーが要件の文言と食い違う、または隔離の確認を通る設定では観測できない

### S3: 変異テストを一度回して見逃しを決着させる

- Purpose: 決着の対象の関数の見逃しを一件ずつ決着させ、対象の要件の verification を見直す
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S2
- May change: tests/contract/ の diff の出し方用モジュール（テストの追加）, tests/contract/diff_max_files.rs（REQ-cli-055 の見逃しを落とすテストの追加だけ。既存のテストの中身と期待は変えない）, tests/contract.rs, .kotowari/mutants-equivalents.yaml, docs/testing/diff-output-test-cleanup.md
- Done when: 実行前の `cargo mutants --list` の関数ごとの件数、最後のテストの状態に対する変異テストの集計（テストを足した後の回し直しは見逃しのあった関数に絞ったもの）、見逃し一件ずつの位置と変異の内容、決着の対象かどうか（外したものはどの範囲で、引き継ぎ先はどこか）、決着の対象の見逃しの全てへの決着が、実行したコミットとともに docs/testing/diff-output-test-cleanup.md にある。負荷の下で落ちるテストだけに検知された変異は名前とともに記録されている。REQ-cli-001・005・009・018・052 から 061 の verification とそれが要件の性質に合う理由が一行ずつある
- Shown by: external — Approach and why の `MUTANTS_JOBS=1 scripts/mutants.sh --re ...` をバックグラウンドで実行して完了を待ち（結果の読み方は `target/mutants-run/mutants.out/outcomes.json` と `log/`）、その結果を docs/testing/diff-output-test-cleanup.md に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 見逃しが不具合の疑いを示した、verification の見直しの候補がある、または決着の対象でない見逃しが Approach and why の区分に当てはまらない（文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S3 は完了しない）、実行がメモリ上限で失敗し続ける

### S4: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/cli/diff-json.md#REQ-cli-001, docs/ir/cli/diff-output.md#REQ-cli-052, docs/ir/cli/diff-output.md#REQ-cli-053, docs/ir/cli/diff-output.md#REQ-cli-054, docs/ir/cli/diff-output.md#REQ-cli-056, docs/ir/cli/diff-output.md#REQ-cli-057, docs/ir/cli/diff-output.md#REQ-cli-058, docs/ir/cli/diff-output.md#REQ-cli-059, docs/ir/cli/diff-output.md#REQ-cli-060, docs/ir/cli/binary.md#REQ-cli-061, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S3
- May change: none
- Done when: S4 の Specification の REQ-cli の全てで `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in REQ-cli-001 REQ-cli-052 REQ-cli-053 REQ-cli-054 REQ-cli-056 REQ-cli-057 REQ-cli-058 REQ-cli-059 REQ-cli-060 REQ-cli-061; do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
