# Plan: テストのない要件と例にテストを付け、IR と違う動きを直す

## Goal

`kotowari check` の requirement_without_test と scenario_without_test が 0 件になり、CI の「Check specifications」が通る。テストを書いて IR と違う動きが見つかったものは、IR のとおりに直してある。

## Specification

IR は `docs/ir/` にある。この計画は IR を変えない。扱うのは、2026-10-12 の main（`fd060ed`）で `kotowari check` がテストなしとして出す次の 37 件である。

- diff の比べ方: `docs/ir/cli/diff-strategy.md#REQ-cli-080`、`#REQ-cli-081`、`#REQ-cli-082`、`#REQ-cli-083`、`#REQ-cli-084`、`#REQ-cli-085`（例 EX-cli-072〜078）
- 走査: `docs/ir/scan/limits.md#REQ-scan-010`、`#REQ-scan-013`、`#REQ-scan-014`（例 EX-scan-012・013）、`docs/ir/scan/listing.md#REQ-scan-012`（例 EX-scan-014）
- バックアップ: `docs/ir/backup/failure.md#REQ-backup-045`（例 EX-backup-026）、`docs/ir/backup/rollback-cli.md#REQ-backup-044`（例 EX-backup-025）
- CLI のそのほか: `docs/ir/cli/diagnostics.md#REQ-cli-093`（例 EX-cli-092）、`docs/ir/cli/diff-output.md#REQ-cli-086`（例 EX-cli-070）、`docs/ir/cli/directory-paths.md#REQ-cli-087`（例 EX-cli-071）、`docs/ir/cli/safety.md#REQ-cli-091`（例 EX-cli-091）、`docs/ir/cli/status.md` の例 EX-cli-089
- merge と設定: `docs/ir/merge/basic.md#REQ-merge-035`（例 EX-merge-042）、`docs/ir/merge/comparison.md` の例 EX-merge-041、`docs/ir/config/filters.md#REQ-config-038`

## Approach and why

どの段も、要件と例を `kotowari query ID` で読み、要件が言うとおりの契約テストを書く。テストは `crates/remote-merge/tests/contract/**`（印が数えられる場所）に置き、要件が三つの経路を挙げていれば三つの経路で確かめる。

書いたテストが通れば、それで終わり（今の動きが IR どおりだったということ）。落ちたら、コードが IR と違うので、IR のとおりにコードを直す（RED → GREEN）。テストが落ちて、IR のとおりに直すと別の要件と食い違う、または IR が二通りに読める、と分かったら直さずに差し戻す。

すでに品質の台帳で分かっている、IR と違う動きが三つある。どれもこの計画の段で表に出るので、その段で直す。

- sr-1（REQ-scan-010 と REQ-scan-004）: 走査の件数が上限に当たったとする条件が経路で違う。SSH は上限の後の次の行が exclude に当たる項目でも打ち切りにし（刈り込めないパターンのとき）、ローカルは exclude を上限の判定の前に除き、エージェントは載せる項目がもう一件あるときだけ打ち切りにする。
- sr-2（REQ-scan-014）: エージェントの走査が、読めないディレクトリとディレクトリの項目の読み取りの失敗を warn にして続け、欠けた一覧を返す。
- so-2（EX-cli-089、TBL-cli-001）: `remote_merge_core::tree::compare_metadata` は更新時刻が完全に一致すれば Equal にし、TBL-cli-001 の「片側でも秒の精度しかなく秒が同じなら中身を読み比べる」を見ていない。

## Scope of change

- 契約テスト `crates/remote-merge/tests/contract/` の中（新しいファイルを足してよい）
- テストが落ちたときに直す製品のコード（どのクレートでもよい。ただし直す理由は IR の要件で示す）

## Step order and prerequisites

S1〜S5 は互いに依存しないので、この順に一つずつ進め、段ごとにコミットする。S6 で全体を確かめる。

## Verification map

