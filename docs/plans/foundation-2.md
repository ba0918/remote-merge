# Plan: 走査を一つの規則と二つの集め方にする

## Goal

走査（status・diff・merge・sync のツリーの取得と、diff の名前で指定したディレクトリの展開）は、一つの純粋な規則の関数群が結果を決め、経路は生の項目を集めるだけになる。ローカルとエージェントは同じ集め方を使い、SSH は find の出力を同じ生の項目に訳す。書き直した走査の IR（REQ-scan-017〜020、REQ-config-044 を含む）の要件と例がすべてテストで示され、`kotowari check` の error が 0 になる。

## Specification

- 方針: `docs/decision/records/2026-10-12-foundation.md`（A1〜A10）、`docs/decision/records/2026-10-12-foundation-2.md`（C1〜C9）
- 走査の IR: `docs/ir/scan/listing.md`、`docs/ir/scan/limits.md`、`docs/ir/scan/directory-links.md`、`docs/ir/scan/FLAGS.md`
- フィルタの IR: `docs/ir/config/filters.md`、`docs/ir/config/include.md`
- パスの確認（include の値と起点の確認に使う）: `docs/ir/paths/resolution.md`
- 道具の違い: `docs/remote-tools.md`
- 段 1 で入った型: `crates/remote-merge-core/src/relative_path/`（RelPath・EntryPath）

## Approach and why

**今の実装を参考にしない（foundation A1）。** 走査の規則と集め方は、上の IR と記録と `docs/remote-tools.md` だけから書く。今の走査の実装（エージェントの `tree_scan.rs`・`dir_ancestry.rs`、engine の `local/mod.rs` の walk、SSH の `client.rs` の走査・`tree_parser.rs` の find の組み立て・`find_records.rs`・`find_diagnostics.rs`・`child_listing.rs`、製品の `include_check.rs`、部分の走査の処理）は読まない・写さない。置き換えたら消す。読んでよいのは、差し替える境界（走査の結果の型 `FileTree`・`FileNode` と、走査を呼ぶ側の関数の形）とテストだけである。

規則を一か所に置く（C6）。規則の入力は生の項目と設定（exclude・include・件数の上限）と起点で、出力は一覧・不完全の報告・エラーである。どこへ降りるか（exclude に当たるディレクトリには降りない、ディレクトリへの symlink は辿る、循環はデバイス番号と inode 番号で止める）も規則が決め、集め方はそれに従って次に集める場所を受け取る。

SSH は find にリンク先を辿らせない（C5）。一度の find で部分木の生の項目を集め、ディレクトリへの symlink を辿るときだけ、そのリンク先をまとめてもう一度集める（C7）。exclude は、find のパターンで同じ意味に書けるものだけを find の刈り込みに使い、書けないものは規則の側で除く。find の標準エラーは、読めないディレクトリのパスとして読めるものだけを規則に渡し、読めないものは走査のエラーにする（握りつぶさない）。

## Scope of change

- 新しい規則と集め方: `crates/remote-merge-core/src/`（規則。純粋な関数）、ローカルとエージェントが共有する集め方の置き場所（core か engine）、SSH の集め方（`crates/remote-merge-ssh/src/`）
- 呼び出し側の差し替え: `crates/remote-merge/src/runtime/`、`crates/remote-merge/src/cli/`（diff のディレクトリの展開）、`crates/remote-merge-engine/src/`、`crates/remote-merge-agent/src/agent/`
- テスト: `crates/remote-merge/tests/contract/` に足す。既存のテストは、関数の形の変更に合わせる以外は変えず、IR と違う動きを固めていたら IR に合わせて直し、記録に C 番号で残す（foundation A6）
- 消すもの: 上の Approach に挙げた古い走査の実装

## Step order and prerequisites

S1 → S2 → S3 → S4 → S5 の順。S2・S3 は S1 の規則を使う。S4 は S2・S3 を使う。

## Verification map

| 要件・例 | 示す段 | 確かめ方 |
|---|---|---|
| REQ-config-021〜025・032〜037・044、REQ-scan-006・010・012・014・015・016・018・019 | S1 | 規則の単体テスト（生の項目を手で作って渡す。経路を使わない） |
| REQ-scan-017（経路によらない） | S2・S3・S5 | 同じ中身のツリーを三つの経路で走査し、結果が同じことを確かめる契約テスト |
| REQ-scan-002・007、循環 | S1・S5 | 規則の単体テストと三つの経路の契約テスト |
| REQ-scan-009・019、TBL-scan-001（起点によらずフィルタを root_dir からのパスに当てる） | S4 | merge・sync のパス指定の契約テスト |
| REQ-scan-020（diff のディレクトリの展開） | S4 | diff の契約テスト（UTF-8 でない名前を読めない名前として出す） |
| C9 の三つの誤り | S4・S5 | 三つの経路で、root_dir がないとき（全体の走査と部分の走査、merge・sync・--dry-run・status・diff、読み込み元と書き込み先）にエラーで止まり何も書かないこと、起点が読めない・途中がディレクトリでないときに空の一覧にしないことを確かめる契約テスト。root_dir の確認が側ごとに一度だけであること |
| REQ-scan-013（SSH の時間切れ） | S3 | 既存の契約テスト |

## Left to the implementer

- 規則の関数群の分け方と名前、生の項目の型の形
- ローカルとエージェントが共有する集め方の置き場所
- find の刈り込みに使えるパターンの判定の仕方（同じ意味になるものだけを使うこと）
- 通信の型を変えるかどうか（変えるなら通信の版を 11 に上げる。段 1 の記録 B7 は段 3 で 11 に上げるとしているので、ここで上げたら段 3 では 12 にする）

