# Plan: merge で書き込み先の変更を黙って失う二つの問題を直す

## Goal

--ref を使うファイル全体の merge は書き込み先が参照先から変わったファイルを書かずに失敗として出し、--hunks の番号は diff --format json の hunk と一致するようになり、どちらも印付きのテストで裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/cli/merge.md#REQ-cli-051`（例 EX-cli-063〜066）
- `docs/ir/merge/hunks.md#REQ-merge-032`（例 EX-merge-038・039）、`#REQ-merge-029`、`#REQ-merge-030`、`#REQ-merge-031`、`#REQ-merge-028`（TBL-merge-001）
- `docs/ir/cli/reference.md#REQ-cli-011`（例 EX-cli-021 の構成が変わった）、`docs/ir/merge/basic.md#REQ-merge-018`（前提が変わった）
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-012`、`#REQ-testing-013`（review）

判断の出典は `docs/decision/records/2026-09-28-merge-ref-hunks-fix.md`（A1〜A11）。要件の本文は `kotowari query ID` で読む。用語「競合」は `docs/ir/CONTEXT.md` にある。

## Approach and why

これは製品コードの修正で、テストを先に書く（RED → GREEN → REFACTOR）。先に要件の例を確かめる根拠テストを書いて今の実装で落ちることを示し、それから実装を直す。

一つ目の修正（REQ-cli-051）は `src/cli/merge.rs` の `execute_merge` の、--ref があり --force も --dry-run もないときに `plan.files` を絞る部分を変える。今は `src/service/merge.rs` の `has_three_way_conflict` で競合だけを止めている。新しい規則は、三つとも中身を読めて読み込み元と書き込み先の中身が違う通常ファイルについて、中身のバイト列を比べ、書き込み先が参照先と同じなら書き込み、読み込み元が参照先と同じで書き込み先だけが違えば failed に "destination changed since reference"、両方が参照先と違えば failed に "three-way conflict" を出す。この判定は三つのバイト列を受けて結果を返す純粋関数として（読み込み元と書き込み先のバイト列が同じときは、参照先によらず今と同じく書き込む結果を返す。このファイルは差分がないため merge の対象に入らず、入力として届かない） `src/service/merge.rs` に置き（プロジェクトの設計原則。単体テストと変異テストの対象にしやすい）、`execute_merge` はその結果に従うだけにする。三つのどれかで中身を読めないファイルの今の扱い（"three-way comparison incomplete"、FLAG-cli-023）は変えない。symlink は A9 で今回の対象から外したため、読み込み元か書き込み先のツリーのノードが symlink のファイルには今の判定（`has_three_way_conflict`）をそのまま使い、挙動を変えない。--hunks の経路（`run_hunk_merge`）の競合の判定は A2 のとおり変えない。

二つ目の修正（REQ-merge-032）は `src/service/merge_flow.rs` の `execute_hunk_merge` を変える。今は `DiffResult::Modified` の `merge_hunks`（文脈 0 行の区切り）の数と番号を使っている。これを `hunks`（文脈 3 行の区切り。diff --format json が出すもの。`src/service/diff.rs` の `convert_hunks` に渡している）の数と番号に変え、選んだ表示用の区切りの行の範囲に入る `merge_hunks` を全て適用する。範囲外の判定と hunks_total も表示用の区切りの数で数える。全ての区切りを選んだときに読み込み元の中身をそのまま使う今の近道は、全ての `merge_hunks` を選んだときに当たるよう保つ。表示用の区切りの番号から適用する `merge_hunks` を選ぶ対応は、純粋関数として `src/diff/engine.rs` か `src/service/merge_flow.rs` に置く。TUI の hunk 操作（`apply_hunk_to_text` の経路）は変えない。

