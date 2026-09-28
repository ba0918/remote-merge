# Plan: merge の書き込みの中身のテストを要件の根拠に整理する

## Goal

merge の書き込みの中身の取り込みで加えた各要件に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、その過程と残った変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/merge/read-failure.md#REQ-merge-019`
- `docs/ir/merge/comparison.md#REQ-merge-020`
- `docs/ir/merge/basic.md#REQ-merge-021`、`#REQ-merge-022`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`、`#REQ-testing-013`（いずれも review）

判断の出典は `docs/decision/records/2026-09-28-adopt-merge-write.md`（取り込み、FLAG-merge-001 から 006、テストの仕分けの件数と範囲外にしたもの）と `docs/decision/records/2026-09-27-test-method-selection.md`（手法の選び方、変異テストとメモリ上限）。要件の本文は `kotowari query REQ-merge-0nn` で読む。同じ手順を merge の指定・確認・出力で行った記録 `docs/testing/merge-cli-test-cleanup.md` が記録の書き方と根拠テストの組み方の手本になる。

## Approach and why

取り込みの仕分けでは、この範囲の既存テスト 26 件は全て既存要件の根拠で、実装詳細をなぞるだけのテストはなかった。そのためこの計画は削除を行わず、根拠テストのない新しい要件 4 件に根拠テストを足し、変異テストで見逃しを決着させることに絞る。既存要件（REQ-merge-005・006・011〜014・017・018、REQ-cli-010）の印付きテストの本体は見直さない。

最初に整理前の変異テストの見逃しを記録する。対象は書き込みの中身の規則を持つ `src/service/merge_flow.rs`、`src/cli/tolerant_io.rs`、`src/service/status.rs` の三つにする。`src/service/status.rs` は status の規則が大半を占めるが、REQ-merge-019 の error を組み立てる `verified_content_pairs`、REQ-merge-020 と REQ-merge-005・006 の比べ方を決める `needs_explicit_file_compare`・`needs_merge_content_compare`・`refine_status_with_content` を持ち、status の整理（`docs/testing/status-test-cleanup.md`）はこれらの見逃しを「merge の比較対象」として記録だけにして merge の回に残したため、対象に含める。`src/cli/merge.rs` の比較から対象の絞り込みまでの区間は merge の指定・確認・出力の整理で変異テストを回し、その区間に見逃しがなかったため対象にしない。

見逃しのうち決着の対象にするのは、`src/service/merge_flow.rs` の `execute_single_merge` の通常ファイルの経路（`MergeAction::Normal` の後の、読み込み・書き込み直前の確認・バックアップの呼び出し・書き込み・権限の設定）と `copy_permissions`、`src/cli/tolerant_io.rs` の `fetch_contents_required`、`src/service/status.rs` の `verified_content_pairs`・`needs_explicit_file_compare`・`needs_merge_content_compare`・`refine_status_with_content` のものに限る。`src/service/status.rs` の他の関数（`needs_content_compare` を含む status の規則）の見逃しは、REQ-merge-020 の根拠テストで落ちた場合を除き記録だけする。`src/cli/tolerant_io.rs` の `fetch_contents_tolerant` は merge では --ref の印の計算にだけ使われ（他の呼び出し元は status と diff）、この計画の要件では確かめられないため、その見逃しは記録だけする。`fetch_contents_required` のうち、まとめて読む処理が失敗して一件ずつ読み直す経路は、ローカルのまとめ読みが一件でも読めないと全体で失敗するため、ローカルの差し替えでは片方の経路しか通らない。その経路の違いでしか落ちない見逃しは、SSH の fixture を使わずに記録だけする。`execute_single_merge` の symlink の分岐（`SkipDifferentKind`・`CreateSymlink`・`ReplaceSymlinkWithFile`）、`check_source_exists`、`execute_deletions`、`validate_hunk_merge_target`、`execute_hunk_merge` の見逃しは merge の後の回（symlink と削除、変更のまとまりを選ぶマージ）で扱うため記録だけする。`copy_permissions` の見逃しのうち、読み込み元がリモートの場合（FLAG-merge-002）、新規ファイルに読み込み元の権限を付けること（FLAG-merge-003）、権限の変更の失敗（FLAG-merge-004）に関わるものはその FLAG の範囲として記録する。`fetch_contents_required` のエラーの文言が "read failed:" で始まらない経路（FLAG-merge-005）、書き込み直前の確認で失敗したファイルの扱い（FLAG-merge-006）に関わる見逃しも、その FLAG の範囲として記録する。

