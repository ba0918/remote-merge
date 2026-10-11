# Plan: 走査と子の一覧の exclude と symlink の扱いを、三つの経路でそろえる

## Goal

同じ設定と同じファイルの状態で、ローカル・SSH・エージェントのどの経路でも、走査の一覧と子の一覧が同じものを含む。"/" を含む exclude に当たったディレクトリの子孫はどの経路の走査でも除かれ、エージェントの走査もディレクトリ symlink の配下を辿り、子の一覧の exclude はどの経路でも走査と同じ規則で当たる。

## Specification

IR は `docs/ir/` にある。決定は `docs/decision/records/2026-10-11-scan-route-parity.md` の A1（子の一覧の exclude）と、承認済みの要件のもとになった `docs/decision/records/2026-10-05-quality-config.md` の A7、`docs/decision/records/2026-10-05-quality-scan.md` の A2。この計画が扱うのは次のとおり。

- "/" を含む exclude の子孫をどの経路でも除く: `docs/ir/config/filters.md#REQ-config-022`（例 EX-config-009）
- エージェントの走査もディレクトリ symlink を辿る: `docs/ir/scan/directory-links.md#REQ-scan-011`、`docs/ir/scan/directory-links.md#REQ-scan-002`（例 EX-scan-003・EX-scan-004・EX-scan-005）
- 子の一覧の exclude: `docs/ir/paths/operations.md#REQ-paths-017`（例 EX-paths-036）、`docs/ir/paths/operations.md#REQ-paths-013`

## Approach and why

三つとも「同じ入力で経路によって一覧が違う」件なので、どの段も、三つの経路で同じ状態を作って一覧を比べる契約テストを先に書き、RED を確かめてから直す。基準はローカルの経路の今の振る舞いである（ローカルは `crates/remote-merge-engine/src/local/mod.rs` の走査が `follow_links(true)` で辿り、`filter::is_path_excluded` で子孫ごと除いている）。

SSH の走査の exclude（S1）。調べでは、SSH は find の出力を一行ずつ `parse_find_line`（`crates/remote-merge-ssh/` の木の読み込み）で読み、行ごとに exclude を当てるだけなので、"a/b" に当たったディレクトリの下の "a/b/c.txt" は "a/b/c.txt" そのものが "a/b" に当たらず残る。ローカルと同じく、当たったディレクトリの子孫を除く。find のコマンドで刈るか、読んだ後に祖先で判定するかは実装者に任せるが、走査件数の数え方（REQ-scan-010）と、件数上限の超過の報告は変えない。

エージェントの走査の symlink（S2）。`crates/remote-merge-agent/src/agent/tree_scan.rs` は、ディレクトリを指す symlink の配下を辿らない。ローカルと SSH と同じく配下を列挙し、循環をデバイス番号と inode 番号で検出して不完全な一覧として報告する（REQ-scan-011）。エージェントの通信の型（`crates/remote-merge-protocol/`）を変える必要が出たら、通信の版を上げる。版は今 6 で、手元と同じ版だけを受け入れる（agent-operations A2・A18）。版を上げるだけなら IR は変わらない。
symlink の配下の項目のパスは、ローカルの経路と同じく symlink を通ったパス（リンク先の実パスではない）で載せる。include の値のディレクトリ symlink の扱い（include-values で決めた REQ-config-024 まわり）は変えない。

SSH の子の一覧の exclude（S3）。`crates/remote-merge/src/runtime/remote_io.rs` の `list_remote_children_excluding` と `fetch_remote_children` の exclude の当て方を、`crates/remote-merge-core/src/listing.rs` の `list_children_with`（ローカルとエージェントが使う）と同じく、直下の子の root_dir からの相対パスに `filter::is_path_excluded` で当てる形にする。名前に TAB や改行を含む子の読み違い（FLAG-paths-001）は別の単位なので触らない。

## Scope of change

- `crates/remote-merge-ssh/`（find の出力の読み込みと、走査のコマンド）、必要なら `crates/remote-merge/src/runtime/remote_io.rs`
- `crates/remote-merge-agent/src/agent/tree_scan.rs` と、要るならエージェントの通信の型 `crates/remote-merge-protocol/src/lib.rs`
- `crates/remote-merge/src/runtime/remote_io.rs`（子の一覧）
- 契約テスト `crates/remote-merge/tests/contract/` の中（走査と子の一覧の三経路のテスト。今ある `config_filters.rs` や、エージェントと SSH の偽サーバを使う既存のテストの形にそろえる）

## Step order and prerequisites

S1・S2・S3 は互いに依存しないが、この順に一つずつコミットする。S1 と S3 はどちらも exclude の当て方で、S1 の判定の関数を S3 で使えることがある。S4 は最後に全体を確かめる。

## Verification map

| 要件・例 | 示す段 | 確かめ方 |
|---|---|---|
| REQ-config-022、EX-config-009 | S1 | 契約テスト: exclude "a/b"、"a/b/c.txt" がある状態で、ローカル・SSH・エージェントの status の一覧のどれにも "a/b/c.txt" がない |
| REQ-scan-011、REQ-scan-002、EX-scan-003 | S2 | 契約テスト: ディレクトリ symlink の先の複数のファイルが、三つの経路の status の一覧に同じパスで出る |
| EX-scan-004 | S2 | 契約テスト: 祖先へ戻る symlink で、三つの経路とも不完全な一覧として報告される |
| EX-scan-005 | S2 | 契約テスト: リンク先を含めて上限を超えると、三つの経路とも上限の超過が報告される |
| REQ-paths-017、REQ-paths-013、EX-paths-036 | S3 | 契約テスト: exclude "vendor/**"、"vendor/x.txt" と "a/vendor/y.txt" がある状態で、三つの経路の "a" の子の一覧に "a/vendor" が含まれる |

