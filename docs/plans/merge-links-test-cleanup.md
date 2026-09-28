# Plan: merge の symlink と削除のテストを要件の根拠に整理する

## Goal

merge の symlink と削除の取り込みで加えた各要件に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、その過程と残った変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/merge/symlink.md#REQ-merge-023`
- `docs/ir/merge/deletion.md#REQ-merge-024`、`#REQ-merge-025`、`#REQ-merge-026`、`#REQ-merge-027`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`、`#REQ-testing-013`（いずれも review）

判断の出典は `docs/decision/records/2026-09-28-adopt-merge-links.md`（取り込み、FLAG-merge-008 から 014、テストの仕分けの件数と範囲外にしたもの）と `docs/decision/records/2026-09-27-test-method-selection.md`（手法の選び方、変異テストとメモリ上限）。要件の本文は `kotowari query REQ-merge-0nn` で読む。同じ手順を merge の書き込みの中身で行った記録 `docs/testing/merge-write-test-cleanup.md` が記録の書き方と根拠テストの組み方の手本になる。

## Approach and why

取り込みの仕分けでは、この範囲のテスト 79 件のうち実装詳細をなぞるだけのテストはなかった。そのためこの計画は削除を行わず、根拠テストのない新しい要件 5 件に根拠テストを足し、変異テストで見逃しを決着させることに絞る。既存要件（REQ-merge-001〜004・007・008・016）の印付きテストの本体は見直さない。既存のテストは消さず、書き換えない。

最初に整理前の変異テストの見逃しを記録する。対象は symlink と削除の規則を持つ `src/service/merge.rs`、`src/service/sync.rs`、`src/service/path_resolver.rs`、`src/service/merge_flow.rs` の四つにする。`src/service/output.rs` は status・diff・sync・rollback の出力が大半を占め、対象に加えると変異テストの実行時間が大きく増えるため対象にしない（削除のテキストの行は S3 で `execute_merge` の結果を `format_merge_text` に渡すテストで確かめる）。

見逃しのうち決着の対象にするのは次の関数のものに限る: `src/service/merge.rs` の `find_symlink_target`・`determine_merge_action`、`src/service/sync.rs` の `plan_deletions`・`skip_symlink_deletions`、`src/service/path_resolver.rs` の `check_path_traversal`・`filter_merge_candidates`、`src/service/merge_flow.rs` の `execute_single_merge` の symlink の分岐（`SkipDifferentKind`・`CreateSymlink`・`ReplaceSymlinkWithFile`）と `execute_deletions`。merge の指定・確認・出力の整理と書き込みの中身の整理がこれらの見逃しを後の回に残したため、ここで決着させる。他の関数（`src/service/merge.rs` の `plan_merge` などの指定・確認・出力の規則、`src/service/sync.rs` の状態と集計、`src/service/path_resolver.rs` のパスの解決、`src/service/merge_flow.rs` の通常ファイルの経路と hunk の経路）の見逃しは、前の回で決着済みか他の回の範囲のため記録だけする。FLAG-merge-008 から 014 の挙動（スキップ理由の文言、dry-run での種類の違い、パス脱出の拒否の範囲と文言、エージェントの経路、dry-run の削除予定の表示、削除の直前の調査と削除の失敗）に関わる見逃しは、その FLAG の範囲として記録する。

