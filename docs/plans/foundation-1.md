# Plan: root_dir からの相対パスを一つの型にし、経路ごとのパスの検査をなくす

## Goal

root_dir からの相対パスは、入力の形を検査して作る一つの型（以下 RelPath）だけで受け渡される。検査はコマンドの入口とエージェントの受け取り口の二か所で、同じ関数が行う。経路の操作（ローカル・SSH・エージェント）は root_dir の外かどうかを自分で判定せず、エージェントだけが拒んでいた読み書きと symlink の作成をほかの経路と同じに行う。

## Specification

- 方針: `docs/decision/records/2026-10-12-foundation.md`（A1〜A9）と `docs/decision/records/2026-10-12-foundation-1.md`（B1〜B4）
- 入力の形: `docs/ir/cli/diff-output.md#REQ-cli-078`、`docs/ir/merge/symlink.md#REQ-merge-007`、`docs/ir/cli/directory-paths.md#REQ-cli-025`
- パスの確認: `docs/ir/paths/resolution.md`（REQ-paths-001〜007）、`docs/ir/paths/routes.md`（REQ-paths-008・011）
- 経路の結果をそろえる: `docs/ir/merge/symlink.md#REQ-merge-004`、`#REQ-merge-023`、`docs/ir/merge/FLAGS.md#FLAG-merge-012`

## Approach and why

**今の実装を参考にしない（foundation A1）。** RelPath とその検査の関数は、上の IR だけから書く。今のコードにある同じ目的の関数（`check_rel_path`、`validate_path_within_root`、エージェントの `validate_path`・`validate_entry_path` の root の判定、`fetch_tree_for_subpath` の中の手書きの検査、`path_resolver::check_path_traversal` など）は、中身を読んで写さない。呼び出し側を新しい型に替えて、古い関数は消す。読んでよいのは、差し替える呼び出し側の関数の形と、テストである。

型で受け取ることで、経路の操作から入力の形の検査を外せる。外したあと、経路に残すのは IR が求める「パスを辿る」処理（パスの確認の関数）だけにする（B2）。

## Scope of change

- `crates/remote-merge-core/`（RelPath の置き場所。今の `rel_path.rs` は置き換える）
- RelPath を受け取るように変える呼び出し側: `crates/remote-merge/src/runtime/`、`crates/remote-merge/src/cli/`、`crates/remote-merge/src/service/`、`crates/remote-merge-engine/src/`、`crates/remote-merge-agent/src/agent/`、`crates/remote-merge-ssh/src/`
- テスト: `crates/remote-merge/tests/contract/` に足す。既存のテストは、関数の形の変更に合わせる以外は変えない（foundation A6）
- `docs/ir/merge/FLAGS.md` から FLAG-merge-012 を消す

## Step order and prerequisites

S1 → S2 → S3 → S4 の順。S2 以降は S1 の型を使う。

## Verification map

| 要件・例 | 示す段 | 確かめ方 |
|---|---|---|
| REQ-cli-078、REQ-merge-007、REQ-cli-025 | S1 | RelPath の単体テスト（IR の文言どおりのエラー、"/" と末尾の "/" の扱い）と、今の契約テストがそのまま通ること |
| B1・B2（経路が検査しない） | S2・S3 | 経路の操作が RelPath だけを受け取ること（型で示す）と、全テストが通ること |
| REQ-merge-004、REQ-merge-023、FLAG-merge-012 の解消 | S4 | エージェントの経路で、root_dir の外の共有ディレクトリへの途中の symlink 越しの読み書きと、".." と絶対パスのリンク先の symlink の作成が、ローカルと同じ結果になる契約テスト |

## Left to the implementer

- RelPath の内部表現、モジュールの名前、エラーの型の形（ただし IR の文言を出すこと）
- 通信の型の変更が要るかどうか。要るなら通信の版を 10 に上げる（foundation A9）

## Stop conditions

- IR の文が二通りに読めるか、ほかの要件と食い違うと分かったとき
- 今のテストが、IR と違う動きを固めていて、IR どおりにすると落ちるとき（直さずに、テスト名と IR の要件を添えて差し戻す）
- RelPath にすると、`remote_merge::cli::*` の `execute_*` の関数の形か出力の型を変える必要があるとき

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

