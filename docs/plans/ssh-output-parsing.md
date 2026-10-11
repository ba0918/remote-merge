# Plan: 名前とリンク先を、どの経路でも読んだとおりに一覧に載せる

## Goal

名前やリンク先に TAB・改行・"\" を含む項目が、ローカル・SSH・エージェントのどの経路の走査と子の一覧でも同じ名前・同じリンク先で載り、エージェントの走査はリンク先を読めないときにエラーで止まる。

## Specification

IR は `docs/ir/` にある。決定は `docs/decision/records/2026-10-12-ssh-output-parsing.md` の A1・A2。この計画が扱うのは次のとおり。

- 走査: `docs/ir/scan/listing.md#REQ-scan-006`（例 EX-scan-015・EX-scan-016）、`docs/ir/scan/listing.md#REQ-scan-015`
- 子の一覧: `docs/ir/paths/operations.md#REQ-paths-013`、`docs/ir/paths/operations.md#REQ-paths-017`（例 EX-paths-037）

## Approach and why

SSH の経路の走査（`crates/remote-merge-ssh/src/tree_parser.rs` の `build_find_command` と `parse_find_line`）と子の一覧（`crates/remote-merge-ssh/src/child_listing.rs` の `list_children_command` と `crates/remote-merge-ssh/src/client.rs` の `parse_listing`）は、find の出力を TAB と改行で区切って読む。名前やリンク先にその文字が入ると項目が割れる。symlink のリンク先は `$(readlink ...)` で受けるので、末尾の改行も落ちる。出力を NUL（`\0`）で区切る形に変え、名前とリンク先をバイト列のまま受けて、ほかの経路と同じ規則（REQ-scan-012 の UTF-8 でない名前の扱いを含む）で文字列にする。リンク先は find の `-printf %l` か、シェルの置換を通らない読み方で受ける（`$(...)` は末尾の改行を落とすので使わない）。
SSH の出力をバイト列のまま受け取れるかを先に確かめる。今の `exec` 系が出力を `String` にしているなら、走査と一覧の分だけバイト列で受ける道を足す。

古いサーバの扱い（agent-operations A4 で残すと決めた分岐）を壊さない。新しく使う find の書き方（`-printf` の `\0`、`%l` など）が、今の走査がすでに前提にしている find の機能の範囲に入っているかを確かめ、入らないものを使うなら、今ある古い find 向けの分岐と同じ形で代わりを持つ。ログインシェルの違い（sh・bash・dash・fish）でコマンドが変わらないことは、今あるテストの形で確かめる。
scan-route-parity で足した find の失敗の診断の読み取り（`find_diagnostics.rs`）は標準エラーの話なので、標準出力の区切りを変えても変えない。exclude の刈り込み（`find_prune.rs`）と件数の数え方・上限の判定も変えない。

エージェントの走査（`crates/remote-merge-agent/src/agent/tree_scan.rs`）は、項目の名前の "\" を "/" に替え（202 行目付近）、symlink のリンク先を読めないときに `.ok()` で捨てて空のリンク先にする（225 行目付近）。名前はローカルの走査と同じく読んだとおりにし、リンク先を読めないときは REQ-scan-015 のとおりエラーで止める。止め方はローカルの走査の止め方にそろえる（エージェントの走査の不完全な一覧の報告の形が使えるなら使う）。

## Scope of change

- `crates/remote-merge-ssh/src/tree_parser.rs`、`crates/remote-merge-ssh/src/child_listing.rs`、`crates/remote-merge-ssh/src/client.rs`、`crates/remote-merge-ssh/src/entry_link.rs`（同じ読み方にそろえるときだけ）
- `crates/remote-merge-agent/src/agent/tree_scan.rs`、要るならエージェントの通信の型 `crates/remote-merge-protocol/src/lib.rs`
- 契約テスト `crates/remote-merge/tests/contract/` の中

## Step order and prerequisites

S1（SSH の走査）、S2（SSH の子の一覧）、S3（エージェントの走査）の順にする。S1 と S2 は同じ読み方を使うので、S1 で作った読み方を S2 で使う。S3 は独立している。S4 は最後に全体を確かめる。

## Verification map

| 要件・例 | 示す段 | 確かめ方 |
|---|---|---|
| REQ-scan-006、EX-scan-015 | S1、S3 | 契約テスト: 名前に TAB・改行・"\" を含む三つのファイルが、三つの経路の status の一覧に同じ名前で一件ずつ載る |
| EX-scan-016 | S1 | 契約テスト: リンク先に TAB・末尾の改行・"\" を含む三つの symlink が、三つの経路の走査で読んだとおりのリンク先で載る |
| REQ-paths-013、REQ-paths-017、EX-paths-037 | S2 | 契約テスト: 名前に TAB と改行を含む子が、三つの経路の子の一覧に同じ名前で含まれる |
| REQ-scan-015 | S3 | エージェントの経路で symlink のリンク先を読めない状態を作れるなら契約テストで、作れなければその判定の純粋関数の単体テストで、エラーで止まることを確かめる（どちらを選んだかを報告する） |

## Left to the implementer

