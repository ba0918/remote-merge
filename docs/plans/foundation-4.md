# Plan: 比べる流れを一つにする

## Goal

diff・status・merge・sync・rollback が通る比べる流れ（両側の状態 → 判定 → 必要な分だけ読む → もう一度判定）が、一つの純粋な判定の関数群と、段 2 の走査・段 3 の経路の上で状態とハッシュと中身を集める一つの組み立てとして存在する。判定は経路・コマンド・パスの指定の仕方によらず同じ値を返し、読み込みの失敗はどの種類も判定できないとして扱われ、ないことは状態からだけ決まる。走査は FIFO などをその他の種類として載せ、中身の読み込みは通常ファイルでないものを待たずに失敗にし、子の一覧は走査と同じ件数の上限で数えて打ち切りを知らせる。流れを直接呼ぶ契約テストが、段 4 で示す要件と例（下の Verification map）を三つの経路で示し、`kotowari check` の error は、段 5 で CLI の出力で示す要件と例のテストがまだないことによるものだけになる。

## Specification

- 方針: `docs/decision/records/2026-10-12-foundation.md`（A1〜A10）、`docs/decision/records/2026-10-12-foundation-4.md`（E1〜E20、U1〜U6 は利用者に返している）
- 段 2・段 3 の記録: `docs/decision/records/2026-10-12-foundation-2.md`（C6・C11・C15）、`docs/decision/records/2026-10-12-foundation-3.md`（D1・D2・D6・D7・D29）
- 比べる流れ: `docs/ir/compare/flow.md`（REQ-compare-001〜005）、`docs/ir/compare/judgement.md`（REQ-compare-006〜009、TBL-compare-001〜003）、`docs/ir/compare/reading.md`（REQ-compare-010〜015、TBL-compare-004・005）
- 走査と子の一覧: `docs/ir/scan/listing.md`（REQ-scan-021・REQ-scan-012）、`docs/ir/scan/limits.md`（REQ-scan-010・REQ-scan-004・REQ-scan-008）、`docs/ir/cli/symlink-diff.md`（REQ-cli-021）
- 操作: `docs/ir/paths/operations.md`（REQ-paths-021・REQ-paths-020）
- 流れを引くコマンドの要件（段 5 で CLI の出力として示す）: `docs/ir/cli/status.md`、`docs/ir/cli/unreadable.md`、`docs/ir/cli/size-limit.md`、`docs/ir/cli/diff-strategy.md`、`docs/ir/cli/diff-output.md`、`docs/ir/cli/symlink-diff.md`、`docs/ir/cli/conflicts.md`、`docs/ir/cli/merge.md`、`docs/ir/merge/comparison.md`、`docs/ir/merge/read-failure.md`、`docs/ir/merge/concurrency.md`、`docs/ir/merge/deletion.md`、`docs/ir/merge/symlink.md`、`docs/ir/backup/rollback-path.md`（REQ-backup-027）

## Approach and why

**今の実装を参考にしない（foundation A1）。** 判定の関数群と流れの組み立ては、上の IR と記録だけから書く。下の「読まない・写さない」に挙げた今の比べる実装（コマンドごとの判定・読み込み・三者の扱い）は読まない。読んでよいのは、差し替える境界（`remote_merge::cli::*` の `execute_*` の形、`RuntimeTargets`、出力の型）の最小限のシグネチャと、段 1〜3 で IR から書いた部品（走査の規則と集め方、失敗の型、このマシンの操作、パスの確認、経路の選び方と接続断の扱い、SSH とエージェントの事実の集め方）と、テストだけである。

**判定は純粋な関数にし、扱いはコマンドに残す（E1・E2）。** 二つの側の状態の組と比べ方を受け取って判定を返す関数（TBL-compare-001）、読んだ結果で判定を決める関数（TBL-compare-002）、コマンドと指定から比べ方を選ぶ関数（TBL-compare-003）、読み込みの失敗の種類から扱いを決める関数（TBL-compare-004）、判定できない理由の文を作る関数（REQ-compare-014）、変更の直前の状態と比べたときの状態から変えてよいかを決める関数（REQ-compare-015）を置く。判定の値は列挙型にして、どの扱いの表も全部の値を挙げて書く（値を足したら組み立てが通らない形にする）。コマンドごとの扱いの表（TBL-cli-001 など）は段 5 で、この判定の値からの写しとして書く。