| 要件・例 | 示す段 | 確かめ方 |
|---|---|---|
| REQ-cli-080〜085、EX-cli-072〜078 | S1 | 契約テスト（diff） |
| REQ-scan-010・013・014・012、EX-scan-012・013・014 | S2 | 契約テスト（走査。三つの経路） |
| REQ-backup-045・044、EX-backup-026・025 | S3 | 契約テスト（バックアップと rollback） |
| REQ-cli-093・086・087・091、EX-cli-092・070・071・091・089 | S4 | 契約テスト（logs、diff の出力、ディレクトリのパス、dry-run の終了コード、status の比べ方） |
| REQ-merge-035、EX-merge-042・041、REQ-config-038 | S5 | 契約テスト（merge と設定） |

## Left to the implementer

- テストのファイルの置き場所と名前、偽サーバや既存の補助の使い方
- 要件ごとにテストを一つにまとめるか分けるか（要件と例の両方に印が付けばよい）
- 落ちたテストを直すときの、コードの直し方の内部の形

## Stop conditions

- テストを IR どおりに書くと、IR の文が二通りに読めるか、ほかの要件と食い違うと分かったとき
- 落ちたテストを IR どおりに直すと、ほかの契約テストが要件どおりでなくなるとき
- 要件の条件を作る手段が、契約テストでもコンテナでも作れないとき（そのときは、作れない理由と、代わりに確かめた方法を報告する）

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

エージェントか SSH の読み方を直したときは、最後に `scripts/run-container-e2e.sh` を一度走らせる（Docker が要る）。

## Out of scope

- IR を変えること（変える必要が分かったら差し戻す）
- 不安定な `merge_paths::a_directory_link_does_not_merge_its_children_into_a_different_kind_of_target`（品質の台帳の so-1。別に調べる）

## Steps

### S1: diff の比べ方の要件と例にテストを付ける

- Purpose: diff の比べ方の要件と例をテストで確かめ、違う動きを直す。
- Specification: docs/ir/cli/diff-strategy.md#REQ-cli-080, docs/ir/cli/diff-strategy.md#REQ-cli-081, docs/ir/cli/diff-strategy.md#REQ-cli-082, docs/ir/cli/diff-strategy.md#REQ-cli-083, docs/ir/cli/diff-strategy.md#REQ-cli-084, docs/ir/cli/diff-strategy.md#REQ-cli-085
- Prerequisites: なし
- May change: crates/remote-merge/tests/contract/ の中、テストが落ちたときに直す製品のコード
- Done when: REQ-cli-080〜085 と EX-cli-072〜078 のどれにも印の付いたテストがあり、全テストが通る
- Shown by: test — 各要件と例の契約テスト（印 `// @kotowari[REQ-cli-0nn, EX-cli-0nn]`）。落ちたものは RED を報告してから直す
- Left to the implementer: テストの置き方
- Stop and hand back if: IR の文が二通りに読めるか、ほかの要件と食い違うとき

### S2: 走査の要件と例にテストを付け、経路差を直す

- Purpose: 走査の件数・時間切れ・読めないディレクトリ・UTF-8 でない名前の要件を三つの経路で確かめ、sr-1・sr-2 を直す。
- Specification: docs/ir/scan/limits.md#REQ-scan-010, docs/ir/scan/limits.md#REQ-scan-013, docs/ir/scan/limits.md#REQ-scan-014, docs/ir/scan/listing.md#REQ-scan-012
- Prerequisites: なし
- May change: crates/remote-merge/tests/contract/ の中、crates/remote-merge-agent/src/agent/、crates/remote-merge-ssh/src/、crates/remote-merge-engine/src/、crates/remote-merge-core/src/
- Done when: REQ-scan-010・013・014・012 と EX-scan-012・013・014 のどれにも印の付いたテストがあり、三つの経路が同じ結果になり、全テストと Docker の e2e が通る
- Shown by: test — 三つの経路の契約テスト（印 `// @kotowari[REQ-scan-0nn, EX-scan-0nn]`）。エージェントの読めないディレクトリ（sr-2）と件数の上限の判定（sr-1）は RED を報告してから直す
- Left to the implementer: 経路をそろえる内部の形（エージェントの通信の型を変えるなら版を上げる。今は 8）
- Stop and hand back if: 件数の上限の判定をそろえる形が、REQ-scan-004・REQ-scan-008 の報告の振る舞いを変える必要があるとき

