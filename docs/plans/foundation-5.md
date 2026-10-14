# Plan: 各コマンドを比べる流れに載せ替え、古い実装を消す

## Goal

diff・status・merge・sync・rollback が、段 4 で作った比べる流れ（`crates/remote-merge-core/src/compare/` の判定と、`crates/remote-merge/src/runtime/compare_flow.rs`・`compare_reads.rs`・`compare_checks.rs`・`compare_starts.rs` の組み立て）だけで比べ、判定をコマンドの扱い（表示・書き込み・スキップ・失敗・終了コード）に訳す層だけを持つ。古い比べる実装（古い木、コマンドごとの判定と読み込み、その他の種類の暫定の外し方、Side 単位の文字列のパスの API）は残らない。段 4 で段 5 に送った要件・例・出力の変更（下の Verification map）と、段 5 で決めた出力（F5・F12〜F15・F20）が CLI の出力のテストで示され、`kotowari check` の error が 0 になり、コンテナの e2e が通り、品質レビュー 1 回目の台帳のすべての項目が分けられている。

## Specification

- 方針: `docs/decision/records/2026-10-12-foundation.md`（A1〜A12。A12 でファイルの操作の経路をローカルとエージェントの二つにした）、`docs/decision/records/2026-10-12-foundation-5.md`（F1〜F22）
- 経路とエージェント: `docs/ir/ssh/fallback.md`（REQ-ssh-005・010・021・024、TBL-ssh-001）、`docs/ir/ssh/deploy.md`、`docs/ir/ssh/remote-shell.md`、`docs/ir/ssh/reconnect.md`、`docs/ir/paths/`、`docs/ir/scan/`、`docs/remote-tools.md`
- 前の段の記録: `docs/decision/records/2026-10-12-foundation-1.md`（B6）、`docs/decision/records/2026-10-12-foundation-3.md`（D46〜D49）、`docs/decision/records/2026-10-12-foundation-4.md`（E1〜E42。とくに E3〜E16・E19・E24〜E29・E33）
- 比べる流れ: `docs/ir/compare/flow.md`、`docs/ir/compare/judgement.md`、`docs/ir/compare/reading.md`
- status: `docs/ir/cli/status.md`（TBL-cli-001）、`docs/ir/cli/status-targets.md`（TBL-cli-005・REQ-cli-035）、`docs/ir/cli/status-output.md`、`docs/ir/cli/unreadable.md`（REQ-cli-088〜090）
- diff: `docs/ir/cli/diff-strategy.md`、`docs/ir/cli/diff-output.md`、`docs/ir/cli/diff-json.md`、`docs/ir/cli/directory-paths.md`、`docs/ir/cli/symlink-diff.md`、`docs/ir/cli/binary.md`、`docs/ir/cli/conflicts.md`（REQ-cli-097〜099）、`docs/ir/cli/reference.md`（REQ-cli-063・100・101）、`docs/ir/cli/size-limit.md`
- merge と sync: `docs/ir/cli/merge.md`（REQ-cli-047・051、TBL-cli-009）、`docs/ir/cli/sync.md`（TBL-cli-007、REQ-cli-039・045・072）、`docs/ir/cli/safety.md`、`docs/ir/merge/` の全文書（basic・comparison・concurrency・deletion・hunks・multi-target・permissions・read-failure・symlink）
- rollback: `docs/ir/backup/rollback-path.md`（REQ-backup-027・046）、`docs/ir/backup/rollback-cli.md`
- 台帳: 利用者の作業場所（main の作業ツリー）の `.agents/artifacts/quality/round1/ledger.json`・`ledger.md`（版の管理の外にある。コミットしない）

## Approach and why

**初めに SSH の経路を消す（foundation A12、F26）。** リモートのファイルの操作はエージェントだけで行い、エージェントを使えないサーバは読み書きの前に止める。載せ替えの前に消せば、扱いの層と契約テストが合わせる先はローカルとエージェントの二つになる。

**今の実装を参考にしない（foundation A1、F7）。** 新しい扱いの層は、上の IR と記録と、段 1〜4 で IR から書いた部品だけから書く。下の「読まない・写さない」に挙げた古い比べる実装は開かない。消すときは、`rg` で使い手がないことを確かめてから消し、中身は読まない。

**判定を扱いに訳す層を、コマンドごとの純粋な表にする（E1・E2・F5）。** 比べる流れの判定の値（同じ・違う・片側だけ・種類の違い・両側にない・判定できない・止めた）から、各コマンドの扱いを決める関数を置く（status の TBL-cli-001・TBL-cli-005、diff の files・summary・errors・note・競合、merge・sync の書き込み・スキップ・failed・終了コード、sync の TBL-cli-007、rollback の書き戻し・failed）。判定の値は列挙型なので、どの表も値をすべて挙げて書き、値を足したら組み立てが通らない形にする。判定できない理由の文は F5 の一つの関数で作る。

