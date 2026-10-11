# Plan: 権限の写しと作ったディレクトリの権限の失敗を知らせ、rollback --dry-run の失敗を終了コードに出す

## Goal

`--with-permissions` の merge と sync が、読み込み元がローカルでもリモートでも走査した権限（0 を含む）を写し、写せないときと、どの経路でも作ったディレクトリの権限を付けられないときは、そのファイルを中身は書いたと分かる error で failed に出す。`rollback --dry-run` は failed があれば終了コード 2 を返す。

## Specification

IR は `docs/ir/` にある。決定は `docs/decision/records/2026-10-11-permission-copy.md` の A1〜A4。この計画が扱うのは次のとおり。

- 権限の写し: `docs/ir/merge/permissions.md#REQ-merge-014`（例 EX-merge-056）、`docs/ir/merge/permissions.md#REQ-merge-036`（例 EX-merge-043）、`docs/ir/merge/permissions.md#REQ-merge-037`
- 作ったディレクトリの権限: `docs/ir/merge/permissions.md#REQ-merge-012`
- rollback --dry-run の終了コード: `docs/ir/backup/rollback-cli.md#REQ-backup-038`（例 EX-backup-031）

## Approach and why

三つの段は互いに依存しない。どれも「失敗や未対応を成功に見せる」件なので、先に失敗を見せる契約テストを書いて RED を確かめてから直す。

権限の写し（S1）。`crates/remote-merge/src/service/merge_flow.rs` の `copy_permissions` と `source_permissions` は、読み込み元がローカルなら `std::fs::metadata` で読み、リモートなら `None` を返して何もしない。0 も写さない。REQ-merge-036 は、読み込み元の権限にローカルとリモートのどちらでも「走査したツリーの値」を使うと決めているので、`MergeContext` が持つ読み込み元のツリーのノードの権限を使う形にする。ツリーのノードに権限の値がないときは、REQ-merge-037 のとおり中身は書いたと分かる error で failed にする（今ある `content_written` の扱いにそろえる）。0 はそのまま写す。setuid・setgid・sticky を含むときの今の扱い（EX-merge-051）は変えない。ローカルの読み込み元も走査したツリーの値に替わるので、読み込みと権限の読み取りの間の変化の扱い（以前 FLAG-merge-007 にあった件）は、走査の値に揃う。
新しく作るファイルに読み込み元の権限を付ける REQ-merge-036 の経路（`target_tree` にないときの処理）も、同じ値を使う。

作ったディレクトリの権限（S2）。エージェントの書き込みは、`crates/remote-merge-agent/src/agent/file_io.rs` の `apply_dir_permissions` で作ったディレクトリの権限を付け、失敗を debug のログだけにして成功を返す。一方、`merge_flow.rs` は書く前に `missing_parent_directories` で作られるディレクトリを求め、書いた後にどの経路でも `chmod_file` で設定の権限を付け、失敗を `content_written` で failed にしている。エージェントの経路の失敗が、ほかの経路と同じく failed に出るようにする。エージェントの内側の権限付けを、失敗を返す形にするか、`merge_flow.rs` の後からの権限付けに任せて内側をやめるかは実装者に任せる。どちらでも、作ったディレクトリの権限と setuid・setgid を残す扱い（REQ-merge-012、EX-merge-052）は変えない。
エージェントの書き込み要求の応答の形を変えるなら、通信の版を上げる（今は 8、同じ版だけを受け入れる）。

rollback --dry-run（S3）。`crates/remote-merge/src/cli/rollback.rs` は、dry-run のとき `has_stopped` でなければ 0 にしている。dry-run でも failed があれば 2 にする（`rollback_exit_code` と同じ判定のうち failed の部分。dry-run の「戻したファイルがない」は 2 にしない）。

## Scope of change

- `crates/remote-merge/src/service/merge_flow.rs`（権限の写し、作ったディレクトリ）
- `crates/remote-merge-agent/src/agent/file_io.rs` と、要るならエージェントの通信の型 `crates/remote-merge-protocol/src/lib.rs`、`crates/remote-merge/src/runtime/`
- `crates/remote-merge/src/cli/rollback.rs`
- 契約テスト `crates/remote-merge/tests/contract/` の中

## Step order and prerequisites

S1・S2・S3 は互いに依存しないので、この順に一つずつコミットする。S4 は最後に全体を確かめる。

## Verification map

| 要件・例 | 示す段 | 確かめ方 |
|---|---|---|
| REQ-merge-036、EX-merge-043 | S1 | 契約テスト: リモートの読み込み元の 0o600 のファイルを --with-permissions で新しく作ると、書き込み先の権限が 0o600 |
| REQ-merge-014、EX-merge-056 | S1 | 契約テスト: 読み込み元の権限 0 のファイルを --with-permissions で merge すると、書き込み先の権限が 0。EX-merge-056 のテストは `tests/container-e2e/` の独立ワークスペースにあり、通常の cargo nextest では走らないので、`scripts/run-container-e2e.sh` で走らせる（Docker が要るので、人が走らせて結果を確かめる） |
| REQ-merge-037 | S1 | 走査したツリーに権限の値がないときに failed になることは、その状態を作る経路があれば契約テストで、なければ純粋関数の単体テストで確かめる（どちらを選んだかを報告する） |
| REQ-merge-012 | S2 | 契約テスト: エージェントの経路で、作るディレクトリの権限の変更が失敗する状態で merge すると、そのファイルが中身は書いたと分かる error で failed になり、ローカルと SSH の経路と同じ結果になる |
| REQ-backup-038、EX-backup-031 | S3 | 契約テスト: failed の出る rollback --dry-run が終了コード 2 を返し、failed のない --dry-run は 0 のまま |