## Stop conditions

- IR の文が二通りに読めるか、ほかの要件と食い違うと分かったとき
- 道具の限界で、三つの経路の結果を揃えられないと分かったとき（揃えられない理由と、確かめた道具の版を添えて返す）
- `remote_merge::cli::*` の `execute_*` の関数の形か出力の型を変える必要があるとき

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

SSH とエージェントの集め方を変えたので、最後に `scripts/run-container-e2e.sh` を走らせる。古い find（findutils 4.2.27）で確かめる手段があれば使う（`bench/legacy-ssh/` を参照）。

## Out of scope

- 子の一覧（`list_children`、REQ-paths-017）の作り直し（段 3）
- パスを辿る手順の SSH の書き直し（段 3）
- 比べる流れ（段 4）

## Steps

### S1: 走査の規則を IR から書く

- Purpose: exclude・include・件数・UTF-8 でない名前・循環・辿るか・読めないときに止めるかを、一つの純粋な関数群にする。
- Specification: docs/ir/scan/listing.md, docs/ir/scan/limits.md, docs/ir/scan/directory-links.md, docs/ir/config/filters.md, docs/ir/config/include.md, docs/decision/records/2026-10-12-foundation-2.md#C6
- Prerequisites: なし
- May change: crates/remote-merge-core/src/
- Done when: 規則の単体テストが、生の項目を手で作る形で、上の要件と例を一つずつ確かめて通る
- Shown by: test — 単体テスト（先に書いて落ちることを確かめてから実装する）
- Left to the implementer: 関数群の分け方
- Stop and hand back if: IR の文が二通りに読めるとき

### S2: ローカルとエージェントの集め方を書き、規則に載せ替える

- Purpose: このマシンの生の項目を集める一つの集め方を、ローカルとエージェントが使う。
- Specification: docs/decision/records/2026-10-12-foundation-2.md#C6, docs/ir/scan/listing.md#REQ-scan-017
- Prerequisites: S1
- May change: crates/remote-merge-core/src/ または crates/remote-merge-engine/src/、crates/remote-merge-agent/src/agent/、crates/remote-merge/src/runtime/
- Done when: ローカルとエージェントの走査が新しい集め方と S1 の規則で動き、古い走査の実装が消え、全テストが通る
- Shown by: test — 全テストと、ローカルとエージェントの結果が同じことを確かめる契約テスト
- Left to the implementer: 置き場所
- Stop and hand back if: 通信の型を変える必要があり、それが段 3 の失敗の型の形を先に決めることになるとき

### S3: SSH の集め方を書き、規則に載せ替える

- Purpose: SSH の走査を、find にリンク先を辿らせずに生の項目を集める形にする（C5・C7）。
- Specification: docs/decision/records/2026-10-12-foundation-2.md#C5, docs/decision/records/2026-10-12-foundation-2.md#C7, docs/remote-tools.md
- Prerequisites: S1
- May change: crates/remote-merge-ssh/src/、crates/remote-merge/src/runtime/
- Done when: SSH の走査が新しい集め方と S1 の規則で動き、古い find の組み立てと診断の読み分けが消え、全テストとコンテナの e2e が通る
- Shown by: test — 全テスト、コンテナの e2e、三つの経路の結果が同じことを確かめる契約テスト
- Left to the implementer: find の書式と、まとめて集める単位
- Stop and hand back if: 道具の限界で結果を揃えられないとき

### S4: 走査を呼ぶ側を一つの入口にする

- Purpose: 全体の走査・部分の走査・diff のディレクトリの展開を、同じ規則と同じ root_dir の確認を通る一つの入口にする。
- Specification: docs/ir/scan/limits.md#REQ-scan-019, docs/ir/scan/listing.md#REQ-scan-020, docs/ir/scan/limits.md#TBL-scan-001, docs/decision/records/2026-10-12-foundation-2.md#C9
- Prerequisites: S2, S3
- May change: crates/remote-merge/src/runtime/、crates/remote-merge/src/cli/、crates/remote-merge-engine/src/
- Done when: REQ-scan-019・020 と C9 の契約テストが三つの経路で通り、部分の走査の前の root_dir の確認（段 1 で足した `refuse_missing_root_dir`）が一つの入口に吸収され、全テストが通る
- Shown by: test — 契約テスト（印 `// @kotowari[REQ-scan-019]` など）。先に落ちることを確かめる
- Left to the implementer: 入口の形
- Stop and hand back if: `execute_*` の形を変える必要があるとき

### S5: 三つの経路の一致を見張るテストを足し、片付ける

- Purpose: 経路の差が新しく入ったら落ちるテストを置く。
- Specification: docs/ir/scan/listing.md#REQ-scan-017
- Prerequisites: S4
- May change: crates/remote-merge/tests/contract/、`docs/ir/` の印の付け替え、crates/remote-merge-agent/src/agent/ の古い印とコメント（REQ-scan-011 を指すもの）
- Done when: 名前に TAB・改行・"\"・UTF-8 でないバイトを含む項目、ディレクトリへの symlink の循環、リンク切れ、exclude の下の読めないディレクトリ、件数の上限ちょうどと一つ超え、を含む一つのツリーを三つの経路で走査し、一覧・不完全の報告・エラーが同じことを確かめる契約テストが通る。`kotowari check` の error が 0
- Shown by: test — 契約テスト
- Left to the implementer: ツリーの作り方
- Stop and hand back if: 道具の限界で揃えられない項目があるとき