**載せ替えは読むだけのコマンドから（F6）。** status → diff → merge → sync → rollback の順に、各コマンドの `execute_*` の中身を「比べる流れを呼ぶ → 扱いの表で訳す → 出力の型に詰める」に替える。`execute_*` の形・`RuntimeTargets`・出力の型と書き出し（F7）は変えない。出力の型は E24〜E28 の分だけ欄を足す（F8）。古い実装は、それを使う最後のコマンドを載せ替えた段で消す。

**変わった出力は決めた変化かを確かめてから直す（F10・F11）。** 載せ替えで落ちた今のテストは、F10 に挙げた変化（E と F の番号）に当たるかを確かめ、当たれば IR に合わせて直して記録に F 番号で残す。当たらなければ載せ替えの誤りとして直す。

## Scope of change

- 扱いの層（新しい）: `crates/remote-merge-core/src/compare/` か `crates/remote-merge-engine/src/` に置く（置き場所は実装役が決める。経路を呼ばない純粋な関数にする）
- SSH の経路を消す（S1。F26）: `crates/remote-merge-ssh/src/` の `scan_*`・`path_facts*`・`contents*`・`change_*`・`entry_links*`・`command_limit*`・`tree_parser.rs` の書き込み・削除・symlink の作成のコマンドとそのテスト、`crates/remote-merge/src/runtime/` の `ssh_inspect.rs`・`ssh_contents.rs`・`ssh_scan.rs`・`ssh_changes.rs`・`ssh_entry_links.rs`・`remote_ops.rs`・`remote_change.rs`・`remote_target.rs` のうち SSH の経路の比較先、`crates/remote-merge-core/src/route_choice/` の SSH の経路を選ぶ枝、`root_watch*` の SSH 向けの分け、テストの偽の SSH サーバのうち SSH の経路の答えを作る部分、コンテナで SSH のスクリプトを試すテスト（REMOTE_MERGE_TEST_SCAN_IMAGE・REMOTE_MERGE_TEST_REQUIRE_SCAN_IMAGE・REMOTE_MERGE_TEST_REQUIRE_FISH）、`crates/remote-merge-core/src/op_failure/` の道具がない（F24）。残すのは SSH の接続・ホスト鍵・エージェントの配置と起動・エージェントとの通信（`remote-merge-ssh` の client・host_key_verifier・passphrase_provider・hint と `remote-merge-agent` の配置）と container e2e
- 各コマンドの載せ替え: `crates/remote-merge/src/cli/` の diff・status・merge・sync・rollback の `execute_*` の中身、`crates/remote-merge/src/runtime/` の比べる流れの入口（コマンドが使う中身・書き込み先のハッシュ・変更の直前の判定の告げ方の足りない分）
- 出力の型: `crates/remote-merge-engine/src/service/types.rs` と `crates/remote-merge-core/src/diff/conflict.rs` の `ConflictRegion` に、F8 の欄だけを足す。`crates/remote-merge/src/service/output.rs` は足した欄の書き出しだけを足す
- テスト: `crates/remote-merge/tests/contract/` と `crates/remote-merge/tests/cli_diff.rs` に CLI の出力のテストを足す（kotowari が印を数えるのはここと container-e2e だけである）。今のテストは F11 のとおり直す
- 文書: `PROJECT.md` の構成の説明（SSH の経路の部品と Old remote tools の試験の行（S1）、engine の service の各モジュール、`service/merge_flow.rs`・`service/rollback.rs` の説明、「段 5 まで」の二か所、`runtime/` の side_io の説明）、`skills/remote-merge/SKILL.md`（F10・F12・F13 で変わる出力の説明があれば）、`crates/remote-merge-core/src/scan/tree_build.rs` と `tree_build_tests.rs` の「段 5 で」のコメント
- 読んでよいもの（F7）: `remote_merge::cli::*` の `execute_*` の形、`RuntimeTargets`、`crates/remote-merge-engine/src/service/types.rs`、`crates/remote-merge/src/service/output.rs`（出力の型を文字列にする部分）、`crates/remote-merge-core/src/diff/engine.rs`・`conflict.rs`（行の差分と行の競合の関数の形）、`crates/remote-merge-engine/src/backup_store.rs` と `crates/remote-merge/src/runtime/backup_store.rs`、`crates/remote-merge-engine/src/service/path_resolver.rs` の `parse_path_arguments`、cli のうち判定をしない部分（引数の扱い・書き込む前の確認の問い・結果の書き出しの呼び出し）、段 1〜4 で書いた部品（`crates/remote-merge-core/src/` の `compare/`・`scan/`・`op_failure/`・`machine_ops/`・`path_resolution/`・`relative_path/`・`route_choice/`、`crates/remote-merge-engine/src/scan_result.rs`・`child_listing.rs`・`local_io.rs`、`crates/remote-merge/src/runtime/` の `compare_*`・`side_facts`・`root_watch*`・`root_gone`・`stopped_target`・`route_state`・`route_retry`・`scan_entry`・`local_ops`・`local_target`・`agent_route/`・`change_check`・`restore_before`・`restore_saved`。S1 の後は SSH の経路の部品は残らない）
- 読まない・写さない（古い比べる実装。消すときも中身を読まない）: `crates/remote-merge-engine/src/service/` の `diff.rs`・`status.rs`・`status_ref_badge.rs`・`status_unreadable.rs`・`content_reads.rs`・`fast_path.rs`・`kind_skip.rs`・`max_files.rs`・`merge.rs`・`merge_reference.rs`・`merge_reference_reads.rs`・`merge_flow.rs`・`sync.rs`・`rollback.rs`・`rollback_skip.rs`・`preview_paths.rs`・`other_kinds.rs`・`unreadable_names.rs`、`crates/remote-merge-engine/src/three_way.rs`、`crates/remote-merge/src/service/` の同じ名前の再公開と `merge_flow.rs`・`source_pair.rs`・`preview_paths.rs`・`permission_copy.rs`、`crates/remote-merge/src/cli/` の比べる部分（`diff_scans.rs`・`diff_children.rs`・`diff_kinds.rs`・`diff_listing.rs`・`content_reads.rs`・`ref_guard.rs` と各 `execute_*` の中の判定）、`crates/remote-merge/src/runtime/` の `side_io.rs` の Side 単位の文字列のパスの API・`side_reads.rs`・`side_hashes.rs`、`crates/remote-merge-core/src/tree.rs` と `diff/symlink.rs`、`crates/remote-merge-core/src/scan/tree_build.rs` の古い木を作る部分