**流れの組み立ては経路の事実を集めるだけにする（E1・E5）。** 一つ目の段は段 2 の走査（範囲の走査と子の一覧）と段 1・3 のパスの確認で、左右と参照先の状態を集める。三つ目の段は、判定が中身を求めた組のハッシュと、コマンドが使うと告げた中身だけを、段 3 のまとめて尋ねる操作（大きさ・読み込み・ハッシュ）で集める。ハッシュには大きさの上限を当てず、中身の読み込みにだけ当てる。`接続断` は段 3 の `route_retry` の扱いに任せ、判定できないにしない。

**その他の種類と子の一覧は走査の規則で直す（E8・E11）。** 集め方はすでにその他の種類を見分けているので、走査の規則（`scan/walk.rs` の一覧の種類の決め方）でその他の種類のまま載せる。子の一覧は内部の上限を持たず、走査と同じ上限の値と数え方を使い、打ち切ったら不完全な一覧として返す。

## Scope of change

- 判定の関数群: `crates/remote-merge-core/src/` に新しいモジュール（名前は実装役が決める）
- 走査の規則と一覧の型: `crates/remote-merge-core/src/scan/`、`crates/remote-merge-engine/src/scan_result.rs`・`child_listing.rs`
- 中身の読み込みとハッシュが通常ファイルでないものを待たずに失敗すること: `crates/remote-merge-core/src/machine_ops/`、`crates/remote-merge-ssh/src/`（`contents*`）、`crates/remote-merge/src/runtime/`（`ssh_contents`）
- 子の一覧の上限: `crates/remote-merge-config/src/lib.rs`（`DEFAULT_MAX_DIR_ENTRIES` を消す）、`crates/remote-merge-engine/src/local_io.rs`、`crates/remote-merge/src/runtime/remote_change.rs`、`crates/remote-merge-agent/src/agent/tree_listing.rs`
- 流れの組み立て: `crates/remote-merge-engine/src/` か `crates/remote-merge/src/runtime/` に新しいモジュール（置き場所は実装役が決める。経路を呼ぶ部分は runtime の境界の内側に置く）
- テスト: `crates/remote-merge/tests/contract/` に流れを直接呼ぶ契約テストを足す（kotowari が印を数えるのはここと `cli_diff.rs` だけである）。既存のテストは、IR と違う動きを固めていたら IR に合わせて直し、記録に E 番号で残す（E19）
- 文書: `PROJECT.md` の構成の説明に新しいモジュールを足す。`skills/remote-merge/SKILL.md` の files_with_changes の説明を E14 の意味に直す。`crates/remote-merge-engine/src/child_listing.rs` の FLAG-cli-049 を引くコメントを消す
- 今の実装のうち読まない・写さないもの: 下の各行に挙げる。ファイル名から当たりを付けたもので、中身は読んでいない。段 4 では消さず、段 5 で載せ替えたら消す
- 読まない・写さない（コマンドごとの比べる処理）: `crates/remote-merge-engine/src/service/` の `diff.rs`・`status.rs`・`status_ref_badge.rs`・`status_unreadable.rs`・`content_reads.rs`・`fast_path.rs`・`kind_skip.rs`・`max_files.rs`・`merge.rs`・`merge_reference.rs`・`merge_reference_reads.rs`・`merge_flow.rs`・`sync.rs`・`rollback.rs`、`three_way` の実装
- 読まない・写さない（製品のコマンドと共通処理）: `crates/remote-merge/src/cli/` の diff・status・merge・sync・rollback の比べる部分、`crates/remote-merge/src/service/`
- 読まない・写さない（古い木と差分）: `crates/remote-merge-core/src/tree.rs`・`diff/`（読んでよいのは、段 5 まで残る古い木の型への変換に要る型の定義のシグネチャだけ）
- 読んでよいもの（段 1〜3 で IR から書いた部品）: `crates/remote-merge-core/src/` の `scan/`・`op_failure/`・`machine_ops/`・`path_resolution/`・`relative_path/`・`route_choice/`、`crates/remote-merge-engine/src/scan_result.rs`・`child_listing.rs`・`local_io.rs`、`crates/remote-merge/src/runtime/` の `route_state`・`route_retry`・`scan_entry`・`ssh_*`・`local_ops`・`local_target`・`agent_route/`、`crates/remote-merge-ssh/src/` の `scan_*`・`contents*`・`path_facts*`・`command_limit`

## Step order and prerequisites

S1 → S2 → S3 → S4 → S5 → S6 → S7 の順。S1・S2 は純粋な関数で互いに依らない。S3・S4・S5 は走査と操作の直しで、S1・S2 に依らないので並べてもよい。S6 は S1〜S5 のすべてを使う。S7 は最後に片付ける。