- NUL 区切りの出力の列の並びと、読み取りの関数の形
- SSH の出力をバイト列で受け取る道の作り方
- エージェントの走査でリンク先を読めないときの止め方の内部の形（通信の型を変えるなら版を上げる。今は 8）
- 契約テストの置き場所と名前

## Stop conditions

- 新しい読み方が、今の走査が前提にしていない find の機能を要り、古い find 向けの代わりを作れないとき
- 名前に改行を含むファイルを、ローカルの経路の status や diff の出力が今の形では表せないと分かったとき（出力の形の決定が要る）
- エージェントの走査でリンク先を読めないときに止めると、ローカルと SSH と違う結果になる形でしか作れないとき

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

SSH とエージェントの読み方を変えるので、最後に `scripts/run-container-e2e.sh` を一度走らせて結果を確かめる（Docker が要る）。

## Out of scope

- UTF-8 でない名前の扱い（REQ-scan-012。変えない）
- exclude の刈り込みと find の失敗の診断の読み取り（scan-route-parity で決めた形を変えない）
- 件数の上限の判定の経路差（品質の台帳の sr-1）

## Steps

### S1: SSH の走査が名前とリンク先を読んだとおりに載せる

- Purpose: SSH の走査の出力を NUL で区切って読み、名前とリンク先に TAB・改行・"\" を含む項目を読んだとおりに載せる。
- Specification: docs/ir/scan/listing.md#REQ-scan-006
- Prerequisites: なし
- May change: crates/remote-merge-ssh/src/、crates/remote-merge/src/runtime/、crates/remote-merge/tests/contract/ の中
- Done when: EX-scan-015 と EX-scan-016 の SSH の経路の契約テストが通り、全テストが通る
- Shown by: test — EX-scan-015・EX-scan-016 の契約テスト（印 `// @kotowari[REQ-scan-006, EX-scan-01n]`）。SSH の経路の RED を確かめてから直す
- Left to the implementer: 列の並び、バイト列で受け取る道の作り方
- Stop and hand back if: 古い find 向けの代わりを作れない機能が要るとき

### S2: SSH の子の一覧が名前とリンク先を読んだとおりに返す

- Purpose: SSH の子の一覧も S1 と同じ読み方にし、名前に TAB と改行を含む子をほかの経路と同じ名前で返す。
- Specification: docs/ir/paths/operations.md#REQ-paths-013, docs/ir/paths/operations.md#REQ-paths-017
- Prerequisites: S1
- May change: crates/remote-merge-ssh/src/、crates/remote-merge/src/runtime/、crates/remote-merge/tests/contract/ の中
- Done when: EX-paths-037 の契約テストが三つの経路で通り、全テストが通る
- Shown by: test — EX-paths-037 の契約テスト（印 `// @kotowari[REQ-paths-017, EX-paths-037]`）。SSH の経路の RED を確かめてから直す
- Left to the implementer: 読み取りの関数の形
- Stop and hand back if: 子の一覧を使う diff・merge・rollback の結果が、走査の一覧と食い違うと分かったとき

### S3: エージェントの走査が名前を読んだとおりに載せ、リンク先を読めなければ止まる

- Purpose: エージェントの走査で名前の "\" を替えず、リンク先を読めないときは空で埋めずにエラーで止める。
- Specification: docs/ir/scan/listing.md#REQ-scan-006, docs/ir/scan/listing.md#REQ-scan-015
- Prerequisites: なし
- May change: crates/remote-merge-agent/src/agent/、crates/remote-merge-protocol/src/lib.rs、crates/remote-merge/src/runtime/、crates/remote-merge/tests/contract/ の中
- Done when: EX-scan-015 のエージェントの経路の契約テストが通り、リンク先を読めないときに止まることがテストで確かめられ、全テストが通る
- Shown by: test — EX-scan-015 のエージェントの経路の契約テスト（S1 と同じテストの経路を広げてよい）と、REQ-scan-015 のテスト（印 `// @kotowari[REQ-scan-015]`）。RED を確かめてから直す
- Left to the implementer: 止め方の内部の形、通信の型を変えるか
- Stop and hand back if: ローカルと SSH と違う結果になる形でしか作れないとき

### S4: 計画の要件と例に印が付き、計画の範囲に kotowari check のエラーがない

- Purpose: 計画の要件と例のすべてに印の付いたテストがあることを確かめ、Docker の e2e を一度走らせる。
- Specification: docs/ir/scan/listing.md#REQ-scan-006, docs/ir/paths/operations.md#REQ-paths-017
- Prerequisites: S1、S2、S3
- May change: 印の付け直しに要るテストのファイルだけ
- Done when: `kotowari query` で EX-scan-015・EX-scan-016・EX-paths-037 の tests が空でなく、このブランチが変えたファイルと計画の ID に `kotowari check` のエラーがなく、`scripts/run-container-e2e.sh` が通る
- Shown by: check — `kotowari query <ID> | jq '.items[0].tests'` を各 ID について、`kotowari check --format json`、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features`、`scripts/run-container-e2e.sh`
- Left to the implementer: none
- Stop and hand back if: 計画の範囲の ID の findings が消えないとき