## Step order and prerequisites

S0 → S1 → S2 → S3 → S4 → S5 → S6 → S7 → S8 → S9 の順。S0 は今の diff の出力を固めるので、S4 より前に置く。S1 は SSH の経路を消し、S2〜S9 の合わせる先をローカルとエージェントの二つにする。S2 は S3〜S7 のすべてが使う。S3〜S7 は一つずつ載せ替え、その段で使い手のなくなった古い実装を消す。S8 は構造の残りを消す。S9 は最後に台帳を確かめる。

## Verification map

| 要件・例 | 示す段 | 確かめ方 |
|---|---|---|
| REQ-ssh-005・010・021・024、TBL-ssh-001、EX-ssh-036・049・056、REQ-cli-070（F25）、REQ-paths-020・TBL-compare-004（F24） | S1 | エージェントを使えないサーバ（[agent] enabled が false・安全でない配置先・起動できない・通信の版が違う）が sudo の有無によらず読み書きの前に理由を示す接続のエラーで止まり、終了コード 2 を返す契約テストと、status -v が "connected" だけを出すテスト。F29 の 32 か所の印を消すか付け替える |
| 契約テストの「三つの経路」の要件と例（REQ-paths-008・013・017、REQ-scan-017、REQ-compare-002 と、その例） | S1 | 同じテストをローカルとエージェントの二つの経路で通す |
| REQ-cli-098、EX-cli-106・107（行の競合の数え方。E27） | S0 | 載せ替えの前に今の diff --ref の JSON の "conflict_count" を確かめるテストを書き、S4 の後も同じテストが通る |
| REQ-compare-009・REQ-compare-014、EX-compare-019・020（F5 の理由の文の形） | S2・S3〜S7 | 理由の文の単体テストと、名前で指定・ディレクトリの展開・root_dir 全体の指定の仕方ごと、status・diff・merge・sync・rollback のコマンドごとに理由が "not a regular file: 側" と "read failed: 側: 原因" になる CLI のテスト |
| TBL-cli-001・REQ-cli-088〜090、TBL-cli-005・REQ-cli-035、EX-cli-099・100（E7・E24） | S3 | status の CLI のテスト（参照先の印、読めないファイルの errors と終了コード 2） |
| REQ-compare-013・TBL-compare-005、EX-compare-015（status と diff の部分）、EX-compare-010（E8・E29） | S3・S4 | 同じ読めないファイルとその他の種類を status と diff で比べる CLI のテスト |
| REQ-compare-007、EX-compare-008（E4） | S4 | パスを指定しない diff で、大きさと更新時刻が同じで中身の違うファイルの差分が出て終了コード 1 になる CLI のテスト |
| REQ-compare-010・011、EX-compare-012・013（E5。diff の部分） | S4 | 上限を超える同じ中身と違う中身の diff の CLI のテスト |
| REQ-cli-021（子の一覧の件数。E11）、REQ-cli-022・E12、E13、E14、REQ-compare-003・EX-compare-003（E3）、REQ-compare-004・EX-compare-004（E15）、E33 | S4 | diff の CLI のテスト（子の一覧の打ち切りの errors と終了コード 2、テキストの標準エラー、note、files_with_changes、比べる前に消えたファイル、参照先の root_dir の外、途中がディレクトリでない起点） |
| REQ-cli-016・REQ-cli-097・099、EX-cli-104・105・108（E16・E26）、REQ-cli-063・100・101、EX-cli-109〜111（E28） | S4 | diff --ref の CLI のテスト（ファイル全体の競合、"ref"・"ref_hash"・"truncated"・{"whole_file": true}） |
| REQ-compare-005・EX-compare-005（E10）、REQ-compare-008・EX-compare-009（E6）、REQ-cli-047・EX-cli-101・102（E25） | S5 | --ref と --dry-run の merge、三つとも symlink の merge、種類の違いのスキップの終了コード 2 の CLI のテスト |
| REQ-compare-015・EX-compare-017（E9。--hunks）、REQ-merge-011・033・034、REQ-merge-017・019・020、EX-compare-012（merge の部分）、EX-compare-015（merge の部分） | S5 | 比べた後に中身だけ変わった書き込み先を書かない、読めないファイルを failed に出す、上限を超える同じ中身を書かない merge の CLI のテスト |
| REQ-merge-026・027、EX-merge-067（F12）、REQ-merge-030、EX-merge-068（F13） | S5 | merge --delete --dry-run と --hunks の差分のないファイルの CLI のテスト |
| REQ-backup-018、EX-backup-033（F20） | S5・S6 | バックアップが有効で集約先が決まらず、書き込むファイルも消すファイルもない merge と sync が止まらずに結果を出す CLI のテスト |
| REQ-compare-015・EX-compare-018（E9。--delete）、EX-compare-011（E8）、REQ-cli-041・TBL-cli-007・EX-cli-103・112（E25・F14）、REQ-cli-039・EX-cli-113（F15）、REQ-cli-045・072（F12） | S6 | sync の CLI のテスト（比べた後に書き換えられた書き込み先だけのファイル、ソケット、種類の違いと削除だけの書き込み先の status、接続に失敗した書き込み先の並び、--dry-run の削除予定） |
| REQ-backup-027（E3。戻す先の状態）、REQ-backup-046、TBL-compare-005 の rollback の行 | S7 | rollback の CLI のテスト（戻す先の状態からないことを決める、戻す先がその他の種類なら理由 "not a regular file: target" で failed） |
| foundation-1 B6（Side 単位の文字列のパスの API を消す）、PROJECT.md の「段 5 まで」の二か所 | S8 | 消したことを `rg` で確かめ、全テストが通る |
| foundation A12 のエージェントを CentOS 5 のカーネル 2.6.18 で配置・起動・走査できること（F27） | S9 | qemu で 2.6.18 を起動して確かめる検証 |
| foundation A7 の台帳の確かめ（F17・F28）、F18 の終わりの条件 | S9 | 台帳のすべての項目の分け方と、`kotowari check` の error 0、コンテナの e2e |