根拠テストは merge と sync の公開された入口を通す。関数呼び出しで `remote_merge::cli::merge::execute_merge` と `remote_merge::cli::sync::execute_sync` に `RuntimeTargets::with_local` でサーバをローカルのディレクトリに差し替えて渡す（`tests/contract/merge_support.rs` の `fixture`・`fixture_with_backup`・`merge_json`・`sync_json`・`args`・`sync_args`）。sync の関数呼び出しは `force: true` を渡す（`sync_args` の既定。`force: false` で書き込む予定があると `execute_sync` がテストのプロセスの標準入力を読むため）。ただし REQ-merge-025 は --force がないことを条件にするため、merge では `force: false` の関数呼び出しで確かめ（merge は確認のプロンプトを出さない）、sync では `delete: true`・`force: false`・`dry_run: false` で、書き込み先にだけある `.env` の他に書き込むものも削除するものもない構成で確かめる（書き込む予定がないため `execute_sync` は確認のプロンプトの前に戻り、標準入力を読まない。dry-run は何も消さないため「削除しない」ことの根拠にならず使わない）。merge と sync の引数には、書き込み先にだけあるパスそのものか "." を渡す（`plan_deletions` と `filter_merge_candidates` は解決した引数に含まれる書き込み先だけのファイルしか扱わないため）。REQ-merge-023・024・025 は merge と sync の両方を主語にするため、両方の入口で確かめる。REQ-merge-027 のテキスト出力は、`execute_merge` の結果（`MergeCommandOutput::Files`）を公開された `remote_merge::service::output::format_merge_text` に渡して確かめる（`merge_support.rs` の `merge_json` の隣に `merge_text` を足す）。テキストと JSON の振り分けと実行ファイルの出力は REQ-cli-049 の実行ファイルのテストが確かめており、ここでは削除の行の形だけを確かめるため、SSH の fixture は使わない。`merge_support.rs` の `run_cli` は書き込み先を差し替えられず接続前に止まる指定のためのものなので、この計画では使わない。新しいテストは `tests/contract/` の下の merge 用のモジュール（新しく `merge_links.rs` を作るか既存の `merge_write.rs` に足す）に置き、`.kotowari/config.yaml` の `tests.files` は広げない。symlink は標準ライブラリの `std::os::unix::fs::symlink` で作る。REQ-merge-023 のテストは merge と sync の両方で引数にリンクのパスだけを渡す（"." を渡すとリンクの指す読み込み元のファイルも読み込み元にだけあるファイルとして正当に書き込まれ、「リンクの先へは辿らない」ことを確かめられなくなるため）。書き込み先で辿っていないことは `fs::symlink_metadata(…).file_type().is_symlink()` と `fs::read_link` で確かめる。

新しく書くテストは FLAG-merge-008 から 014 と FLAG-cli-016 から 026 の挙動を確かめない。具体的には、種類の違いと symlink の削除のスキップ理由の文言（FLAG-merge-008・009）、dry-run での種類の違い（FLAG-merge-010）、パス脱出の拒否（FLAG-merge-011）、エージェントの経路（FLAG-merge-012）、dry-run の削除予定の表示（FLAG-merge-013）、削除の直前の調査と削除の失敗の文言（FLAG-merge-014）、スキップだけのときの終了コード（FLAG-cli-016）、機密ファイルの通知（FLAG-cli-022）を検証の対象に含めない。REQ-merge-024・025 のテストでは skipped の reason を要件の文言どおりに確かめてよいが、終了コードと標準エラーは確かめない。REQ-merge-027 のテストは --dry-run を使わない。

