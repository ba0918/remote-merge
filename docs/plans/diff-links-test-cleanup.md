# Plan: CLI diff の symlink とディレクトリのテストを要件の根拠に整理する

## Goal

CLI diff の symlink とディレクトリの要件と例について、印付きの根拠テストが要件を十分に確かめているかが一件ずつ記録され、足りないものは補われ、この範囲の変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/cli/symlink-diff.md#REQ-cli-020`、`#REQ-cli-021`、`#REQ-cli-022`、`#REQ-cli-023`、`#REQ-cli-024`、`#REQ-cli-026` と、`docs/ir/cli/directory-paths.md#REQ-cli-025`、およびそれらを @about に持つ例（EX-cli-039 から 061 のうち二つの文書にあるもの）
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`（いずれも review）

判断の出典は `docs/decision/records/2026-09-30-adopt-diff-links.md`（取り込み、FLAG-cli-043 から 055、テストの仕分けの件数）、`docs/decision/records/2026-09-26-cli-diff-path-semantics.md`（要件の出典）、`docs/decision/records/2026-09-28-mutation-scope.md`、`docs/decision/records/2026-09-29-mutation-rerun-and-load.md`。要件と例の本文と印付きのテストは `kotowari query REQ-cli-020` のように読む。前の回の記録 `docs/testing/diff-output-test-cleanup.md`（とくに「記録だけするもの」の表の「diff の次の回」の行）と `tests/cli_diff.rs` の既存テスト（左はローカル、右は試験 SSH サーバでエージェントは無効）が手本になる。

## Approach and why

この範囲の要件と例は全てそろい、例には全て印付きのテスト（`tests/cli_diff.rs` と `tests/contract/cli_results.rs`）がある。取り込みの仕分けでは実装の中身をなぞるだけのテストはなかったため、テストは消さず、既存のテストの中身も書き換えない。製品コードは変えない。仕事は、(1) 各例と要件について印付きのテストが十分かを見直して記録し、足りないものだけを補うことと、(2) この範囲の変異テストの見逃しを決着させること。

十分かの見直しは、例の Given・When・Then と要件の文の各部分を、印付きのテストが観測しているかで判断する。ただし FLAG-cli-043 から 055 に当たる部分は確かめない（例: note の文言は FLAG-cli-048、機密の symlink の終了コードは FLAG-cli-046、SSH の側だけ参照先がない場合は FLAG-cli-044、ローカルの通常ファイルと SSH のディレクトリ symlink の組は FLAG-cli-045、パスを指定しない diff は FLAG-cli-047、root_dir の外へ出る連鎖の機密の判定は FLAG-cli-051、hunks のリンク文字列は FLAG-cli-052）。取り込みの前の突き合わせで、次の二つは印付きのテストが部分的にしか観測していないと分かっている。

- EX-cli-055: 印付きのテスト `external_directory_nested_secret_stays_hidden_without_force` は、最終参照先が外部のディレクトリの中にあるだけで、外部のディレクトリの中の別のリンクがさらに外部の機密ファイルを指す連鎖を試しておらず、内容のハッシュ（left_hash・right_hash）が出ないことも確かめていない。例の Given（普通のディレクトリリンクの先に別のリンクがあり最終参照先が機密ファイル）と Then（最終参照先の内容とハッシュは表示されない）のとおりに、左右とも root_dir の外のディレクトリを指すディレクトリ symlink "shared" の中に、さらに別の root_dir の外の機密ファイル（名前が機密パターンに当たるもの、例 ".env"）を指すリンクを置き、--follow-external-links を付けて --force なしで diff したとき、JSON のそのリンクの子の項目（"shared/<子>"）に内容の行が出ず、left_hash と right_hash が null であることを確かめるテストを足す。symlink の項目のハッシュは参照先がバイナリのときだけ付くため、機密ファイルの中身は NUL を含むバイナリにし、左右で中身を変える（テキストではハッシュが出ないという確認が隠す処理を壊しても通ってしまう）。外部のディレクトリと機密ファイルは、`tests/cli_diff.rs` の既存の手本と同じく一時ディレクトリの base（`env.local_dir.parent()`）の下の、local と remote のどちらでもない場所に置く（試験 SSH サーバは同じ機械で本物の sh を動かすため、左右が同じ外部のディレクトリを読める）。確かめるのは "shared/<子>" の項目の内容とハッシュだけにし、"shared" の項目そのもの（FLAG-cli-054）と終了コード（FLAG-cli-046）は確かめない。このテストで隠れるのは、入れ子のリンクのリンク文字列の名前（".env"）が機密パターンに当たる経路で、`sensitive_link_chain` が root_dir の外で連鎖を辿らない部分（FLAG-cli-051）は確かめない。したがって S2 で `sensitive_link_chain` の見逃しのうち root_dir の外の連鎖に関わるものは FLAG-cli-051 の範囲として記録だけにする。
- EX-cli-047: 印付きのテストは読めない子が壊れたリンクの場合だけを試す。読めない通常のファイルの子は既存の FLAG-cli-030 の範囲のため、この計画では補わず、十分かの記録にその理由を書く。

それ以外の例と要件で見直しの結果が「足りない」になったものは、FLAG に当たらず要件が定める部分に限り、同じ組み方（`tests/cli_diff.rs` の補助。左はローカル、右は試験 SSH サーバ）でテストを足す。足したテストには見直した例の ID（`@kotowari[EX-cli-0nn]`）の印を付ける。

変異テストは、この範囲の見逃しに絞って一度だけ回す。前の回の `docs/testing/diff-output-test-cleanup.md` の「記録だけするもの」の表で「diff の次の回」とした変異のうち `src/cli/diff.rs` のもの（190:17、198:38、233:21、254:37、255:40 の 2 件、289:17、290:17、304:21、353:21、377:40 の 2 件、394:35、400:21、426:38、440:17、671:17 の 17 件）と `src/service/output.rs:155:12` の 1 件を回す。その後 src/ は変わっていないため位置はそのまま使える（`git log 9c9ff66..HEAD -- src/` は空）。549:30 と 718:48 は前の回の表で FLAG-cli-035 の区分のため回さない。src/service/output.rs:155:12 の行は `if !output.hunks.is_empty()` で "Resolved content differs" を出すかどうかで（前の回の記録の「リンク先の文字列が左右で違うか」という説明は誤り）、決着の対象になるのは内容が実際に違うときに "Resolved content differs" が出る REQ-cli-024 の場合だけで、比べなかったときの出方は FLAG-cli-052、同じ symlink の "Link target:" は FLAG-cli-054 の範囲とする。加えて、前の回に回していない symlink の補助の関数 `sensitive_link_chain`・`path_escapes_root`・`resolved_path_outside_root`・`link_target_for_diff`・`inspected_real_path`・`read_existing_diff_file`（`src/cli/diff.rs`）と `build_symlink_diff_output`（`src/service/diff.rs`）は関数ごと回す。実パスの解決（`src/runtime/target_io.rs` の inspect_path、`src/runtime/remote_io.rs`・`src/runtime/remote_path.rs` の実パスを調べるコマンドとその読み取り）は SSH とエージェントの話題の範囲とし、この計画では回さない。

cargo-mutants 27.1.0 は `--re` を "src/cli/diff.rs:190:17: replace || with && in execute_diff" の形の名前に正規表現として当てるため、名前をそのまま書くと "||" などが正規表現の記号になって全ての変異に一致する。位置で錨を打った次の三つの `--re` を使う: `'^src/cli/diff\.rs:(190:17|198:38|233:21|254:37|255:40|289:17|290:17|304:21|353:21|377:40|394:35|400:21|426:38|440:17|671:17): '`、`'^src/service/output\.rs:155:12: '`、`' in (sensitive_link_chain|path_escapes_root|resolved_path_outside_root|link_target_for_diff|inspected_real_path|read_existing_diff_file|build_symlink_diff_output)$'`。ファイルは `src/cli/diff.rs`・`src/service/diff.rs`・`src/service/output.rs`。回す前に、同じ `--re` とファイルで `cargo mutants --list`（systemd-run のメモリ上限と CPUWeight=idle・Nice=19 の中で実行してよい）を実行し、位置の指定に当たる変異が 18 件であること、補助の関数ごとの件数、それ以外の `execute_diff` などの変異が混ざっていないことを確かめて記録に残す。構造体のフィールドを消す変異は `--re` で除かれずに混ざる（`docs/decision/records/2026-09-28-mutation-scope.md#A3`）ため、件数と見逃しから分けて記録し、対象の関数の外のものは記録だけにする。実行は `scripts/mutants.sh` から行い、既定の並列数（2）より上げない。