## Left to the implementer

- 扱いの層のモジュールの名前と置き場所、関数の形（判定の値をすべて挙げて扱いを決めることは守る）
- 比べる流れの入口に足す告げ方の形（コマンドが使う中身、書き換えるか消す予定の書き込み先のハッシュ、--dry-run）
- 出力の型に足す欄の Rust での形（JSON の名前と値は IR のとおりにする）
- 各段でどのテストファイルに CLI のテストを置くか
- 古い実装を消す順のうち、同じ段の中の順

## Stop conditions

- IR の文が二通りに読めて、どちらを選ぶかで利用者に見える振る舞いが大きく変わるとき
- `execute_*` の形を変えるか、出力の型の今ある欄を消すか名前を変える必要があるとき（F8 の外）
- 載せ替えで落ちた今のテストが、F10 のどの変化にも当たらず、載せ替えの誤りとしても説明できないとき
- 計画の段で古いモジュールを消せないとき（使い手が残る理由を、モジュールの中身を読まずに説明して返す）
- S9 で、critical か warn の台帳の項目が残り、直すのに利用者の判断が要るとき

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

各段の終わりに `kotowari check` を走らせる（S1 の前は、F29 の印が試験の側の unresolved_reference として出る）。S8 と S9 の終わりに `scripts/run-container-e2e.sh` を走らせる。

## Out of scope

- 比べる流れの判定の規則そのもの（段 4 で決めた。誤りが見つかったら止まる条件で返す）
- 行の差分と行の競合を求める関数の作り直し（F21）
- 書き込む前の確認の問いの文言と、JSON の sync で確認を断ったときの出力（FLAG-cli-007・FLAG-cli-015。F19）
- 行番号の数え方と conflict_regions の行の範囲の要素の形（FLAG-cli-032・FLAG-cli-058。F19）
- バックアップの記録の連番のあふれ（FLAG-backup-005。F19）

## Steps

### S0: 行の競合の数え方を今の diff の出力で固める

- Purpose: 載せ替えの前に、行を足す変更と重なる変更の競合の数え方を CLI のテストに固め、載せ替えで変わったかを測れるようにする。
- Specification: docs/ir/cli/conflicts.md#REQ-cli-098, docs/decision/records/2026-10-12-foundation-4.md#E27, docs/decision/records/2026-10-12-foundation-5.md#F6
- Prerequisites: なし
- May change: crates/remote-merge/tests/contract/
- Done when: EX-cli-106・107 と REQ-cli-098 の四つの場合（同じ位置に足す・変えた範囲の中に足す・変えた範囲の先頭の前に足す・接するが重ならない二つ）を確かめる CLI のテストが今の実装で通り、全テストが通る
- Shown by: test — CLI の契約テスト（印 `// @kotowari[REQ-cli-098, EX-cli-106]` など）。今の示し方を固めるテストなので、先に落ちることは求めない
- Left to the implementer: テストの状態の作り方
- Stop and hand back if: 今の diff の数え方が REQ-cli-098 の文と食い違うとき