仕様の変更で扱いが変わる既存のテストは二件ある。一件目は `tests/contract/cli_results.rs` の `reference_side_is_not_modified_by_a_three_way_merge`（印は EX-cli-021 と EX-cli-034）。これは --force で三つとも違うファイルを書くテストで、A1 の確認を通らないため今後も落ちない。テストの本体は変えず、印から EX-cli-021 だけを外して EX-cli-034（競合を利用者が --force で書くと選ぶ例）の根拠として残し、EX-cli-021 には A7 の構成（書き込み先は参照先と同じで読み込み元だけが変わったファイルを、--ref を指定し --force なしで merge し、右が更新され参照先が変わらない）の新しいテストを新しいモジュールに書く。二件目は `tests/contract/merge_results.rs` の `a_binary_file_changed_on_only_one_side_is_not_a_conflict_and_is_written` で、a.bin（読み込み元だけが変わった）と b.bin（書き込み先だけが変わった）を merge する。A1 の後の期待は、merged がちょうど [a.bin]、failed がちょうど [{path: b.bin, error: "destination changed since reference"}]、終了コード 2、develop の a.bin が読み込み元のバイト列、develop の b.bin は変わらない（\xffrght\n）。期待をこう直し、名前を `a_binary_changed_only_on_the_source_is_written_and_one_changed_only_on_the_destination_fails` のように挙動に合わせる。この二件以外の既存のテストが落ちたら、直さずに作業を返す。

根拠テストは merge の公開された入口を通す。`remote_merge::cli::merge::execute_merge` に `RuntimeTargets::with_local` でサーバをローカルのディレクトリに差し替えて渡す（`tests/contract/merge_support.rs` の `fixture`（develop と staging を持つ）・`args`・`merge_json_and_text`・`merge_error`）。--ref は `args.ref_server = Some("staging".into())` で渡す。EX-merge-038 の diff の hunk の数は `remote_merge::cli::diff::execute_diff` を関数として呼んで確かめる（`tests/contract/cli_results.rs` の `diff_args` の形）。新しいテストは `tests/contract/` の下の新しいモジュール `merge_ref_hunks_fix.rs`（名前は変えてよい）に置く。印は要件と例の両方を書く（例: `// @kotowari[REQ-cli-051, EX-cli-063]`、`// @kotowari[REQ-merge-032, EX-merge-038]`）。S2 の --hunks のテストは、`merge_hunks.rs` と同じく `force: true` にして ref_server を付けない（--hunks が確認を出さないこと（FLAG-merge-016）に頼らないため。`merge_hunks.rs` の `hunk_args` は非公開なので、同じ形の補助関数を `merge_support.rs` に足すか新しいモジュールに書く）。まだない純粋関数の単体テストを先に書くとコンパイルできないため、先に今の挙動を返す仮の実装（または `todo!()`）で関数の形だけを足し、単体テストが落ちることを示してから本体を書く。

新しく書くテストは残っている FLAG の挙動を確かめない。特に、三つの中身がそろわないファイル（FLAG-cli-023）、--dry-run での確認（FLAG-cli-024）、symlink（FLAG-cli-027）、--hunks の確認と書き込み直前の確認（FLAG-merge-016・017）、差分のないファイル（FLAG-merge-019）を構成に含めない。

変異テストは、`docs/decision/records/2026-09-28-mutation-scope.md` のとおり、最後に一度だけ、直した関数に絞って回す。テストを削除しない整理にあたるため最初の実行は省く。

## Scope of change