## Left to the implementer

- 読み込み元のツリーの権限の取り出し方と、関数の形
- エージェントの内側の権限付けを失敗を返す形にするか、`merge_flow.rs` の後からの権限付けに任せるか
- 契約テストの置き場所と名前、失敗を起こす方法（権限を変えられない状態の作り方）

## Stop conditions

- 走査したツリーにリモートの権限の値が入っていない経路があると分かり、それを入れるために走査の出力の形を変える必要があるとき
- エージェントの経路で作ったディレクトリの権限の失敗を起こす方法が、契約テストで作れず、ほかの経路と同じであることを確かめられないとき
- rollback --dry-run の failed が、実行時には failed にならないもの（実行時には書き戻せるもの）まで含んでいると分かったとき

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

Docker の実機の例（EX-merge-056）は `scripts/run-container-e2e.sh` で走らせる（Docker が要るので、人が走らせて結果を確かめる）。

## Out of scope

- --with-permissions で書き込み先の既存のファイルの所有者を写すこと（IR にない）
- merge と sync の --dry-run の終了コード（REQ-cli-091 で決まっていて、変えない）
- SSH の出力の読み方の件（品質の台帳の ao-2）

## Steps

### S1: --with-permissions が走査した読み込み元の権限を 0 も含めて写し、分からなければ failed にする

- Purpose: 読み込み元がローカルでもリモートでも走査したツリーの権限を写し、0 も写し、値がないときは failed にする。
- Specification: docs/ir/merge/permissions.md#REQ-merge-036, docs/ir/merge/permissions.md#REQ-merge-014, docs/ir/merge/permissions.md#REQ-merge-037
- Prerequisites: なし
- May change: crates/remote-merge/src/service/merge_flow.rs、crates/remote-merge/src/service/permission_copy.rs、crates/remote-merge/src/cli/merge.rs、crates/remote-merge/src/cli/sync.rs、crates/remote-merge/tests/contract/ の中、tests/container-e2e/tests/sudo.rs
- Done when: EX-merge-043 と EX-merge-056 の契約テストが通り、値がないときの failed がテストで確かめられ、全テストが通る
- Shown by: test — EX-merge-043・EX-merge-056 の契約テスト（印 `// @kotowari[REQ-merge-036, EX-merge-043]`、`// @kotowari[REQ-merge-014, EX-merge-056]`）。RED を確かめてから直す
- Left to the implementer: 権限の取り出し方、値がないときのテストを契約テストにするか単体テストにするか
- Stop and hand back if: 走査したツリーにリモートの権限の値が入っていない経路があるとき

### S2: エージェントの経路でも作ったディレクトリの権限の失敗を failed に出す

- Purpose: 作ったディレクトリの権限の変更の失敗を、エージェントの経路でもローカルと SSH と同じく failed に出す。
- Specification: docs/ir/merge/permissions.md#REQ-merge-012
- Prerequisites: なし
- May change: crates/remote-merge-agent/src/agent/file_io.rs、crates/remote-merge-agent/src/agent/、crates/remote-merge-protocol/src/lib.rs、crates/remote-merge/src/runtime/、crates/remote-merge/src/service/merge_flow.rs、crates/remote-merge/tests/contract/ の中
- Done when: エージェントの経路で作ったディレクトリの権限の変更が失敗する契約テストが、ほかの経路と同じ failed の結果で通り、EX-merge-052 のテストも通り、全テストが通る
- Shown by: test — 三つの経路の契約テスト（印 `// @kotowari[REQ-merge-012]`）。エージェントの経路の RED を確かめてから直す
- Left to the implementer: 内側の権限付けの扱い、通信の型を変えるか
- Stop and hand back if: 失敗を起こす方法が契約テストで作れないとき

### S3: rollback --dry-run が failed があれば終了コード 2 を返す

- Purpose: rollback の --dry-run も、merge と sync と同じく failed があれば 2 を返す。
- Specification: docs/ir/backup/rollback-cli.md#REQ-backup-038
- Prerequisites: なし
- May change: crates/remote-merge/src/cli/rollback.rs、crates/remote-merge/tests/contract/ の中
- Done when: EX-backup-031 の契約テストが通り、failed のない --dry-run が 0 のままのテストも通り、全テストが通る
- Shown by: test — EX-backup-031 の契約テスト（印 `// @kotowari[REQ-backup-038, EX-backup-031]`）。RED を確かめてから直す
- Left to the implementer: failed を起こす方法
- Stop and hand back if: dry-run の failed が、実行時には書き戻せるものまで含んでいると分かったとき

### S4: 計画の要件と例に印が付き、計画の範囲に kotowari check のエラーがない

- Purpose: 計画の要件と例のすべてに印の付いたテストがあることを確かめる。
- Specification: docs/ir/merge/permissions.md#REQ-merge-036, docs/ir/merge/permissions.md#REQ-merge-014, docs/ir/backup/rollback-cli.md#REQ-backup-038
- Prerequisites: S1、S2、S3
- May change: 印の付け直しに要るテストのファイルだけ
- Done when: `kotowari query` で REQ-merge-036・EX-merge-043・EX-merge-056・EX-backup-031 の tests が空でなく、このブランチが変えたファイルと計画の ID に `kotowari check` のエラーがない
- Shown by: check — `kotowari query <ID> | jq '.items[0].tests'` を各 ID について、`kotowari check --format json`、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features`、`scripts/run-container-e2e.sh`（EX-merge-056 のテストを走らせる。Docker が要るので、人が走らせて結果を確かめる）
- Left to the implementer: none
- Stop and hand back if: 計画の範囲の ID の findings が消えないとき
