# Plan: merge の変更のまとまりを選ぶマージのテストを要件の根拠に整理する

## Goal

merge の変更のまとまりを選ぶマージの取り込みで加えた各要件に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、残った変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/merge/hunks.md#REQ-merge-028`（`- definition:` から決定表 TBL-merge-001 を辿れる）、`#REQ-merge-029`、`#REQ-merge-030`、`#REQ-merge-031`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`、`#REQ-testing-013`（いずれも review）

判断の出典は `docs/decision/records/2026-09-28-adopt-merge-hunks.md`（取り込み、FLAG-merge-015 から 020、テストの仕分けの件数）、`docs/decision/records/2026-09-27-test-method-selection.md`（手法の選び方）、`docs/decision/records/2026-09-28-mutation-scope.md`（変異テストを決着の対象の関数に絞ること、テストを削除しない整理で最初の実行を省くこと、`scripts/mutants.sh --re`）。要件の本文は `kotowari query REQ-merge-0nn` で読む。同じ手順を merge の symlink と削除で行った記録 `docs/testing/merge-links-test-cleanup.md` が記録の書き方と根拠テストの組み方の手本になる。

## Approach and why

取り込みの仕分けでは、この範囲のテスト 40 件に実装詳細をなぞるだけのテストはなかった。そのためこの計画は削除を行わず、根拠テストのない新しい要件 4 件に根拠テストを足し、最後に一度だけ変異テストを回して見逃しを決着させる。REQ-testing-012 の改訂（`docs/decision/records/2026-09-28-mutation-scope.md` の A2）により、テストを削除しない整理では最初の変異テストを省き、最後の実行の見逃しは、そのファイルを前に回したときの記録（`docs/testing/merge-cli-test-cleanup.md`、`docs/testing/merge-write-test-cleanup.md`、`docs/testing/merge-links-test-cleanup.md` の整理後の節）と比べる。既存のテストは消さず、書き換えない。製品コードは変えない。

変異テストは決着の対象の関数に絞って一度だけ回す: `scripts/mutants.sh --re '(validate_merge_args|run_hunk_merge|validate_hunk_merge_target|execute_hunk_merge|apply_selected_hunks_single_pass)' src/cli/merge.rs src/service/merge_flow.rs src/diff/engine.rs`。cargo-mutants 27.1.0 では構造体のフィールドを消す変異が `--re` で除かれないため、結果にはそれらが混ざる。そのうち `execute_hunk_merge` の書き込みと --dry-run の経路（DiffResult::Modified）のもの（前の回で見逃しとして記録した MergeFileResult のフィールドを消す変異）は決着の対象に含め、差分のないファイルの経路（DiffResult::Equal の "skipped (no changes)"）のものは差分のないファイルでしか落とせないため FLAG-merge-019 の範囲として記録し、他の関数のものは記録だけする。`src/diff/engine.rs` と `run_hunk_merge` はこれまでの回で変異テストを回していないため、その見逃しは比べる相手なしに新しい見逃しとして決着させる。`validate_merge_args` の --hunks でない分岐は merge の指定・確認・出力の整理で決着済みのため、そこに入った見逃しは記録だけする。FLAG-merge-015 から 020 の挙動（diff の JSON との番号、確認と衝突、書き込み直前の確認、--max-entries、差分のないファイルのテキスト、手引きの機密ファイル）に関わる見逃しはその FLAG の範囲として記録する。

根拠テストは merge の公開された入口を通す。関数呼び出しで `remote_merge::cli::merge::execute_merge` に `RuntimeTargets::with_local` でサーバをローカルのディレクトリに差し替えて渡す（`tests/contract/merge_support.rs` の `fixture`・`fixture_with_backup`・`merge_json_and_text`・`args`）。`args` の `hunks` に番号を入れて使う。書き込むテストは `force: true` にする（`args` の既定は `force: false` で、CLI の --hunks が確認なしに書き込むのは FLAG-merge-016 の挙動のため、それに頼らない。書き込むファイルは機密ファイルでなく --ref もないので、--force で他の挙動は変わらない）。`merge_support.rs` の `merge_json_and_text` は `execute_merge` の失敗を unwrap するため、エラーの行と REQ-merge-031 には `execute_merge` のエラーを返す補助関数を `merge_support.rs` に足す（`config()` と `runtime_targets()` は非公開のため、同じファイルの中に置く）。テキストは `execute_merge` の結果を `remote_merge::service::output::format_merge_text` に渡して確かめる（振り分けは REQ-cli-049 の実行ファイルのテストが確かめている）。二つの hunk を作る構成では、二つの変更の間に変わらない行を 7 行以上置く（`tests/contract/merge_paths.rs` の selected_hunk_changes_only_the_selected_region の、間に 12 行を置く構成を写してよい。`--hunks` の番号は文脈 0 行で変更ごとに分けた区切りを数え、離しておけば表示用の区切りとも数が合い、FLAG-merge-015 の違いに触れない）。