- `src/service/merge.rs`（参照先に対する判定の純粋関数の追加）
- `src/cli/merge.rs`（`execute_merge` の --ref の判定をその関数に従わせる）
- `src/service/merge_flow.rs`（`execute_hunk_merge` の番号と数の数え方）
- `src/diff/engine.rs` または `src/service/merge_flow.rs`（表示用の区切りから適用する区切りを選ぶ純粋関数の追加）
- 上の各ファイルの `#[cfg(test)]` の単体テスト（追加だけ）
- `tests/contract.rs`（モジュール宣言の追加だけ）、`tests/contract/` の新しいモジュールと `merge_support.rs` への補助関数の追加
- `tests/contract/cli_results.rs` の `reference_side_is_not_modified_by_a_three_way_merge`（印の行から EX-cli-021 を外すことだけ）と `tests/contract/merge_results.rs` の `a_binary_file_changed_on_only_one_side_is_not_a_conflict_and_is_written`（A1 に合わせた期待と名前の修正だけ）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/merge-ref-hunks-fix.md`（新規。根拠テストと変異テストの結果の記録）

## Step order and prerequisites

S1（REQ-cli-051）と S2（REQ-merge-032）は互いに独立した修正だが、同じテストのモジュールを書き換えるため順に行う。どちらもテストを先に書いて落ちることを示してから直す。S3 で変異テストを一度回して見逃しを決着させる。S4 で全体を確かめる。

S3 には利用者の判断を待つ区切りがある。新しい FLAG の候補があれば記録に書いてコミットしたところで作業を返す。

作業ブランチは `fix/merge-ref-hunks`（作業ツリーはこのブランチを取り出した別の作業ツリー）。仕様は同じブランチにコミット済み。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-cli-051, REQ-cli-011, REQ-merge-018 | EX-cli-063, EX-cli-064, EX-cli-065, EX-cli-066, EX-cli-021 |
| S2 | REQ-merge-032, REQ-merge-029, REQ-merge-030, REQ-merge-028 | EX-merge-038, EX-merge-039 |
| S3 | REQ-testing-012（review）, REQ-testing-013（review） | — |
| S4 | 上記すべて | — |

## Left to the implementer

- 二つの純粋関数の名前、置き場所（Scope of change の候補の中で）、戻り値の型
- 新しいテストのモジュールの名前と分け方

## Stop conditions

- 仕様の変更で期待が変わったと Approach and why に書いた二件以外の既存のテストが、修正の後に落ちた
- symlink を今の判定のまま残すことが、ツリーのノードの種類から判別できない
- 表示用の区切りの行の範囲と `merge_hunks` の行の範囲の対応が、区切りの境界で一意に決まらない（一つの `merge_hunks` が二つの表示用の区切りにまたがる）
- 要件を確かめるために残っている FLAG の挙動を確かめる必要が生じた
- 変異テストの見逃しが不具合の疑いを示した、または実行がメモリ上限で失敗し続ける

## Test command

```sh
cargo nextest run --all-features
```

変異テストは `scripts/mutants.sh` から実行し、並列数を 2 より上げない。

## Out of scope

- 残っている FLAG（FLAG-cli-023・024・027、FLAG-merge-016・017・019 など）の決着
- TUI の hunk 操作
- diff --format json の出力の形
- 三者の中身を行単位で混ぜ合わせる三者マージ

## Steps

### S1: 書き込み先が参照先から変わったファイルを書かないようにする

- Purpose: --ref を使うファイル全体の merge で書き込み先の変更を黙って失わないようにする
- Specification: docs/ir/cli/merge.md#REQ-cli-051, docs/ir/cli/reference.md#REQ-cli-011, docs/ir/merge/basic.md#REQ-merge-018
- Prerequisites: none
- May change: src/service/merge.rs, src/cli/merge.rs, tests/contract.rs, tests/contract/ の新しいモジュール, tests/contract/merge_support.rs, tests/contract/cli_results.rs の reference_side_is_not_modified_by_a_three_way_merge の印の行だけ, tests/contract/merge_results.rs の a_binary_file_changed_on_only_one_side_is_not_a_conflict_and_is_written, docs/testing/merge-ref-hunks-fix.md
- Done when: EX-cli-063・064・065・066 と EX-cli-021 の印付きテストがあり（`kotowari query` でそれぞれの tests が空でない）、EX-cli-021 は A7 の構成で右が更新され参照先が変わらないことを確かめ、063（左右が別々の行を変えたテキストのファイル）は書き込み先が変わらず failed の error がちょうど "three-way conflict"、064（読み込み元は参照先と同じで書き込み先だけが変わった）は書き込み先が変わらず error がちょうど "destination changed since reference"、065（書き込み先は参照先と同じで読み込み元だけが変わった）は書き込み先が読み込み元の中身になる、066（063 と同じ構成に --force）は書き込み先が読み込み元の中身になることを確かめる。063 と 064 のテストは修正の前に落ちたことが記録にある。判定の純粋関数に四つの場合（書き込み先が参照先と同じ、読み込み元だけが参照先と同じ、両方が違う、読み込み元と書き込み先が同じ）の単体テストがある。cli_results.rs の reference_side_is_not_modified_by_a_three_way_merge は本体を変えずに印が EX-cli-034 だけになり、merge_results.rs のバイナリのテストが Approach and why の期待に直っている
- Shown by: test — 新しいモジュールの EX-cli-063〜066 のテストと、判定の純粋関数の単体テストを先に書いて RED を示し（063 と 064 が落ちる。065 と 066 は今も通る）、実装を直して GREEN にし、直した二件の既存テストを含めて `cargo nextest run --all-features` が通る
- Left to the implementer: 純粋関数の名前と戻り値の型
- Stop and hand back if: 直した二件以外の既存のテストが落ちた、symlink を今の判定のまま残す分岐がツリーのノードの種類から作れない

### S2: --hunks の番号を diff の hunk で数えるようにする

- Purpose: 利用者が diff の出力で調べた番号のとおりの変更を書き込むようにする
- Specification: docs/ir/merge/hunks.md#REQ-merge-032, docs/ir/merge/hunks.md#REQ-merge-029, docs/ir/merge/hunks.md#REQ-merge-030, docs/ir/merge/hunks.md#REQ-merge-028
- Prerequisites: S1
- May change: src/service/merge_flow.rs, src/diff/engine.rs, tests/contract.rs, tests/contract/ の新しいモジュール, tests/contract/merge_support.rs, docs/testing/merge-ref-hunks-fix.md
- Done when: EX-merge-038 と EX-merge-039 の印付きテストがあり、038 は変わらない行を 2 行だけ挟んだ二つの変更を持つファイルで、execute_diff の JSON の hunks がちょうど一つで、--hunks 0 の merge で二つの変更の両方が書き込み先に入り、JSON の hunks_total が 1 になることを確かめ、039 は変わらない行を 7 行以上挟んだ二つの変更で --hunks 1 の merge が二つ目の変更だけを書き込むことを確かめる。038 のテストは修正の前に落ちたことが記録にある。--hunks 1 が 038 の構成で "Hunk index 1 is out of range (total hunks: 1)" のエラーになることも確かめる（これも修正の前は番号 1 が有効なため落ちる）。表示用の区切りから適用する区切りを選ぶ純粋関数の単体テストがある。既存の merge_hunks.rs のテストと、merge_paths.rs の selected_hunk_changes_only_the_selected_region・selecting_all_hunks_applies_both_regions が全て通る。REQ-merge-032、EX-merge-038、EX-merge-039 の `kotowari query` の tests が空でない
- Shown by: test — EX-merge-038・039 のテストと純粋関数の単体テストを先に書いて RED を示し（038 が落ちる）、execute_hunk_merge を直して GREEN にし、`cargo nextest run --all-features` が通る
- Left to the implementer: 純粋関数の名前と置き場所（src/diff/engine.rs か src/service/merge_flow.rs）
- Stop and hand back if: 一つの操作用の区切りが二つの表示用の区切りにまたがる構成が見つかった（`build_hunks` は文脈を広げると区切りを結合するだけなので、操作用の区切りはちょうど一つの表示用の区切りに含まれるはずで、行の範囲の包含で対応が一意に決まる。これは念のための条件）

### S3: 変異テストを一度回して見逃しを決着させる

- Purpose: 直した関数の見逃しを決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-013
- Prerequisites: S2
- May change: tests/contract/ の新しいモジュール（テストの追加）, 直したファイルの #[cfg(test)] の単体テスト（追加だけ）, .kotowari/mutants-equivalents.yaml, docs/testing/merge-ref-hunks-fix.md
- Done when: S1 と S2 で足した二つの純粋関数、`execute_hunk_merge`、`execute_merge` の --ref の判定の部分に絞った変異テストを最後のテストの状態で回した集計と、見逃し一件ずつの決着（足したテスト、理由付きの同等変異の登録、既存の FLAG の範囲、新しい FLAG の候補のどれか）が、実行したコミットとともに docs/testing/merge-ref-hunks-fix.md にある。`execute_merge` のうち --ref の判定でない部分の見逃しは記録だけし、symlink を今の判定に回す分岐の見逃しは FLAG-cli-027 の範囲として記録する。最初の実行は省いたため、`execute_merge` と `execute_hunk_merge` の見逃しは docs/testing/merge-cli-test-cleanup.md と docs/testing/merge-hunks-test-cleanup.md の最後の記録と比べた結果も書く
- Shown by: external — `scripts/mutants.sh --re '(<S1 の純粋関数の名前>|<S2 の純粋関数の名前>|execute_hunk_merge|execute_merge)' src/service/merge.rs src/cli/merge.rs src/service/merge_flow.rs src/diff/engine.rs` をバックグラウンドで実行して完了を待ち、結果を記録に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 見逃しが不具合の疑いを示した、実行がメモリ上限で失敗し続ける

### S4: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件と例に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/cli/merge.md#REQ-cli-051, docs/ir/merge/hunks.md#REQ-merge-032, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S3
- May change: none
- Done when: `kotowari check` が error を出さず（scenario_without_test と requirement_without_test を含む）、テストと静的検査が通る
- Shown by: check — `kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