### S1: SSH の経路を消し、エージェントを使えないサーバを止める

- Purpose: リモートのファイルの操作をエージェントだけで行い、エージェントを使えないサーバを sudo の有無によらず読み書きの前に止め、SSH の経路のコードとテストを消す。
- Specification: docs/decision/records/2026-10-12-foundation.md#A12, docs/ir/ssh/fallback.md#REQ-ssh-005, docs/ir/ssh/fallback.md#REQ-ssh-010, docs/ir/ssh/fallback.md#REQ-ssh-021, docs/ir/ssh/fallback.md#TBL-ssh-001, docs/ir/ssh/remote-shell.md#REQ-ssh-020, docs/ir/ssh/reconnect.md#REQ-ssh-003, docs/ir/paths/operations.md#REQ-paths-020, docs/ir/cli/status-output.md#REQ-cli-070, docs/decision/records/2026-10-12-foundation-5.md#F24, docs/decision/records/2026-10-12-foundation-5.md#F25, docs/decision/records/2026-10-12-foundation-5.md#F26, docs/decision/records/2026-10-12-foundation-5.md#F29
- Prerequisites: S0
- May change: crates/remote-merge-ssh/src/、crates/remote-merge/src/runtime/、crates/remote-merge-core/src/route_choice/、crates/remote-merge-core/src/op_failure/、crates/remote-merge-agent/src/、crates/remote-merge-protocol/src/（道具がないの種類を運ばなくなるなら通信の版を上げる）、crates/remote-merge/tests/、tests/container-e2e/、PROJECT.md、docs/decision/records/2026-10-12-foundation-5.md（F11 の記録だけ）
- Done when: Verification map の S1 の行のテストが先に落ちてから通り、F26 の消すものが消え（`rg` で使い手がないことを確かめる）、F29 の印がすべて消えるか付け替えられて `kotowari check` に試験の側の unresolved_reference が残らず、契約テストの三つの経路がローカルとエージェントの二つになり、PROJECT.md から SSH の経路の部品と Old remote tools の試験の行が消え、全テストが通り、コンテナの e2e が通る
- Shown by: test — 契約テストと container e2e（先に落ちることを確かめる）
- Left to the implementer: 消す順と、偽の SSH サーバの残す部分の形
- Stop and hand back if: エージェントの配置か起動に、IR と docs/remote-tools.md にない道具が要ると分かったとき、または SSH の経路を消すと container e2e のエージェントの sudo の確かめが成り立たないとき

### S2: 判定を各コマンドの扱いに訳す表と、判定できない理由の文を置く

- Purpose: 比べる流れの判定の値から、status・diff・merge・sync・rollback の扱いを決める純粋な関数と、F5 の理由の文を作る関数を置く。
- Specification: docs/ir/compare/flow.md#REQ-compare-001, docs/ir/compare/reading.md#TBL-compare-005, docs/ir/compare/reading.md#REQ-compare-014, docs/ir/compare/judgement.md#REQ-compare-009, docs/ir/cli/status.md#TBL-cli-001, docs/ir/cli/status-targets.md#TBL-cli-005, docs/ir/cli/sync.md#TBL-cli-007, docs/ir/cli/merge.md#TBL-cli-009, docs/decision/records/2026-10-12-foundation-4.md#E2, docs/decision/records/2026-10-12-foundation-4.md#E7, docs/decision/records/2026-10-12-foundation-5.md#F5, docs/decision/records/2026-10-12-foundation-5.md#F14
- Prerequisites: S1
- May change: crates/remote-merge-core/src/、crates/remote-merge-engine/src/（新しいモジュールだけ）
- Done when: 各表の行ごとの単体テストと、理由の文の側の組み合わせごと（left・right・ref、rollback の target、読み込みの失敗とその他の種類）の単体テストが通り、全テストが通る
- Shown by: test — 単体テスト（先に書いて落ちることを確かめてから実装する）
- Left to the implementer: 表の関数の形と置き場所
- Stop and hand back if: 判定の値のどれかが、あるコマンドの表のどの行にも当たらないとき

### S3: status を比べる流れに載せ替え、status だけが使う古い実装を消す