### S3: バックアップの集約先の失敗の要件と例にテストを付ける

- Purpose: 集約先の準備に失敗したときと、読めない集約先の中身の要件を確かめる。
- Specification: docs/ir/backup/failure.md#REQ-backup-045, docs/ir/backup/rollback-cli.md#REQ-backup-044
- Prerequisites: なし
- May change: crates/remote-merge/tests/contract/ の中、テストが落ちたときに直す製品のコード
- Done when: REQ-backup-045・044 と EX-backup-026・025 に印の付いたテストがあり、全テストが通る
- Shown by: test — 契約テスト（印 `// @kotowari[REQ-backup-04n, EX-backup-02n]`）。root で動くと権限の失敗を作れないので、そのときはテストを飛ばす形にする
- Left to the implementer: 失敗を起こす方法
- Stop and hand back if: 失敗を起こす方法が契約テストで作れないとき

### S4: CLI のそのほかの要件と例にテストを付ける

- Purpose: logs の水準、片側だけの 0 バイトのファイル、片側だけのディレクトリ、dry-run の終了コード、status の秒の精度の比べ方を確かめる。
- Specification: docs/ir/cli/diagnostics.md#REQ-cli-093, docs/ir/cli/diff-output.md#REQ-cli-086, docs/ir/cli/directory-paths.md#REQ-cli-087, docs/ir/cli/safety.md#REQ-cli-091
- Prerequisites: なし
- May change: crates/remote-merge/tests/contract/ の中、crates/remote-merge-core/src/tree.rs、テストが落ちたときに直す製品のコード
- Done when: REQ-cli-093・086・087・091 と EX-cli-092・070・071・091・089 に印の付いたテストがあり、全テストが通る
- Shown by: test — 契約テスト（印 `// @kotowari[...]`）。EX-cli-089（so-2）は RED を報告してから `compare_metadata` を TBL-cli-001 のとおりに直す
- Left to the implementer: 秒の精度しかないことの判定の内部の形
- Stop and hand back if: 秒の精度の判定が、どの経路の値が秒の精度かを IR から決められないとき

### S5: merge と設定の要件と例にテストを付ける

- Purpose: 両側にないパスの merge、比べ方の例、"../" を含む exclude の扱いを確かめる。
- Specification: docs/ir/merge/basic.md#REQ-merge-035, docs/ir/config/filters.md#REQ-config-038
- Prerequisites: なし
- May change: crates/remote-merge/tests/contract/ の中、テストが落ちたときに直す製品のコード
- Done when: REQ-merge-035・REQ-config-038 と EX-merge-042・041 に印の付いたテストがあり、全テストが通る
- Shown by: test — 契約テスト（印 `// @kotowari[...]`）
- Left to the implementer: テストの置き方
- Stop and hand back if: IR の文が二通りに読めるとき

### S6: テストのない要件と例が 0 件になる

- Purpose: `kotowari check` のテスト側の指摘が 0 件になり、CI の「Check specifications」が通ることを確かめる。
- Specification: docs/ir/cli/diff-strategy.md#REQ-cli-080, docs/ir/scan/limits.md#REQ-scan-014
- Prerequisites: S1、S2、S3、S4、S5
- May change: 印の付け直しに要るテストのファイルだけ
- Done when: `kotowari check --format json` の findings に requirement_without_test と scenario_without_test が 0 件で、残るのは notice（too_many_requirements）だけである
- Shown by: check — `kotowari check --format json | jq '[.findings[] | select(.severity == "error")] | length'` が 0、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features`、`scripts/run-container-e2e.sh`
- Left to the implementer: none
- Stop and hand back if: error の findings が消えないとき