## Verification map

| 要件・例 | 示す段 | 確かめ方 |
|---|---|---|
| REQ-compare-006・TBL-compare-001・TBL-compare-002、EX-compare-006・007 | S1・S6 | 表の行ごとの単体テストと、流れを直接呼ぶ契約テスト |
| REQ-compare-007・TBL-compare-003 | S1 | 表の行ごとの単体テストと契約テスト。EX-compare-008 は CLI の出力の例なので段 5 |
| REQ-compare-008 | S1・S6 | symlink の組と辿った先の組の契約テスト。EX-compare-009 は merge の例なので段 5 |
| REQ-compare-009 | S1・S3・S6 | その他の種類の組を読まずに判定できないとする契約テスト。EX-compare-010・011 は CLI の例なので段 5 |
| REQ-compare-012・TBL-compare-004、EX-compare-014 | S2・S6 | 失敗の種類ごとの単体テストと、三つの経路で読み込みの失敗を起こす契約テスト |
| REQ-compare-014、EX-compare-016 | S2 | 理由の文の単体テストと契約テスト |
| REQ-compare-001・002・003・004、EX-compare-001・002・003・004 | S6 | 流れを直接呼ぶ契約テスト（三つの経路と、パスの指定の仕方を変えた組） |
| REQ-compare-010・011、EX-compare-012・013 | S6 | 上限を超える同じ中身・違う中身の組を流れで比べる契約テスト（ハッシュに上限が当たらず、中身の読み込みにだけ当たる） |
| REQ-compare-015、EX-compare-017・018 | S6 | 比べた後に書き込み先を書き換え、変更の直前の判定が止めることを確かめる契約テスト。merge・sync の結果としての確かめは段 5 |
| REQ-scan-021、EX-scan-017、REQ-scan-010 | S3 | 三つの経路で FIFO とソケットを走査する契約テスト |
| REQ-paths-021、EX-paths-046 | S4 | 三つの経路で書き手のいない FIFO の中身とハッシュを読み、待たずに失敗する契約テスト（時間の上限を付ける） |
| REQ-cli-021（子の一覧の件数） | S5 | 子の一覧が走査の上限を超えたときに不完全な一覧として返る契約テスト |
| REQ-compare-005・013・TBL-compare-005、EX-compare-005・015、コマンドの要件の書き直し（TBL-cli-001・REQ-cli-080・REQ-cli-088〜090・REQ-merge-017・019・020・033・034 ほか） | 段 5 | 各コマンドを流れに載せ替えたときに CLI の出力で示す |

## Left to the implementer

- 判定の関数群と流れの組み立てのモジュールの名前と置き場所
- 判定の値と状態の型の形（判定の値は列挙型にし、扱いの表は全部の値を挙げて書くことは守る）
- 流れの組み立ての入口の形（コマンドが中身を使うことを告げる仕方、書き込み先のハッシュを取る予定の告げ方）
- 段 5 まで残る古い木の型へその他の種類をどう渡すか（古いコマンドが FIFO を読みに行かないことは守る）
- 契約テストの状態の作り方

## Stop conditions

- IR の文が二通りに読めて、どちらを選ぶかで利用者に見える振る舞いが大きく変わるとき
- 判定の表のどの行にも当たらない状態の組が見つかったとき
- `remote_merge::cli::*` の `execute_*` の形か出力の型を変える必要があるとき
- 道具の限界で、三つの経路の状態やハッシュを揃えられないと分かったとき（特に、SSH で FIFO を開かずに通常ファイルでないと決められないとき）。揃えられない理由と、確かめた道具の版を添えて返す
- その他の種類を載せる変更や子の一覧の上限の変更で、既存のテストを IR と違う動きを固めていたと説明できる以上に書き換えないと通らないとき
- 利用者に返した U1〜U6 の答えが要るところに来たとき（とくに U6 で、その他の種類の扱いを変える答えが来たとき）

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

走査と SSH の読み込みのスクリプトを変えるので、最後に `scripts/run-container-e2e.sh` を走らせる。

## Out of scope

- diff・status・merge・sync・rollback を新しい流れに載せ替えること、コマンドごとの扱いの表を判定の値からの写しとして書くこと、古い比べる実装を消すこと（段 5）
- CLI の出力で示す要件と例のテスト（Verification map の「段 5」の行）
- 利用者に返した U1〜U6（参照先の印・種類の違いの終了コード・競合の意味・参照先との差の出し方・その他の種類の終了コード）
- 新しいディレクトリの権限の要件から経路の名前を外すこと（REQ-merge-012。段 5）
- 行の差分と競合の箇所の数え方（diff の hunk の作り方）