- Purpose: `execute_status` を、比べる流れ（比べ方は TBL-compare-003 の status の行）と S2 の表で動かし、参照先の印と判定できないパスを IR のとおりに出す。
- Specification: docs/ir/cli/status.md#TBL-cli-001, docs/ir/cli/status-targets.md#TBL-cli-005, docs/ir/cli/status-targets.md#REQ-cli-035, docs/ir/cli/unreadable.md#REQ-cli-088, docs/ir/cli/unreadable.md#REQ-cli-090, docs/ir/compare/reading.md#REQ-compare-013, docs/ir/compare/judgement.md#REQ-compare-009, docs/ir/cli/status-output.md#REQ-cli-070, docs/decision/records/2026-10-12-foundation-4.md#E24, docs/decision/records/2026-10-12-foundation-5.md#F10, docs/decision/records/2026-10-12-foundation-5.md#F11
- Prerequisites: S2
- May change: crates/remote-merge/src/cli/status.rs、crates/remote-merge/src/runtime/、crates/remote-merge-engine/src/service/（status だけが使うモジュールを消す）、crates/remote-merge/src/service/（同じ名前の再公開を消す）、crates/remote-merge/tests/contract/、docs/decision/records/2026-10-12-foundation-5.md（F11 のテストの直しの記録だけ）
- Done when: Verification map の S3 の行の CLI のテストが先に落ちてから通り、status だけが使っていた古い実装（engine の `status.rs`・`status_ref_badge.rs`・`status_unreadable.rs`、runtime の `side_hashes.rs`。使い手がほかに残れば、その段まで残す）が消え、IR と違う動きを固めていたテストは IR に合わせて直してどの要件と違っていたかを記録に F 番号で残し、全テストが通る
- Shown by: test — CLI の契約テスト（先に落ちることを確かめる）
- Left to the implementer: 消す順と、比べる流れへの入口の足し方
- Stop and hand back if: status の出力の型の今ある欄を変えないと TBL-cli-001・TBL-cli-005 を出せないとき

### S4: diff を比べる流れに載せ替え、diff だけが使う古い実装を消す

- Purpose: `execute_diff` を、比べる流れ（パスの指定の仕方によらず中身で比べる）と S2 の表で動かし、差分・errors・note・summary・参照先との差・競合を IR のとおりに出す。
- Specification: docs/ir/compare/judgement.md#REQ-compare-007, docs/ir/compare/judgement.md#REQ-compare-008, docs/ir/compare/reading.md#REQ-compare-011, docs/ir/compare/flow.md#REQ-compare-003, docs/ir/compare/flow.md#REQ-compare-004, docs/ir/cli/diff-strategy.md#REQ-cli-080, docs/ir/cli/symlink-diff.md#REQ-cli-021, docs/ir/cli/symlink-diff.md#REQ-cli-022, docs/ir/cli/conflicts.md#REQ-cli-097, docs/ir/cli/conflicts.md#REQ-cli-099, docs/ir/cli/reference.md#REQ-cli-063, docs/ir/cli/reference.md#REQ-cli-100, docs/ir/cli/reference.md#REQ-cli-101, docs/decision/records/2026-10-12-foundation-4.md#E4, docs/decision/records/2026-10-12-foundation-4.md#E11, docs/decision/records/2026-10-12-foundation-4.md#E13, docs/decision/records/2026-10-12-foundation-4.md#E14, docs/decision/records/2026-10-12-foundation-4.md#E16, docs/decision/records/2026-10-12-foundation-4.md#E26, docs/decision/records/2026-10-12-foundation-4.md#E28, docs/decision/records/2026-10-12-foundation-5.md#F8
- Prerequisites: S0, S3
- May change: crates/remote-merge/src/cli/、crates/remote-merge/src/runtime/、crates/remote-merge-engine/src/service/types.rs（F8 の欄だけ）、crates/remote-merge-core/src/diff/conflict.rs（`ConflictRegion` の F8 の形だけ）、crates/remote-merge/src/service/output.rs（足した欄の書き出しだけ）、crates/remote-merge-engine/src/service/（diff だけが使うモジュールを消す）、crates/remote-merge/src/service/、crates/remote-merge/tests/contract/、crates/remote-merge/tests/cli_diff.rs、skills/remote-merge/SKILL.md、docs/decision/records/2026-10-12-foundation-5.md（F11 の記録だけ）
- Done when: Verification map の S4 の行の CLI のテストが先に落ちてから通り、S0 のテストも通り、diff だけが使っていた古い実装（engine の `diff.rs`・`fast_path.rs`・`max_files.rs`・`content_reads.rs`、cli の `diff_scans.rs`・`diff_children.rs`・`diff_kinds.rs`・`diff_listing.rs`・`content_reads.rs` と、比べる流れの範囲の集め方と重なる `diff_expansion.rs`）が消え、F11 のとおりテストを直し、全テストが通る
- Shown by: test — CLI の契約テスト（先に落ちることを確かめる）
- Left to the implementer: --max-files と --max-lines の枠を扱いの層のどこで当てるか、消す順
- Stop and hand back if: 行の差分の関数（F7）の中身を変えないと REQ-cli-097・099 を出せないとき

### S5: merge を比べる流れに載せ替える