## Left to the implementer

- SSH の走査で子孫を除く方法（find で刈るか、読んだ後に祖先で判定するか）。どちらでも、除かれるパスの集合と、件数の数え方と上限の報告は同じ
- エージェントの循環の検出の作り方と、通信の型を変えるか
- 契約テストの置き場所と名前、偽サーバの使い方

## Stop conditions

- 三つの経路で同じ結果にするために、REQ-scan-010（件数の数え方）や REQ-scan-004・REQ-scan-008（不完全な一覧と上限の報告）の振る舞いを変える必要があるとき
- エージェントの走査で symlink を辿ると、root_dir の外を指す symlink の配下の扱いが、ローカルと SSH の経路と違う形でしか作れないとき
- 子の一覧の exclude をそろえると、子の一覧を使う diff・merge・rollback の結果が、走査の一覧と食い違うと分かったとき

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

## Out of scope

- 名前に TAB や改行を含む子の読み違い（FLAG-paths-001、品質の台帳の ao-2）
- 祖先の名前が "/" を含む exclude に当たる深い include の値の扱い（品質の台帳の iv-6、まだ測っていない）
- ローカルの走査が include の起点のディレクトリを max_entries に数える件（iv-5）

## Steps

### S1: SSH の走査が "/" を含む exclude に当たったディレクトリの子孫を除く

- Purpose: "/" を含む exclude に当たったディレクトリの下のパスを、SSH の走査もローカルとエージェントと同じく除く。
- Specification: docs/ir/config/filters.md#REQ-config-022
- Prerequisites: なし
- May change: crates/remote-merge-ssh/、crates/remote-merge/src/runtime/remote_io.rs、crates/remote-merge/tests/contract/ の中
- Done when: EX-config-009 の契約テストが三つの経路で通り、全テストが通る
- Shown by: test — EX-config-009 の契約テスト（印 `// @kotowari[REQ-config-022, EX-config-009]`）。RED を確かめてから直す
- Left to the implementer: 子孫を除く方法、テストの名前
- Stop and hand back if: 件数の数え方か上限の報告を変える必要があるとき

### S2: エージェントの走査がディレクトリ symlink の配下を辿る

- Purpose: エージェントの走査も、ディレクトリを指す symlink の配下を列挙し、循環と上限の超過を不完全な一覧として報告する。
- Specification: docs/ir/scan/directory-links.md#REQ-scan-011, docs/ir/scan/directory-links.md#REQ-scan-002
- Prerequisites: なし
- May change: crates/remote-merge-agent/src/agent/、crates/remote-merge-protocol/src/lib.rs、crates/remote-merge/src/runtime/、crates/remote-merge/tests/contract/ の中
- Done when: EX-scan-003・EX-scan-004・EX-scan-005 の契約テストが三つの経路で通り、全テストが通る
- Shown by: test — EX-scan-003・EX-scan-004・EX-scan-005 の契約テスト（印 `// @kotowari[REQ-scan-011, REQ-scan-002, EX-scan-00n]`）。エージェントの経路の RED を確かめてから直す
- Left to the implementer: 循環の検出の作り方、通信の型を変えるか
- Stop and hand back if: root_dir の外を指す symlink の配下の扱いが、ローカルと SSH と違う形でしか作れないとき

### S3: SSH の子の一覧の exclude を走査と同じ規則で当てる

- Purpose: SSH の経路の子の一覧も、exclude を直下の子の root_dir からの相対パスに、走査と同じ規則で当てる。
- Specification: docs/ir/paths/operations.md#REQ-paths-017, docs/ir/paths/operations.md#REQ-paths-013
- Prerequisites: なし
- May change: crates/remote-merge/src/runtime/remote_io.rs、crates/remote-merge-ssh/、crates/remote-merge/tests/contract/ の中
- Done when: EX-paths-036 の契約テストが三つの経路で通り、全テストが通る
- Shown by: test — EX-paths-036 の契約テスト（印 `// @kotowari[REQ-paths-017, EX-paths-036]`）。SSH の経路の RED を確かめてから直す
- Left to the implementer: テストの置き場所と名前
- Stop and hand back if: 子の一覧を使う diff・merge・rollback の結果が走査の一覧と食い違うと分かったとき

### S4: 計画の要件と例に印が付き、計画の範囲に kotowari check のエラーがない

- Purpose: 計画の要件と例のすべてに印の付いたテストがあることを確かめる。
- Specification: docs/ir/config/filters.md#REQ-config-022, docs/ir/scan/directory-links.md#REQ-scan-011, docs/ir/scan/directory-links.md#REQ-scan-002, docs/ir/paths/operations.md#REQ-paths-017
- Prerequisites: S1、S2、S3
- May change: 印の付け直しに要るテストのファイルだけ
- Done when: `kotowari query` で REQ-scan-011・REQ-scan-002・EX-config-009・EX-scan-003・EX-scan-004・EX-scan-005・EX-paths-036 の tests が空でなく、このブランチが変えたファイルと計画の ID に `kotowari check` のエラーがない
- Shown by: check — `kotowari query <ID> | jq '.items[0].tests'` を各 ID について、`kotowari check --format json`、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: 計画の範囲の ID の findings が消えないとき