## Steps

### S1: 判定の表と比べ方の表を純粋な関数にする

- Purpose: 二つの側の状態の組と比べ方から判定を返す関数、読んだ結果で判定を決める関数、コマンドと指定から比べ方を選ぶ関数を置く。
- Specification: docs/ir/compare/judgement.md#REQ-compare-006, docs/ir/compare/judgement.md#REQ-compare-007, docs/ir/compare/judgement.md#REQ-compare-008, docs/ir/compare/judgement.md#REQ-compare-009, docs/ir/compare/flow.md#REQ-compare-002, docs/decision/records/2026-10-12-foundation-4.md#E2, docs/decision/records/2026-10-12-foundation-4.md#E4, docs/decision/records/2026-10-12-foundation-4.md#E6
- Prerequisites: なし
- May change: crates/remote-merge-core/src/
- Done when: TBL-compare-001・002・003 の行ごとの単体テストと、ディレクトリとディレクトリへの symlink の組・秒の精度しかない側・両側にない組・その他の種類の組を確かめる単体テストが通り、全テストが通る
- Shown by: test — 単体テスト（先に書いて落ちることを確かめてから実装する。印 `// @kotowari[REQ-compare-006]` など）
- Left to the implementer: 状態と判定の値の型の形と、モジュールの名前
- Stop and hand back if: 表のどの行にも当たらない状態の組が見つかったとき

### S2: 読み込みの失敗の扱いと判定できない理由の文を純粋な関数にする

- Purpose: 失敗の種類から判定できないか `接続断` の扱いかを決める関数と、失敗した側ごとの理由の文を作る関数を置く。
- Specification: docs/ir/compare/reading.md#REQ-compare-012, docs/ir/compare/reading.md#TBL-compare-004, docs/ir/compare/reading.md#REQ-compare-014, docs/ir/paths/operations.md#REQ-paths-020, docs/decision/records/2026-10-12-foundation-4.md#E7
- Prerequisites: なし
- May change: crates/remote-merge-core/src/
- Done when: 七つの失敗の種類ごとの単体テストと、左・右・参照先の組み合わせごとの理由の文の単体テストが通り、全テストが通る
- Shown by: test — 単体テスト（先に落ちることを確かめる）
- Left to the implementer: 関数の形と置き場所
- Stop and hand back if: 理由の文を作るのに失敗の型の文言（段 3 の `OpError`）を変える必要があるとき

### S3: 走査でその他の種類をその種類のまま載せる

- Purpose: FIFO・ソケット・デバイスを通常ファイルとせずにその他の種類として一覧に載せ、件数に数える。
- Specification: docs/ir/scan/listing.md#REQ-scan-021, docs/ir/scan/limits.md#REQ-scan-010, docs/ir/scan/listing.md#REQ-scan-017, docs/decision/records/2026-10-12-foundation-4.md#E8
- Prerequisites: なし
- May change: crates/remote-merge-core/src/scan/、crates/remote-merge-engine/src/scan_result.rs、crates/remote-merge-engine/src/child_listing.rs、古い木の型への変換（段 5 まで残る部分）、crates/remote-merge/tests/contract/
- Done when: EX-scan-017 の契約テスト（三つの経路で FIFO とソケットがその他の種類として載り、走査が止まらない）と、その他の種類を件数に数える単体テストが通り、古いコマンドが FIFO を読みに行かず、全テストが通る
- Shown by: test — 契約テスト（先に落ちることを確かめる）
- Left to the implementer: 一覧の型の形と、古い木の型への渡し方
- Stop and hand back if: 古い木の型にその他の種類を渡すと、古いコマンドの多くを書き換えないと組み立てが通らないとき

### S4: 中身の読み込みとハッシュで通常ファイルでないものを待たずに失敗にする

- Purpose: どの経路でも、辿った先が通常ファイルでないパスの中身とハッシュを、開いて待たずに I/O の失敗（"not a regular file"）にする。
- Specification: docs/ir/paths/operations.md#REQ-paths-021, docs/ir/paths/operations.md#REQ-paths-013, docs/ir/paths/operations.md#REQ-paths-020, docs/decision/records/2026-10-12-foundation-4.md#E8
- Prerequisites: なし
- May change: crates/remote-merge-core/src/machine_ops/、crates/remote-merge-ssh/src/、crates/remote-merge/src/runtime/、crates/remote-merge/tests/contract/
- Done when: EX-paths-046 の契約テスト（三つの経路で書き手のいない FIFO の中身とハッシュを読み、時間の上限の内に "not a regular file" の I/O で終わる）が通り、全テストが通る
- Shown by: test — 契約テスト（先に落ちることを確かめる）
- Left to the implementer: 開かずに決めるか、待たずに開いて確かめるかの仕方
- Stop and hand back if: SSH のリモートの道具で、FIFO を開かずに通常ファイルでないと決められないとき