- Purpose: `execute_merge`（--hunks・--ref・--dry-run・--delete・--with-permissions を含む）を、比べる流れと S2 の表と変更の直前の判定（`recheck_before_change`）で動かす。
- Specification: docs/ir/compare/flow.md#REQ-compare-005, docs/ir/compare/reading.md#REQ-compare-015, docs/ir/compare/judgement.md#REQ-compare-008, docs/ir/compare/judgement.md#REQ-compare-009, docs/ir/cli/merge.md#REQ-cli-047, docs/ir/cli/merge.md#REQ-cli-051, docs/ir/merge/symlink.md#REQ-merge-001, docs/ir/merge/concurrency.md#REQ-merge-011, docs/ir/merge/concurrency.md#REQ-merge-033, docs/ir/merge/concurrency.md#REQ-merge-044, docs/ir/merge/read-failure.md#REQ-merge-017, docs/ir/merge/read-failure.md#REQ-merge-019, docs/ir/merge/comparison.md#REQ-merge-020, docs/ir/merge/permissions.md#REQ-merge-036, docs/ir/merge/deletion.md#REQ-merge-026, docs/ir/merge/deletion.md#REQ-merge-027, docs/ir/merge/hunks.md#REQ-merge-030, docs/decision/records/2026-10-12-foundation-4.md#E9, docs/decision/records/2026-10-12-foundation-4.md#E10, docs/decision/records/2026-10-12-foundation-4.md#E25, docs/decision/records/2026-10-12-foundation-5.md#F12, docs/decision/records/2026-10-12-foundation-5.md#F13, docs/ir/backup/failure.md#REQ-backup-018, docs/decision/records/2026-10-12-foundation-5.md#F20
- Prerequisites: S4
- May change: crates/remote-merge/src/cli/merge.rs、crates/remote-merge/src/runtime/、crates/remote-merge-engine/src/service/types.rs（F8 の欄だけ）、crates/remote-merge/src/service/output.rs（F12・F13 の行だけ）、crates/remote-merge/tests/contract/、docs/decision/records/2026-10-12-foundation-5.md（F11 の記録だけ）
- Done when: Verification map の S5 の行の CLI のテストが先に落ちてから通り、merge の書き込み・symlink の作成と置き換え・削除・--hunks のどれでも変更の直前の判定を通ることを契約テストで確かめ、merge だけが使っていた古い実装（使い手が sync に残るものは S6 まで残す）が消え、F11 のとおりテストを直し、全テストが通る
- Shown by: test — CLI の契約テスト（先に落ちることを確かめる）
- Left to the implementer: 変更の直前の判定を書き込みの手順のどこで呼ぶか、merge と sync で共有する部分の形
- Stop and hand back if: --dry-run を実行時と同じ判定にすると今ある出力の欄を変える必要があるとき

### S6: sync を比べる流れに載せ替え、merge と sync が共有した古い実装を消す

- Purpose: `execute_sync` を、S5 と同じ部品で書き込み先ごとに動かし、書き込み先の status・並び・削除予定の出し方を IR のとおりにする。
- Specification: docs/ir/cli/sync.md#REQ-cli-039, docs/ir/cli/sync.md#REQ-cli-041, docs/ir/cli/sync.md#TBL-cli-007, docs/ir/cli/sync.md#REQ-cli-045, docs/ir/cli/sync.md#REQ-cli-071, docs/ir/cli/sync.md#REQ-cli-072, docs/ir/merge/basic.md#REQ-merge-035, docs/ir/merge/deletion.md#REQ-merge-024, docs/ir/compare/reading.md#REQ-compare-015, docs/ir/compare/judgement.md#REQ-compare-009, docs/decision/records/2026-10-12-foundation-4.md#E25, docs/decision/records/2026-10-12-foundation-5.md#F12, docs/decision/records/2026-10-12-foundation-5.md#F14, docs/decision/records/2026-10-12-foundation-5.md#F15
- Prerequisites: S5
- May change: crates/remote-merge/src/cli/sync.rs、crates/remote-merge/src/runtime/、crates/remote-merge-engine/src/service/、crates/remote-merge-engine/src/three_way.rs、crates/remote-merge/src/service/、crates/remote-merge/src/cli/ref_guard.rs、crates/remote-merge/tests/contract/、docs/decision/records/2026-10-12-foundation-5.md（F11 の記録だけ）
- Done when: Verification map の S6 の行の CLI のテストが先に落ちてから通り、merge と sync が使っていた古い実装（engine の `merge.rs`・`merge_flow.rs`・`merge_reference.rs`・`merge_reference_reads.rs`・`kind_skip.rs`・`sync.rs`・`preview_paths.rs`・`three_way.rs`、製品の `service/merge_flow.rs`・`source_pair.rs`・`preview_paths.rs`・`permission_copy.rs`、cli の `ref_guard.rs`）が消え、F11 のとおりテストを直し、全テストが通る
- Shown by: test — CLI の契約テスト（先に落ちることを確かめる）
- Left to the implementer: 消す順
- Stop and hand back if: 計画の段で消せないモジュールがあるとき

### S7: rollback を比べる流れに載せ替え、rollback だけが使う古い実装を消す