根拠テストは merge と sync の公開された入口を通す。関数呼び出しで `remote_merge::cli::merge::execute_merge` と `remote_merge::cli::sync::execute_sync` に `RuntimeTargets::with_local` でサーバをローカルのディレクトリに差し替えて渡す（`tests/contract/merge_support.rs` の `fixture` と `args`、`tests/contract/cli_results.rs` の `sync_fixture` の形）。sync の関数呼び出しは `force: true` か `dry_run: true` を渡す（`force: false` で書き込む予定があると `execute_sync` がテストのプロセスの標準入力を読むため）。REQ-merge-019 と REQ-merge-020 は merge と sync の両方を主語にするため、両方の入口で確かめる。REQ-merge-020 の sync のテストは `force: true` を渡す（dry-run は何も書かないため「書かない」ことの根拠にならない）。`dry_run: true` は REQ-merge-019 の sync のテストにだけ使ってよい。`tests/contract/cli_results.rs` の `sync_fixture` は非公開で書き込み先の名前も固定されており、`tests/contract/merge_support.rs` の `Fixture` は設定と差し替えの組み立てが非公開で sync の入口を持たず、`Fixture::write` は親ディレクトリを作らない。sync の補助関数は `merge_support.rs` の中に足す（非公開の組み立てを使い回すため）か、`tests/contract/merge_paths.rs` の `setup` の形を写し、ディレクトリの構成では先に配下のディレクトリを作る。

既存要件（REQ-merge-011〜014・018、REQ-backup-*）の領域に残った見逃しは、S4 で既存のテストを変えずに新しいテストを足して落としてよい。そのテストにはそれが確かめる既存要件の ID で印を付ける（書き込み直前の確認は REQ-merge-011、親ディレクトリと新規ファイルの権限は REQ-merge-012、権限の維持は REQ-merge-013、権限の複製は REQ-merge-014（FLAG-merge-002〜004 の挙動は確かめない）、指定先だけの更新は REQ-merge-018、バックアップの呼び出しは docs/ir/backup/ の該当する要件）。新しいテストは `tests/contract/` の下の merge 用のモジュール（既存の `merge_results.rs` に足すか、新しいモジュールを作る）に置き、`.kotowari/config.yaml` の `tests.files` は広げない。

読めないファイルは、既存の `tests/contract/merge_paths.rs` の `unreadable_source_fails_one_file_without_blocking_the_other_merge` と同じく権限を 0o200 に落として作り、テストの中でそのファイルを開けないことを先に確かめる（root で実行される環境では権限を落としても読めてしまうため、前提が崩れたことが分かるようにする）。更新時刻を揃える構成は `tests/contract/merge_paths.rs` の `directory_sync_without_checksum_uses_the_metadata_quick_check` と同じく標準ライブラリの `File::set_modified` で作る。ローカルの走査は更新時刻を秒単位で持つ（`src/local/mod.rs`）ため、同じ秒に作ったファイルは更新時刻が同じとみなされる。更新時刻が違う構成が要るときは、書き込み先の更新時刻を `File::set_modified` で読み込み元から 60 秒以上ずらし、merge や sync を呼ぶ前に秒単位の更新時刻が違うことをテストの中で確かめる。中身が違うファイルを書かせる構成では、左右の中身の長さを変えるか更新時刻を 60 秒以上ずらし、同じ秒に作られてメタデータで等しいとみなされないようにする。

新しく書くテストは FLAG-merge-001 から 006 の挙動を確かめない。具体的には、中身まで同じファイルを明示したときの skipped と出力（FLAG-merge-001）、--with-permissions の挙動（FLAG-merge-002 から 004）、中身を読み比べない書き込み先の読み取り失敗の error（FLAG-merge-005）、書き込み直前の確認で失敗したときの error の文言と他のファイルの扱い（FLAG-merge-006）を検証の対象に含めない。REQ-merge-019 の error は "read failed: " で始まること、読めなかった側の "left: " と "right: " が出ること、両側が読めないとき二つが "; " でつながることを確かめ、原因の文言（OS のエラーの文言）は確かめない。