REQ-merge-028 の終了コード 2 は、接続より前に止まる行（パスが一つでない、--delete と併せる）だけを `merge_support.rs` の `run_cli`（書き込み先を差し替えない隔離した実行ファイルの呼び出し）で確かめる。パスが一つでない行は二つのパスを渡す（パスがないと引数の解析が先に止める）: `run_cli(&["a.txt", "b.txt", "--hunks", "0", "--left", "local", "--right", "develop"])` で標準エラーに "--hunks requires exactly one path (got 2)" が含まれる。--delete の行は一つのパスに `--delete` を足す。--format json は付けない（JSON ではエラーが標準出力に出るため）。symlink の行とバイナリの行は、読み込み元だけ・書き込み先だけの二通りを作り、バイナリの少なくとも一つは NUL ではなく UTF-8 として正しくないバイト列にする（読み込み元か書き込み先かの判定の "||" を変えた変異を落とせるようにするため）。接続の後に止まる行（番号が範囲外、symlink、バイナリ、機密ファイル）は `execute_merge` がエラーを返すことと、その文言と、書き込み先が変わらないことで確かめる（エラーを終了コード 2 にするのは `src/main.rs` の共通の経路で、REQ-cli-046 の実行ファイルのテストが確かめている）。機密ファイルの行は、`merge_support.rs` の `fixture_with_sensitive` で設定に書いたパターン（既定のパターンに当たらないもの）を使う。

新しく書くテストは FLAG-merge-015 から 020 と FLAG-cli-016 から 026 の挙動を確かめない。具体的には、diff の JSON の番号との対応（FLAG-merge-015）、確認のプロンプトの有無（FLAG-merge-016）、書き込み直前の確認（FLAG-merge-017）、--max-entries（FLAG-merge-018）、差分のないファイルの出力（FLAG-merge-019）、機密ファイルのスキップ（FLAG-merge-020）、リモート間の拒否（FLAG-cli-020）を検証の対象に含めない。