- Purpose: `execute_rollback` を、戻す先の状態を比べる流れの集め方で集め、S2 の rollback の表で書き戻し・failed を決める形にする。
- Specification: docs/ir/backup/rollback-path.md#REQ-backup-027, docs/ir/backup/rollback-path.md#REQ-backup-046, docs/ir/compare/flow.md#REQ-compare-003, docs/ir/compare/reading.md#TBL-compare-005, docs/ir/compare/reading.md#REQ-compare-014, docs/ir/merge/concurrency.md#REQ-merge-044, docs/decision/records/2026-10-12-foundation-4.md#E3, docs/decision/records/2026-10-12-foundation-4.md#E7, docs/decision/records/2026-10-12-foundation-5.md#F5
- Prerequisites: S6
- May change: crates/remote-merge/src/cli/rollback.rs、crates/remote-merge/src/runtime/、crates/remote-merge-engine/src/service/（`rollback.rs`・`rollback_skip.rs`）、crates/remote-merge/src/service/、crates/remote-merge/tests/contract/、docs/decision/records/2026-10-12-foundation-5.md（F11 の記録だけ）
- Done when: Verification map の S7 の行の CLI のテストが先に落ちてから通り、rollback だけが使っていた古い実装が消え、F11 のとおりテストを直し、全テストが通る
- Shown by: test — CLI の契約テスト（先に落ちることを確かめる）
- Left to the implementer: 戻す先の状態を集める単位（まとめて尋ねること）
- Stop and hand back if: 書き戻しの最中の `接続断` の確かめ（`change_check`・`restore_before`・`restore_saved`）の形を変えないと載せ替えられないとき

### S8: 構造の残りを消し、構成の説明を合わせる

- Purpose: どのコマンドにも属さない古い実装（古い木、その他の種類の暫定の扱い、Side 単位の文字列のパスの API）を消し、PROJECT.md を今の構成に合わせる。
- Specification: docs/decision/records/2026-10-12-foundation-1.md#B6, docs/decision/records/2026-10-12-foundation-5.md#F6, docs/decision/records/2026-10-12-foundation-5.md#F9
- Prerequisites: S7
- May change: crates/remote-merge/src/runtime/（`side_io.rs`・`side_reads.rs`・`side_hashes.rs` の残り）、crates/remote-merge-engine/src/service/（`other_kinds.rs`・`unreadable_names.rs`・`path_resolver.rs` の `parse_path_arguments` 以外・`types.rs` の使われない入力の型）、crates/remote-merge/src/service/、crates/remote-merge-core/src/tree.rs、crates/remote-merge-core/src/diff/symlink.rs、crates/remote-merge-core/src/scan/tree_build.rs・tree_build_tests.rs、crates/remote-merge-engine/src/scan_result.rs、PROJECT.md、各テスト（消した部品を呼んでいたもの）
- Done when: 上の古い実装の使い手がないことを `rg` で確かめて消し（出力の型と、段 1〜4 の部品が使う型は残す）、PROJECT.md の構成の説明に消したモジュールが残らず、「段 5 まで」の二か所が今の構成の文になり、全テストが通り、コンテナの e2e が通る
- Shown by: test — 全テストとコンテナの e2e
- Left to the implementer: 古い木の型のうち、段 1〜4 の部品が使っているものをどの型に置き換えるか
- Stop and hand back if: 段 1〜4 の部品が古い木の型に頼っていて、置き換えると段 1〜4 の契約テストの確かめる中身が変わるとき

### S9: 品質レビュー 1 回目の台帳の項目を確かめる

- Purpose: 台帳のすべての項目を、消えた・security の境界の外・残るに分け、残る critical と warn を直す。
- Specification: docs/decision/records/2026-10-12-foundation-5.md#F17, docs/decision/records/2026-10-12-foundation-5.md#F18, docs/decision/records/2026-10-12-foundation-5.md#F27, docs/decision/records/2026-10-12-foundation-5.md#F28, docs/decision/records/2026-10-12-foundation.md#A7, docs/decision/records/2026-10-12-foundation.md#A11
- Prerequisites: S8
- May change: crates/、docs/decision/records/2026-10-12-foundation-5.md（残った項目を直す決定だけ）、台帳（main の作業ツリーの `.agents/artifacts/quality/round1/ledger.json`・`ledger.md`。コミットしない）
- Done when: ledger.json のすべての項目（132 件）と、ledger.md の後の単位の項目（sk-・ui-・so-・pv- で始まるもの）が、消えた（決定の番号と、示すテストか消えたコードの場所）・security の境界の外（A11・D46）・経路をなくしたので消えた（A12 と S1。SSH の経路にだけ関わる項目）・残る（info だけ）に分けられ、ledger.md に「foundation の結果」の節があり、ledger.json の status が "fixed (foundation, コミット)" か分けた理由の形になり、残っていた critical と warn はテストを先に書いて直され、`kotowari check` の error が 0 で、コンテナの e2e が通り、qemu で起動した 2.6.18 のカーネルでエージェントを配置・起動・走査できる（F27）
- Shown by: test — 全テスト、`kotowari check`、コンテナの e2e、qemu の検証。台帳の分け方は review（台帳の各項目に根拠を書く）
- Left to the implementer: 項目を確かめる順と、根拠の書き方
- Stop and hand back if: critical か warn の項目が残り、直すのに IR の変更か利用者の判断が要るとき、または 2.6.18 のカーネルでエージェントが動かないとき