最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。既存要件（REQ-merge-001〜004・007・008・016）の領域に残った見逃しは、既存のテストを変えずに新しいテストを足して落としてよく、そのテストにはそれが確かめる既存要件の ID で印を付ける。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。別の文脈を立てられないときは登録せず候補として手渡す。変異の一時的な書き換えで確かめるときは本体の作業ツリーで行い、確かめた後に `git diff --stat src/` が空に戻ることを確かめる。作業ツリーやブランチは作らない。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の merge 用のモジュール（`merge_links.rs` の新規作成、または `merge_write.rs`・`merge_support.rs` への追加）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/merge-links-test-cleanup.md`（新規。変異テストの結果と見逃しの決着、根拠にしたテストの記録）

既存のテストは消さず、書き換えない。製品コードは変えない。

## Step order and prerequisites

S1 の整理前の結果が S4 の比較の基準になるため最初に取る。S2 と S3 は要件ごとの審査で互いに独立しているが、同じモジュールを書き換えるため順に行う。S4 で整理後の変異テストを回して見逃しを決着させる。S4 の変異テストは、S4 で足すテストも含めた最後のテストの状態で回す（テストを足したら回し直す）。S5 で全体を確かめる。

S4 には利用者の判断を待つ区切りがある。新しい FLAG の候補や verification の見直しの候補があれば、`docs/testing/merge-links-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/merge-links`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-testing-012（review）, REQ-testing-013（review） | — |
| S2 | REQ-merge-023, REQ-merge-024, REQ-merge-025 | — |
| S3 | REQ-merge-026, REQ-merge-027 | — |
| S4 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S5 | 上記すべて | — |

## Left to the implementer

- 新しいテストを置くモジュールとその名前
- 補助関数を `merge_support.rs` に足すか、テストのモジュールに置くか

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 要件を確かめるために FLAG-merge-008 から 014 または FLAG-cli-016 から 026 の挙動を確かめる必要が生じた
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

- FLAG-merge-008 から FLAG-merge-014 の決着と、その挙動の修正
- 既存要件（REQ-merge-001〜004・007・008・016）の印付きテストの見直し
- 変更のまとまりを選ぶマージのテスト（merge の最後の回で扱う）
- root_dir の解決（config）と symlink のツリー取得（scan）
- テストの削除
- 面の検査（`surface`）の導入

## Steps

### S1: 整理前の変異テストの見逃しを記録する

- Purpose: 見逃しの決着の基準にするため、整理前の見逃しを残す
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-013
- Prerequisites: none
- May change: docs/testing/merge-links-test-cleanup.md
- Done when: `src/service/merge.rs`、`src/service/sync.rs`、`src/service/path_resolver.rs`、`src/service/merge_flow.rs` を一回の実行にまとめた変異テストの全体の集計（caught・survived・timeout・unviable）、ファイルごとの内訳、見逃し一件ずつの位置と変異の内容と決着の対象かどうか（Approach and why の区別）が、実行したコミットとともに `docs/testing/merge-links-test-cleanup.md` に書かれている。検知のうち、変異と関係のないテスト（tui_merge や agent_ssh のように負荷の下で落ちるもの）だけで検知されたものが名前とともに記録されている
- Shown by: artifact — `scripts/mutants.sh src/service/merge.rs src/service/sync.rs src/service/path_resolver.rs src/service/merge_flow.rs` をバックグラウンドで実行して完了を待ち、その出力を docs/testing/merge-links-test-cleanup.md の「整理前」の節に集計と見逃しの表として書く。ファイルごとの内訳とどのテストが検知したかは `target/mutants-run/mutants.out/outcomes.json` と `target/mutants-run/mutants.out/log/` から読み、次の実行でスクリプトが `target/mutants-run` を消すため、比較に使う控えを target/ の外の一時ディレクトリに写しておく
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 実行がメモリ上限で失敗し続ける、負荷の下で落ちるテストによる見かけの検知が多く、整理前と整理後の比較が成り立たない

### S2: symlink の作成と削除しないファイルのスキップの根拠テストを書いて印を付ける

- Purpose: 書き込み先にない symlink の作成と、書き込み先だけのファイル・機密ファイルのスキップに、merge と sync の入口を通す根拠テストを置く
- Specification: docs/ir/merge/symlink.md#REQ-merge-023, docs/ir/merge/deletion.md#REQ-merge-024, docs/ir/merge/deletion.md#REQ-merge-025
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の merge 用モジュール, docs/testing/merge-links-test-cleanup.md
- Done when: REQ-merge-023・024・025 の `kotowari query` の tests が空でない。REQ-merge-023 の印付きテストは merge と sync のそれぞれで、引数にリンクのパスだけを渡し、読み込み元の末尾の symlink（読み込み元の root の中の実在するファイルを指すもの）を書き込み先にないパスへ書き込んだとき、書き込み先のそのパスが symlink になり `fs::read_link` のリンク先の文字列が読み込み元と同じで、リンク先のファイルが書き込み先に作られないことを確かめる。REQ-merge-024 の印付きテストは merge と sync のそれぞれで、--delete のない実行で書き込み先にだけある通常ファイルが変わらず、skipped にそのパスが reason "right-only file (use --delete to remove)" で出ることを確かめる。REQ-merge-025 の印付きテストは merge（force: false）と sync（force: false・dry_run: false、書き込み先にだけある `.env` の他に書き込むものも削除するものもない構成）のそれぞれで、--delete の実行で書き込み先にだけある機密ファイル（`.env`）が削除されず、skipped にそのパスが reason "sensitive file (use --force to include)" で出ることを確かめる。どのテストも終了コードと標準エラーは確かめない。どのテストを根拠にし、どの既存テストの組み方を手本にしたかが docs/testing/merge-links-test-cleanup.md にある
- Shown by: test — src/service/merge.rs の test_determine_merge_action_source_symlink_target_not_exists、src/service/path_resolver.rs の filter_merge_candidates の単体テスト、src/service/sync.rs の plan_deletions の単体テスト、tests/contract/merge_paths.rs の sync_without_delete_keeps_destination_only_regular_files を手本にし、execute_merge と execute_sync を通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: 一つのテストで merge と sync の両方を確かめるか分けるか
- Stop and hand back if: sync で機密ファイルのスキップが結果に出ない、または force: false の sync が標準入力を読んでしまう（その場合は構成を報告して作業を返す）

### S3: 削除の結果の JSON とテキストの根拠テストを書いて印を付ける

- Purpose: 削除したファイルの JSON とテキストの出し方に、merge の入口を通す根拠テストを置く
- Specification: docs/ir/merge/deletion.md#REQ-merge-026, docs/ir/merge/deletion.md#REQ-merge-027
- Prerequisites: S2
- May change: tests/contract.rs, tests/contract/ の merge 用モジュール, docs/testing/merge-links-test-cleanup.md
- Done when: REQ-merge-026・027 の `kotowari query` の tests が空でない。REQ-merge-026 の印付きテストは --delete の merge で、バックアップが有効な構成では削除したファイルが JSON の deleted に path、status "ok"、backup（"セッションID/パス" の形で、パスの部分が削除したファイルのパスと一致し、セッションIDの部分が "/" を含まず fixture のバックアップの集約先に実在するセッションの名前と一致する。現在時刻とは比べない）で出ることを、バックアップが無効な構成では backup の項目がないことを確かめる。REQ-merge-027 の印付きテストは --dry-run のない --delete の merge の結果を `format_merge_text` に渡し、バックアップが有効な構成では削除したファイルの行が "Deleted: パス (backup: …)" になり、バックアップが無効な構成では行がちょうど "Deleted: パス" になることを確かめる。どのテストを根拠にしたかが docs/testing/merge-links-test-cleanup.md にある
- Shown by: test — tests/contract/merge_paths.rs の sync_delete_removes_a_destination_only_regular_file と a_deleted_regular_file_is_backed_up_and_recreated_by_rollback、src/service/output.rs の test_format_merge_text_with_deleted と test_format_merge_text_deleted_no_backup を手本にし、execute_merge と format_json（REQ-merge-026）、execute_merge と format_merge_text（REQ-merge-027）を通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: none
- Stop and hand back if: バックアップが有効な構成で集約先がテストの外に書かれる

### S4: 整理後の変異テストで見逃しを比べ、全て決着させる

- Purpose: 足したテストで見逃しが減ったかを確かめ、決着の対象に残った見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S3
- May change: tests/contract/ の merge 用モジュール（テストの追加。Approach and why のとおり既存要件の ID の印を付けてよい）, tests/contract.rs, .kotowari/mutants-equivalents.yaml, docs/testing/merge-links-test-cleanup.md
- Done when: 最後のテストの状態で決着の対象の関数に絞って回した変異テストの見逃しが、S1 の同じ関数の見逃しと、S1 で負荷の下で落ちるテストだけに検知された変異とを合わせたものの部分集合になっている。決着の対象の見逃しの全てに、足したテスト、同等変異の一覧への理由付きの登録（別の文脈のエージェントが落とすテストを書けなかった試みつき）、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれかが docs/testing/merge-links-test-cleanup.md に記録されている。決着の対象でない見逃しは一覧として記録されている。同じ文書に REQ-merge-023 から 027 の verification とそれが要件の性質に合う理由が一行ずつあり、合わないと判断したものは IR を直さず候補として挙がっている
- Shown by: external — `scripts/mutants.sh --re '(find_symlink_target|determine_merge_action|plan_deletions|skip_symlink_deletions|check_path_traversal|filter_merge_candidates|execute_deletions|execute_single_merge)' src/service/merge.rs src/service/sync.rs src/service/path_resolver.rs src/service/merge_flow.rs`（決着の対象の関数だけに絞った実行。対象外の関数の見逃しは S1 の記録のまま扱う）の出力と、S1 の記録のうち同じ関数の部分を突き合わせた結果を docs/testing/merge-links-test-cleanup.md の「整理後」の節に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: none
- Stop and hand back if: 見逃しが不具合の疑いを示した、または verification の見直しの候補がある（どちらも文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S4 は完了しない。実装と IR は直さない）、負荷の下で落ちるテストによる見かけの検知が集計に混ざり、比較が成り立たない

### S5: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/merge/symlink.md#REQ-merge-023, docs/ir/merge/deletion.md#REQ-merge-024, docs/ir/merge/deletion.md#REQ-merge-025, docs/ir/merge/deletion.md#REQ-merge-026, docs/ir/merge/deletion.md#REQ-merge-027, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S4
- May change: none
- Done when: REQ-merge-023 から REQ-merge-027 の全てで `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in $(seq -f 'REQ-merge-%03g' 23 27); do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