最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。見逃しを落とすためにテストを足したら、その状態で変異テストを回し直してから記録する。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。変異の一時的な書き換えで確かめるときは本体の作業ツリーで行い、確かめた後に `git diff --stat src/` が空に戻ることを確かめる。作業ツリーやブランチは作らない。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の merge 用のモジュール（`merge_hunks.rs` の新規作成、`merge_support.rs` への補助関数の追加）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/merge-hunks-test-cleanup.md`（新規。根拠にしたテスト、変異テストの結果と見逃しの決着の記録）

## Step order and prerequisites

S1 と S2 は要件ごとの根拠テストの追加で、同じモジュールを書き換えるため順に行う。S3 で変異テストを一度だけ回して見逃しを決着させる。S4 で全体を確かめる。

S3 には利用者の判断を待つ区切りがある。新しい FLAG の候補や verification の見直しの候補があれば、`docs/testing/merge-hunks-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/merge-hunks`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-merge-028, REQ-merge-031 | — |
| S2 | REQ-merge-029, REQ-merge-030 | — |
| S3 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review）, REQ-testing-013（review） | — |
| S4 | 上記すべて | — |

## Left to the implementer

- 新しいテストのモジュールの名前と、テストの分け方
- 補助関数を `merge_support.rs` に足すか、テストのモジュールに置くか

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 要件を確かめるために FLAG-merge-015 から 020 または FLAG-cli-016 から 026 の挙動を確かめる必要が生じた
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

- FLAG-merge-015 から FLAG-merge-020 の決着と、その挙動の修正
- REQ-merge-009・010 の印付きテストの見直し
- TUI の hunk 操作とそのテスト
- テストの削除
- 面の検査（`surface`）の導入

## Steps

### S1: --hunks の指定のエラーと三者の競合の根拠テストを書いて印を付ける

- Purpose: --hunks の指定の誤りと、参照先に対する競合で止まることに、merge の入口を通す根拠テストを置く
- Specification: docs/ir/merge/hunks.md#REQ-merge-028, docs/ir/merge/hunks.md#REQ-merge-031
- Prerequisites: none
- May change: tests/contract.rs, tests/contract/ の merge 用モジュール, docs/testing/merge-hunks-test-cleanup.md
- Done when: REQ-merge-028 と REQ-merge-031 の `kotowari query` の tests が空でない。REQ-merge-028 の印付きテストは TBL-merge-001 の六行を確かめる: パスが一つでない行と --delete と併せる行は `run_cli` で終了コード 2 と標準エラーに表の文言が含まれること、番号が範囲外・symlink・バイナリ・機密ファイル（設定に書いたパターン）の行は `execute_merge` のエラーの文言が表の文言と一致し（N・M・パスは構成から決まる値）書き込み先が変わらないこと。REQ-merge-031 の印付きテストは --ref があり --force のない --hunks の merge で、同じ箇所を左右が別々に変えたファイルが書き込まれず、エラーの文言が "three-way conflict: パス" で、--dry-run を付けても同じエラーで止まることを確かめる。どのテストを根拠にし、どの既存テストの組み方を手本にしたかが docs/testing/merge-hunks-test-cleanup.md にある
- Shown by: test — src/cli/merge.rs の test_hunks_with_multiple_paths_returns_error・test_hunks_with_delete_returns_error、src/service/merge_flow.rs の validate_hunk_merge_target の単体テスト、tests/contract/cli_results.rs の selected_hunk_with_three_way_conflict_does_not_overwrite_the_target を手本にし、execute_merge と run_cli を通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: 一つのテストで決定表の複数の行を確かめるか分けるか
- Stop and hand back if: パスが一つでない行か --delete と併せる行が、接続より前に止まらず run_cli で観測できない

### S2: --hunks の JSON とテキストの根拠テストを書いて印を付ける

- Purpose: --hunks の merge の結果の出し方に、merge の入口を通す根拠テストを置く
- Specification: docs/ir/merge/hunks.md#REQ-merge-029, docs/ir/merge/hunks.md#REQ-merge-030
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の merge 用モジュール, docs/testing/merge-hunks-test-cleanup.md
- Done when: REQ-merge-029 と REQ-merge-030 の `kotowari query` の tests が空でない。構成は、7 行以上離れた二つの変更を持つテキストのファイル一つで、--hunks に片方の番号だけを渡す。REQ-merge-029 の印付きテストは JSON の merged の一件が、書き込んだとき status "merged"、--dry-run では status "would merge"、hunks_applied が渡した番号、hunks_total が 2、direction が "left_to_right" で、backup が --dry-run でない書き込みのうちバックアップの有効な構成（fixture_with_backup）でだけ出て、無効な構成（fixture）では出ないことを確かめ（--dry-run では backup を確かめない）、書き込み先には選んだ変更だけが入ることを確かめる。REQ-merge-030 の印付きテストはテキストの行が、書き込んだときバックアップの無効な構成でちょうど "Merged: パス (hunks: 番号/2)"、有効な構成でその後に " (backup: …)" が続き、--dry-run でちょうど "Would merge: パス (hunks: 番号/2)" になることを確かめる。差分のないファイルは構成に含めない（FLAG-merge-019）。どのテストを根拠にしたかが docs/testing/merge-hunks-test-cleanup.md にある
- Shown by: test — tests/contract/merge_paths.rs の selected_hunk_changes_only_the_selected_region、src/service/output.rs の test_format_merge_text_hunk_merge_info を手本にし、execute_merge と format_json・format_merge_text を通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: none
- Stop and hand back if: 7 行以上離した二つの変更が二つの hunk にならない

### S3: 変異テストを一度回して見逃しを決着させる

- Purpose: 決着の対象の関数の見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010, docs/ir/testing/methods.md#REQ-testing-013
- Prerequisites: S2
- May change: tests/contract/ の merge 用モジュール（テストの追加）, tests/contract.rs, .kotowari/mutants-equivalents.yaml, docs/testing/merge-hunks-test-cleanup.md
- Done when: 最後のテストの状態で回した Approach and why の変異テストの集計、見逃し一件ずつの位置と変異の内容、決着の対象かどうか、決着の対象の見逃しの全てへの決着（足したテスト、同等変異の理由付きの登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれか）が、実行したコミットとともに docs/testing/merge-hunks-test-cleanup.md にある。前の回の記録にある同じ関数の見逃し（execute_hunk_merge の MergeFileResult のフィールドを消す変異など）との比べた結果も書かれている。負荷の下で落ちるテスト（tui_merge や agent_ssh）だけに検知された変異は名前とともに記録されている。REQ-merge-028 から 031 の verification とそれが要件の性質に合う理由が一行ずつあり、合わないと判断したものは IR を直さず候補として挙がっている
- Shown by: external — `scripts/mutants.sh --re '(validate_merge_args|run_hunk_merge|validate_hunk_merge_target|execute_hunk_merge|apply_selected_hunks_single_pass)' src/cli/merge.rs src/service/merge_flow.rs src/diff/engine.rs` をバックグラウンドで実行して完了を待ち（結果の読み方は `target/mutants-run/mutants.out/outcomes.json` と `log/`）、その結果を docs/testing/merge-hunks-test-cleanup.md に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 見逃しが不具合の疑いを示した、または verification の見直しの候補がある（文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S3 は完了しない）、実行がメモリ上限で失敗し続ける

### S4: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/merge/hunks.md#REQ-merge-028, docs/ir/merge/hunks.md#REQ-merge-029, docs/ir/merge/hunks.md#REQ-merge-030, docs/ir/merge/hunks.md#REQ-merge-031, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S3
- May change: none
- Done when: REQ-merge-028 から REQ-merge-031 の全てで `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in $(seq -f 'REQ-merge-%03g' 28 31); do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