最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG（FLAG-merge-001 から 006）の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。別の文脈を立てられないときは登録せず候補として手渡す。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の merge 用のモジュール（`merge_results.rs` と `merge_support.rs` への追加、または新しいモジュール）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/merge-write-test-cleanup.md`（新規。変異テストの結果と見逃しの決着、根拠にしたテストの記録）

既存のテストは消さず、書き換えない。製品コードは変えない。

## Step order and prerequisites

S1 の整理前の結果が S4 の比較の基準になるため最初に取る。S2 と S3 は要件ごとの審査で互いに独立しているが、同じモジュールを書き換えるため順に行う。S4 で整理後の変異テストを回して見逃しを決着させる。S5 で全体を確かめる。

S4 には利用者の判断を待つ区切りがある。新しい FLAG の候補や verification の見直しの候補があれば、`docs/testing/merge-write-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/merge-write`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-testing-012（review）, REQ-testing-013（review） | — |
| S2 | REQ-merge-019, REQ-merge-020 | — |
| S3 | REQ-merge-021, REQ-merge-022 | — |
| S4 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S5 | 上記すべて | — |

## Left to the implementer

- 新しいテストを既存の `merge_results.rs` に足すか新しいモジュールに置くか、とその名前
- 補助関数を `merge_support.rs` に足すか、テストのモジュールに置くか

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 要件を確かめるために FLAG-merge-001 から 006 の挙動を確かめる必要が生じた
- 要件の verification が要件の性質に合わないと判断した（IR は直さず、S4 で候補として手渡す）
- 要件を確かめるために製品コードの挙動や公開範囲を変える必要が生じた
- 変異テストの見逃しや新しいテストが不具合の疑いを示した（FLAG として記録するかは利用者が決めるため、その見逃しは未決着のまま報告する）
- 変異テストが WSL のメモリを使い切りそうな兆候（スクリプトの上限に当たって失敗し続ける）があり、並列数を下げても解消しない

## Test command

```sh
cargo nextest run --all-features
```

変異テストは `scripts/mutants.sh` から実行し、cargo-mutants を直接実行しない。並列数を 2 より上げない。

## Out of scope

- FLAG-merge-001 から FLAG-merge-006 の決着と、その挙動の修正
- 既存要件（REQ-merge-005・006・011〜014・017・018、REQ-cli-010）の印付きテストの見直し
- symlink・削除・パスの検査と、変更のまとまりを選ぶマージのテスト（merge の後の回で扱う）
- テストの削除
- 面の検査（`surface`）の導入

## Steps

### S1: 整理前の変異テストの見逃しを記録する

- Purpose: 見逃しの決着の基準にするため、整理前の見逃しを残す
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-013
- Prerequisites: none
- May change: docs/testing/merge-write-test-cleanup.md
- Done when: `src/service/merge_flow.rs`、`src/cli/tolerant_io.rs`、`src/service/status.rs` を一回の実行にまとめた変異テストの全体の集計（caught・survived・timeout・unviable）、ファイルごとの内訳、見逃し一件ずつの位置と変異の内容と決着の対象かどうか（Approach and why の区別）が、実行したコミットとともに `docs/testing/merge-write-test-cleanup.md` に書かれている。検知のうち、変異と関係のないテスト（tui_merge や agent_ssh のように負荷の下で落ちるもの）だけで検知されたものが名前とともに記録されている
- Shown by: artifact — `scripts/mutants.sh src/service/merge_flow.rs src/cli/tolerant_io.rs src/service/status.rs` をバックグラウンドで実行して完了を待ち、その出力を docs/testing/merge-write-test-cleanup.md の「整理前」の節に集計と見逃しの表として書く。ファイルごとの内訳と、どのテストが検知したかは、`target/mutants-run/mutants.out/outcomes.json` と `target/mutants-run/mutants.out/log/` の変異ごとの記録から読む（次の実行でスクリプトが `target/mutants-run` を消すため、比較に使う控えを target/ の外の一時ディレクトリに写しておく）
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 実行がメモリ上限で失敗し続ける、負荷の下で落ちるテストによる見かけの検知が多く、整理前と整理後の比較が成り立たない

### S2: 読めないときの error の形とディレクトリ指定の既定の比べ方の根拠テストを書いて印を付ける

- Purpose: 読み取りの失敗の error の形と、--checksum のないディレクトリ指定の比べ方に、merge と sync の入口を通す根拠テストを置く
- Specification: docs/ir/merge/read-failure.md#REQ-merge-019, docs/ir/merge/comparison.md#REQ-merge-020
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の merge 用モジュール, docs/testing/merge-write-test-cleanup.md
- Done when: REQ-merge-019 と REQ-merge-020 の `kotowari query` の tests が空でない。REQ-merge-019 の印付きテストは merge と sync のそれぞれで、ファイルを明示した中身の読み比べで読み込み元だけ・書き込み先だけ・両側が読めない構成の failed の error が "read failed: " で始まり、読めなかった側について "left: " か "right: " が出て、両側のときは二つが "; " でつながることを確かめる（原因の文言は確かめない）。REQ-merge-020 の印付きテストは merge と sync のそれぞれで、--checksum のないディレクトリ指定で、サイズと更新時刻が同じで中身が違うファイルが書かれないことと、サイズが同じで更新時刻が 60 秒以上違い中身が同じファイルが書かれない（merged に出ず、書き込み先の中身と更新時刻が変わらない）ことを確かめる。同じディレクトリに、左右で長さの違う中身を持つ対照のファイルを置き、merge の結果が per-file の出力（MergeCommandOutput::Files）になって対照のファイルだけが merged に出ることを確かめる。書き込むものがないときの出力（"no files to merge in the specified path(s)" と NoFilesToMerge）は確かめない。どのテストを根拠にし、どの既存テストの組み方を手本にしたかが docs/testing/merge-write-test-cleanup.md にある
- Shown by: test — tests/contract/merge_paths.rs の explicit_merge_checks_readability_even_when_file_sizes_differ・unreadable_source_fails_one_file_without_blocking_the_other_merge・two_unreadable_sides_are_not_reported_as_identical_empty_files・directory_sync_without_checksum_uses_the_metadata_quick_check を手本にし、execute_merge と execute_sync を通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: 一つのテストで複数の構成を確かめるか分けるか
- Stop and hand back if: 権限を落としてもファイルを開けてしまう（root で実行される環境）、書き込み先の更新時刻を揃えられず REQ-merge-020 の構成が作れない

### S3: ディレクトリ指定と複数パスの書き込みの根拠テストを書いて印を付ける

- Purpose: ディレクトリ指定で書き込むファイルと、複数のパスを指定したときの書き込みに、merge の入口を通す根拠テストを置く
- Specification: docs/ir/merge/basic.md#REQ-merge-021, docs/ir/merge/basic.md#REQ-merge-022
- Prerequisites: S2
- May change: tests/contract.rs, tests/contract/ の merge 用モジュール, docs/testing/merge-write-test-cleanup.md
- Done when: REQ-merge-021 と REQ-merge-022 の `kotowari query` の tests が空でない。REQ-merge-021 の印付きテストは、ディレクトリを指定した merge で、配下の中身の違うファイル（左右で中身の長さが違うもの）が上書きされ、読み込み元にだけあるファイルが書き込み先に作られ、中身が同じファイルが書かれない（merged に出ない）ことを確かめる。書き込み先にだけあるファイルを構成に含める場合は、そのファイルが変わらないことだけを確かめ、skipped の reason は確かめない（symlink と削除の回の範囲のため）。REQ-merge-022 の印付きテストは、二つのパスを指定した merge でそれぞれが書かれることと、同じパスを重ねて指定した merge で merged にそのパスが一度だけ出て、failed が空で、終了コードが 0 であることを確かめる（failed の error の文言は確かめない）。どのテストを根拠にしたかが docs/testing/merge-write-test-cleanup.md にある
- Shown by: test — tests/cli_merge.rs の test_merge_multiple_files・test_merge_directory・test_merge_duplicate_paths_deduplicated を手本にし、execute_merge を通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: none
- Stop and hand back if: 中身の違うファイルを、長さを変えても更新時刻をずらしても Modified と判定させられない

### S4: 整理後の変異テストで見逃しを比べ、全て決着させる

- Purpose: 足したテストで見逃しが減ったかを確かめ、決着の対象に残った見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S3
- May change: tests/contract/ の merge 用モジュール（テストの追加。Approach and why のとおり既存要件の ID の印を付けてよい）, tests/contract.rs, .kotowari/mutants-equivalents.yaml, docs/testing/merge-write-test-cleanup.md
- Done when: S1 と同じ三ファイルの変異テストの見逃しが、S1 の見逃しと、S1 で負荷の下で落ちるテストだけに検知された変異とを合わせたものの部分集合になっている。決着の対象の見逃しの全てに、足したテスト、同等変異の一覧への理由付きの登録（別の文脈のエージェントが落とすテストを書けなかった試みつき）、既存の FLAG-merge-0NN の範囲としての記録、新しい FLAG の候補としての報告のどれかが docs/testing/merge-write-test-cleanup.md に記録されている。決着の対象でない見逃しは一覧として記録されている。同じ文書に REQ-merge-019 から 022 の verification とそれが要件の性質に合う理由が一行ずつあり、合わないと判断したものは IR を直さず候補として挙がっている
- Shown by: external — `scripts/mutants.sh src/service/merge_flow.rs src/cli/tolerant_io.rs src/service/status.rs` の出力と S1 の記録を突き合わせた結果を docs/testing/merge-write-test-cleanup.md の「整理後」の節に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: none
- Stop and hand back if: 見逃しが不具合の疑いを示した、または verification の見直しの候補がある（どちらも文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S4 は完了しない。実装と IR は直さない）、負荷の下で落ちるテストによる見かけの検知が集計に混ざり、比較が成り立たない

### S5: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/merge/read-failure.md#REQ-merge-019, docs/ir/merge/comparison.md#REQ-merge-020, docs/ir/merge/basic.md#REQ-merge-021, docs/ir/merge/basic.md#REQ-merge-022, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S4
- May change: none
- Done when: REQ-merge-019 から REQ-merge-022 の全てで `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in $(seq -f 'REQ-merge-%03g' 19 22); do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