エージェントか SSH の経路を変えたら、最後に `scripts/run-container-e2e.sh` を一度走らせる（Docker が要る）。

## Out of scope

- パスを辿る手順そのものの作り直し（B4。段 3）
- include の値の検査と正規化（段 2 の走査の規則で扱う）
- 操作の失敗の型（foundation A2。段 3 で経路の操作と一緒に入れる）

## Steps

### S1: RelPath と入力の形の検査を IR から書く

- Purpose: root_dir からの相対パスを、検査を通ったときだけ作れる型にする。
- Specification: docs/ir/cli/diff-output.md#REQ-cli-078, docs/ir/merge/symlink.md#REQ-merge-007, docs/ir/cli/directory-paths.md#REQ-cli-025
- Prerequisites: なし
- May change: crates/remote-merge-core/src/
- Done when: 絶対パス・".." の要素・"/"（root_dir 全体）・末尾の "/"・"./" の形について、IR の文言どおりの結果を返す単体テストが通る
- Shown by: test — 単体テスト（先に書いて落ちることを確かめてから実装する）
- Left to the implementer: 型の内部表現
- Stop and hand back if: IR の文言が diff と merge で違い、どちらに合わせるか決まらないとき

### S2: コマンドの入口とエージェントの受け取り口を RelPath にする

- Purpose: 入力の形の検査を二か所に集める。
- Specification: docs/decision/records/2026-10-12-foundation-1.md#B1
- Prerequisites: S1
- May change: crates/remote-merge/src/cli/、crates/remote-merge/src/service/、crates/remote-merge-engine/src/service/、crates/remote-merge-agent/src/agent/
- Done when: diff・merge・sync・status・rollback の引数と、エージェントが通信で受け取るパスが S1 の関数で検査され、全テストが通る
- Shown by: test — 全テスト
- Left to the implementer: エラーの伝え方の内部の形
- Stop and hand back if: `execute_*` の関数の形を変える必要があるとき

### S3: 経路の操作を RelPath で受け取り、経路ごとの検査を消す

- Purpose: 経路から入力の形と root の外の判定をなくす（B1・B2）。
- Specification: docs/decision/records/2026-10-12-foundation-1.md#B1, docs/decision/records/2026-10-12-foundation-1.md#B2
- Prerequisites: S2
- May change: crates/remote-merge/src/runtime/、crates/remote-merge-engine/src/、crates/remote-merge-agent/src/agent/、crates/remote-merge-ssh/src/
- Done when: TargetIo とローカル・SSH・エージェントの操作が RelPath を受け取り、古い検査の関数（上の Approach に挙げたもの）が消え、全テストが通る
- Shown by: test — 全テスト。消した関数の名前を報告に並べる
- Left to the implementer: 消す順番
- Stop and hand back if: 消すと IR のどれかの要件が満たせなくなるとき

### S4: エージェントの拒否をなくし、FLAG-merge-012 を閉じる

- Purpose: エージェントの経路をローカル・SSH と同じ結果にする（B3）。
- Specification: docs/ir/merge/symlink.md#REQ-merge-004, docs/ir/merge/symlink.md#REQ-merge-023, docs/decision/records/2026-10-12-foundation-1.md#B3
- Prerequisites: S3
- May change: crates/remote-merge-agent/src/agent/、crates/remote-merge/tests/contract/、docs/ir/merge/FLAGS.md
- Done when: 契約テストで、エージェントの経路でも root_dir の外の共有ディレクトリへの途中の symlink 越しに読み書きでき、".." と絶対パスのリンク先の symlink を作れ、ローカルと同じ結果になる。FLAG-merge-012 が消え、`kotowari check` の error が 0
- Shown by: test — 契約テスト（印 `// @kotowari[REQ-merge-004]`・`// @kotowari[REQ-merge-023]`）。先に落ちることを確かめる
- Left to the implementer: テストの置き方
- Stop and hand back if: sudo のサーバでの書き込みに、別の要件が拒否を求めているとき