変異テストの結果のうち、FLAG-cli-043 から 055 と既存の FLAG に当たる見逃しはその FLAG の範囲として、--ref の参照先の扱いは diff の次の回（--ref と競合）の範囲として、エージェントの経路は SSH とエージェントの話題の範囲として、引き継ぎ先を書いて記録だけにする（`docs/decision/records/2026-09-29-mutation-rerun-and-load.md#A3`）。それ以外の見逃しは全て決着の対象にし、テストの追加、同等変異の登録、新しい FLAG の候補としての報告のどれかで決着させる。見逃しを落とすためにテストを足したら、同じ決定記録の A2 に従い、前の実行で caught にならなかった変異に絞って回し直してから記録する。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。変異の一時的な書き換えで確かめるときは本体の作業ツリーで行い、確かめた後に `git diff --stat src/` が空に戻ることを確かめる。作業ツリーやブランチは作らない。

## Scope of change

- `tests/cli_diff.rs`（テストの追加だけ。既存のテストの中身と期待は変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/diff-links-test-cleanup.md`（新規。例と要件ごとの十分かの見直し、足したテスト、変異テストの結果と見逃しの決着、verification の見直しの記録）

## Step order and prerequisites

S1 で十分かを見直して足りないテストを補う。S2 で変異テストを一度だけ回して見逃しを決着させる。S3 で全体を確かめる。

S2 には利用者の判断を待つ区切りがある。新しい FLAG の候補、verification の見直しの候補、決着の対象でない見逃しで範囲の区分に当てはまらないものがあれば、`docs/testing/diff-links-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/diff-links`。取り込みの記録は同じブランチにコミット済み。main へのマージはこの計画の完了後にする。FLAG-cli-043（symlink を経由する root_dir）の修正はこの計画の後に別に行う。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-cli-020, REQ-cli-021, REQ-cli-022, REQ-cli-023, REQ-cli-024, REQ-cli-025, REQ-cli-026 | EX-cli-055 と、見直しで足りないとした例 |
| S2 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S3 | 上記すべて | — |

## Left to the implementer

- 十分かの見直しの表の形
- 足すテストの名前と、一つのテストで複数の場合を確かめるか分けるか

## Stop conditions

- テストを書く途中で、実装が IR の要件や例と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 要件や例を確かめるために FLAG-cli-043 から 055 や既存の FLAG の挙動を確かめる必要が生じた
- 要件の verification が要件の性質に合わないと判断した（IR は直さず、S2 で候補として手渡す）
- 要件を確かめるために製品コードの挙動や公開範囲を変える必要が生じた
- 変異テストの見逃しや新しいテストが不具合の疑いを示した（見逃しは未決着のまま報告する）
- 変異テストが利用者の PC を重くする兆候（CPU の張り付き、メモリ上限での失敗）があった（止めて報告する）

## Test command

```sh
cargo nextest run --all-features
```

cargo のテストとビルドは systemd-run のメモリ上限（MemoryMax=40%、MemorySwapMax=0）と CPUWeight=idle・Nice=19 の中で実行する。変異テストは `scripts/mutants.sh` から実行し、cargo-mutants を直接実行しない（例外は Approach and why の `cargo mutants --list` だけ）。

## Out of scope

- FLAG-cli-043 から FLAG-cli-055 の決着と修正（FLAG-cli-043 の修正はこの計画の後）
- --ref の参照先と競合（diff の次の回）、SSH とエージェントの経路の話題、TUI の差分表示
- 既存のテストの書き換えと削除、どこからも使われていない "src/diff/symlink.rs"
- 面の検査（`surface`）の導入

## Steps

### S1: 例と要件ごとに根拠テストが十分かを見直し、足りないものを補う

- Purpose: 各例と要件の印付きのテストが、FLAG に当たらない部分を十分に観測しているかを記録し、足りないものを補う
- Specification: docs/ir/cli/symlink-diff.md#REQ-cli-020, docs/ir/cli/symlink-diff.md#REQ-cli-021, docs/ir/cli/symlink-diff.md#REQ-cli-022, docs/ir/cli/symlink-diff.md#REQ-cli-023, docs/ir/cli/symlink-diff.md#REQ-cli-024, docs/ir/cli/directory-paths.md#REQ-cli-025, docs/ir/cli/symlink-diff.md#REQ-cli-026
- Prerequisites: none
- May change: tests/cli_diff.rs, docs/testing/diff-links-test-cleanup.md
- Done when: docs/testing/diff-links-test-cleanup.md に、対象の要件と例の一件ずつについて、印付きのテストの名前、十分か（十分・補った・FLAG の範囲として補わない）とその理由がある。EX-cli-055 に、Approach and why の連鎖の場合で内容の行もハッシュも出ないことを確かめる印付きのテストがあり、ほかに足りないとしたものには FLAG に当たらない部分を確かめる印付きのテストがある
- Shown by: test — tests/cli_diff.rs の external_directory_nested_secret_stays_hidden_without_force・nested_link_to_sensitive_file_does_not_show_resolved_contents と同じ組み方でテストを足して印を付け、`cargo nextest run --all-features --test cli_diff` で通ることと、足したテストが狙いの挙動を壊す変異を一時的に書き入れると落ちることを確かめる（書き入れた変異は戻す）
- Left to the implementer: 見直しの表の形、足すテストの名前と分け方
- Stop and hand back if: 足りない部分を確かめるには FLAG の挙動を確かめる必要がある、または実装が例と食い違う

### S2: この範囲の変異テストを一度回して見逃しを決着させる

- Purpose: この範囲の見逃しを一件ずつ決着させ、対象の要件の verification を見直す
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S1
- May change: tests/cli_diff.rs（テストの追加）, .kotowari/mutants-equivalents.yaml, docs/testing/diff-links-test-cleanup.md
- Done when: 実行前の `cargo mutants --list` の件数と前の回の表の位置が現れたことの確認、最後のテストの状態で回した変異テストの集計、見逃し一件ずつの位置と変異の内容、決着（外したものは範囲と引き継ぎ先）が、実行したコミットとともに docs/testing/diff-links-test-cleanup.md にある。REQ-cli-020 から 026 の verification とそれが要件の性質に合う理由が一行ずつある
- Shown by: external — Approach and why の絞った `scripts/mutants.sh --re ...` をバックグラウンドで実行して完了を待ち（結果の読み方は `target/mutants-run/mutants.out/outcomes.json` と `log/`）、その結果を docs/testing/diff-links-test-cleanup.md に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 見逃しが不具合の疑いを示した、verification の見直しの候補がある、決着の対象でない見逃しが Approach and why の区分に当てはまらない（文書に書いてコミットして作業を返す）、PC を重くする兆候がある

### S3: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件と例に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/cli/symlink-diff.md#REQ-cli-020, docs/ir/cli/symlink-diff.md#REQ-cli-021, docs/ir/cli/symlink-diff.md#REQ-cli-022, docs/ir/cli/symlink-diff.md#REQ-cli-023, docs/ir/cli/symlink-diff.md#REQ-cli-024, docs/ir/cli/directory-paths.md#REQ-cli-025, docs/ir/cli/symlink-diff.md#REQ-cli-026, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S2
- May change: none
- Done when: EX-cli-055 の `kotowari query` の tests に足したテストがあり、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `kotowari query EX-cli-055 | jq -r '.items[0].tests[].name'`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