### S5: 子の一覧を走査と同じ件数の上限で数え、打ち切りを知らせる

- Purpose: 子の一覧の内部の上限（10,000）をなくし、走査と同じ上限の値と数え方を使い、超えたら不完全な一覧として返す。
- Specification: docs/ir/scan/limits.md#REQ-scan-010, docs/ir/scan/limits.md#REQ-scan-004, docs/ir/cli/symlink-diff.md#REQ-cli-021, docs/decision/records/2026-10-12-foundation-4.md#E11
- Prerequisites: なし
- May change: crates/remote-merge-config/src/lib.rs、crates/remote-merge-engine/src/、crates/remote-merge/src/runtime/、crates/remote-merge-agent/src/agent/、crates/remote-merge/tests/contract/
- Done when: 子の一覧が上限を超えたときに三つの経路で不完全な一覧として返る契約テストが通り、`DEFAULT_MAX_DIR_ENTRIES` が消え、全テストが通る
- Shown by: test — 契約テスト（先に落ちることを確かめる）
- Left to the implementer: 上限の値を子の一覧に渡す仕方
- Stop and hand back if: 子の一覧を使う今のコマンドが、不完全な一覧を受けると IR と違う出力をするとき（段 5 の載せ替えを先取りしないと直せない場合）

### S6: 状態を集め、必要な分だけ読み、もう一度判定する流れを組み立てる

- Purpose: 段 2 の走査と段 1・3 のパスの確認で左右と参照先の状態を集め、S1 の判定が求めたハッシュとコマンドが使う中身だけをまとめて読み、S1・S2 で判定し直す流れと、変更の直前に書き込み先をもう一度判定する関数を置く。
- Specification: docs/ir/compare/flow.md#REQ-compare-001, docs/ir/compare/flow.md#REQ-compare-002, docs/ir/compare/flow.md#REQ-compare-003, docs/ir/compare/flow.md#REQ-compare-004, docs/ir/compare/reading.md#REQ-compare-010, docs/ir/compare/reading.md#REQ-compare-011, docs/ir/compare/reading.md#REQ-compare-015, docs/decision/records/2026-10-12-foundation-4.md#E1, docs/decision/records/2026-10-12-foundation-4.md#E3, docs/decision/records/2026-10-12-foundation-4.md#E5, docs/decision/records/2026-10-12-foundation-4.md#E9, docs/decision/records/2026-10-12-foundation-4.md#E15
- Prerequisites: S1, S2, S3, S4, S5
- May change: crates/remote-merge-engine/src/、crates/remote-merge/src/runtime/、crates/remote-merge/tests/contract/
- Done when: Verification map の S6 の行の契約テストが、ローカル・SSH・エージェントの経路で流れを直接呼んで通る（EX-compare-001〜004・006・007・012〜014・016〜018 を含む）。判定のためには中身を読まず、ハッシュに上限を当てないことを、読んだ操作を数えて確かめる。全テストが通る
- Shown by: test — 契約テスト（先に落ちることを確かめる）
- Left to the implementer: 入口の形と置き場所、まとめて尋ねる単位
- Stop and hand back if: `execute_*` の形か出力の型を変えないと流れを組み立てられないとき

### S7: 片付けと見張り

- Purpose: 構成の説明と利用者向けの手引きを合わせ、段 4 で示す要件のテストが揃ったことを確かめる。
- Specification: docs/decision/records/2026-10-12-foundation-4.md#E14, docs/decision/records/2026-10-12-foundation-4.md#E18, docs/decision/records/2026-10-12-foundation-4.md#E19
- Prerequisites: S6
- May change: PROJECT.md、skills/remote-merge/SKILL.md、crates/remote-merge-engine/src/child_listing.rs（コメント）、docs/decision/records/2026-10-12-foundation-4.md（E19 のテストの直しの記録だけ）
- Done when: `kotowari check` の error が、Verification map の「段 5」の行の要件と例のテストがないことによるものだけで、コンテナの e2e が通る
- Shown by: test — `kotowari check` とコンテナの e2e
- Left to the implementer: 説明の書き方
- Stop and hand back if: 段 4 で示すはずの要件か例にテストがないまま残るとき
