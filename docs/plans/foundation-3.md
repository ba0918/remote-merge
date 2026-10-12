# Plan: 経路を事実の集め方と一つの型の失敗にする

## Goal

三つの経路（ローカル・SSH・エージェント）は、操作の失敗を一つの型の七つの種類で返し、規則（パスを辿る手順・子の一覧と走査の規則・大きさの上限・経路の選び方）は経路の外の純粋な関数が決める。SSH はパスの確認・子の一覧・大きさ・読み込み・ハッシュの事実をまとめて尋ねるだけになり、エージェントは通信の版 12 で失敗の種類を運ぶ。エージェントを使うか SSH にするかは一つの関数がサーバにつなぐときに一度だけ決め、操作の失敗では選び直さない。書き直した IR（下の Specification）の要件と例がすべてテストで示され、`kotowari check` の error が 0 になる。

## Specification

- 方針: `docs/decision/records/2026-10-12-foundation.md`（A1〜A10）、`docs/decision/records/2026-10-12-foundation-3.md`（D1〜D12・D14・D18〜D20。D13 は D8・D18・D19 で決まった）
- 段 1・段 2 の記録: `docs/decision/records/2026-10-12-foundation-1.md`（B4・B7・B8）、`docs/decision/records/2026-10-12-foundation-2.md`（C6・C11・C14・C15）
- 操作と失敗の種類: `docs/ir/paths/operations.md`（REQ-paths-013・016・017・020）、`docs/ir/paths/FLAGS.md`
- パスの確認: `docs/ir/paths/resolution.md`、`docs/ir/paths/routes.md`（REQ-paths-008・009・012）
- 経路の選び方: `docs/ir/ssh/fallback.md`（TBL-ssh-001、REQ-ssh-005・010・021・024・027）、`docs/ir/ssh/deploy.md`
- 接続断: `docs/ir/CONTEXT.md` の用語、`docs/ir/ssh/reconnect.md`（REQ-ssh-003・004・022・023）
- リモートの道具: `docs/ir/ssh/compatibility.md`（TBL-ssh-002、REQ-ssh-025・026）、`docs/remote-tools.md`
- 大きさの上限と書き込み先の操作: `docs/ir/cli/size-limit.md`（REQ-cli-079）、`docs/ir/merge/concurrency.md`（REQ-merge-038・039・040・041）、`docs/ir/merge/FLAGS.md`（FLAG-merge-021）

## Approach and why

**今の実装を参考にしない（foundation A1）。** 失敗の型・訳し方・経路の選び方・SSH の事実の集め方は、上の IR と記録と `docs/remote-tools.md` だけから書く。下の「読まない・写さないもの」は、置き換えたら消す。読んでよいのは、差し替える境界（`remote_merge::cli::*` の `execute_*` の形、`RuntimeTargets`、出力の型、段 1 の `RelPath`・`EntryPath`、段 2 の走査の規則と集め方の入口、通信の型の定義）の最小限のシグネチャとテストだけである。

**失敗は一つの型にする（D1）。** 型は core に置き、ローカルとエージェントは同じ関数で OS の誤りをこの型に訳す（D2）。上限を超えるは、経路が答えた大きさと上限から一つの純粋な関数が決める。利用者に見える失敗の文言はこの型から一か所で作り、出力の型は変えない（foundation A6）。

**経路の選び方は一つの純粋な関数にする（D3・D4）。** 入力は sudo の指定・`[agent] enabled`・配置先の検査の結果・配置と起動の結果・通信の版が同じか、出力はエージェント・SSH（警告付き）・止める、で TBL-ssh-001 をそのまま書く。配置・起動できずに SSH にするときも、理由を含む英語の警告を標準エラーに出す（D19）。サーバにつなぐときに一度だけ呼び、その後は選び直さない。接続断は同じ経路でつなぎ直す。段 2 で走査の失敗を TreeFailed として運び SSH に回さないようにした形（C11・C15）を、すべての操作に広げる。

**SSH は事実を集めるだけにする（D5〜D8）。** パスの確認は段 1 の純粋な手順をそのまま使い、SSH は確かめる途中のすべてのパスの、まだ辿っていない要素の事実を一回の sh で答える。手順が symlink を差し込んだパスだけを次の回に回す。失敗の理由は、シェルのテストで決まらないときだけ `ls -d` の C ロケールの文言の終わりで見分ける（D2・D10）。子の一覧は段 2 の走査の集め方を深さ一つにして使い、子の一覧のための find の書き方を持たない（D6）。大きさ・読み込み・ハッシュは複数のパスをまとめ、答えはパスごとに成功か失敗の種類を持つ（D7）。道具がないときは推し量らずに道具がないの失敗にする（TBL-ssh-002）。find が -printf か `-exec {} +` を受け付けないときの誤りの文には、要る find と、エージェントを使えば扱えることを入れる（D8）。

**通信の版を 12 に上げる（D9）。** 操作の応答は成功の値か失敗の種類を運ぶ。エージェントの応答のフレームを読めない・要求と合わない応答は接続断にする（D2）。SSH の答えの形が壊れているときも接続断にする（D20）。

**書き込みはどの経路もその場で書き換える（D18）。** 一時ファイルを作らず、書き込み先の inode・所有者・ハードリンクを保つ。ローカルやエージェントが今この書き方になっていないと分かったら、S7 で直す。

## Scope of change

- 失敗の型と訳し方・大きさの上限の判定: `crates/remote-merge-core/src/`
- 経路の選び方: 純粋な関数を `crates/remote-merge-core/src/` か `crates/remote-merge-engine/src/` に置き、呼び出しを `crates/remote-merge/src/runtime/` から行う
- 通信: `crates/remote-merge-protocol/src/`、`crates/remote-merge-agent/src/agent/`
- SSH の事実の集め方: `crates/remote-merge-ssh/src/`、`crates/remote-merge/src/runtime/`
- 経路の操作の差し替え: `crates/remote-merge-engine/src/`、`crates/remote-merge/src/runtime/`、`crates/remote-merge/src/agent/`
- テスト: `crates/remote-merge/tests/contract/` に足す。既存のテストは、経路の部品の形の変更に合わせる以外は変えず、IR と違う動きを固めていたら IR に合わせて直し、記録に D 番号で残す（D14）
- 文書: `docs/remote-tools.md` の通信の版を 12 にする。`docs/ir/` の how_to_verify が指すテストの場所が変わったら付け替える。`PROJECT.md` の構成の説明を新しい置き場所に合わせる
- 今の実装のうち読まない・写さないもの: 下の各行に挙げる。ファイル名から当たりを付けたもので、中身は読んでいない。置き換えたら消すか、中身を作り直す
- 読まない・写さない（SSH のパスの確認）: `crates/remote-merge/src/runtime/remote_path/mod.rs`・`script.rs`・`output.rs`、`crates/remote-merge/src/runtime/path_route.rs`
- 読まない・写さない（SSH の子の一覧・項目のリンク先・大きさ・読み込み・ハッシュ）: `crates/remote-merge-ssh/src/child_listing.rs`・`entry_link.rs`・`batch_read.rs`・`batch_hash.rs`・`tree_parser.rs`・`find_records.rs`、`crates/remote-merge/src/runtime/remote_size.rs`
- 読まない・写さない（経路の操作と失敗の扱い）: `crates/remote-merge/src/runtime/remote_io.rs`・`target_io.rs`・`side_io.rs`・`agent_operations.rs`・`size_limit.rs`、`crates/remote-merge-engine/src/local_io.rs`、`crates/remote-merge-core/src/error.rs`・`operations.rs`・`listing.rs`・`entry_link.rs`
- 読まない・写さない（経路の選び方と接続断）: `crates/remote-merge/src/runtime/connection.rs`・`reconnect.rs`・`remote_target.rs`・`stopped_target.rs`・`scripted_agent.rs`
- 読まない・写さない（エージェント）: `crates/remote-merge-agent/src/agent/dispatch.rs`・`file_io.rs`・`client.rs`・`child_nodes.rs`・`listing.rs`・`entry_link.rs`・`server.rs`、`crates/remote-merge/src/agent/protocol.rs`
- 読まない・写さない（通信の型）: `crates/remote-merge-protocol/src/lib.rs`（読んでよいのは型の定義のシグネチャだけ）
- 読んでよいもの（段 1・段 2 で IR から書いた新しい土台の部品）: `crates/remote-merge-core/src/path_resolution/`（パスを辿る手順）、`crates/remote-merge-core/src/relative_path/`、`crates/remote-merge-core/src/scan/`、`crates/remote-merge-ssh/src/scan_*.rs`、`crates/remote-merge/src/runtime/scan_entry.rs`。

## Step order and prerequisites

S1 → S2 → S3 → S4 → S5 → S6 → S7 の順。S2 は S1 に依らない純粋な関数なので、S1 と並べてもよい。S3 は S1 の型を運ぶ。S4〜S6 は S1 の型を返す。S7 はすべてを使う。

## Verification map

| 要件・例 | 示す段 | 確かめ方 |
|---|---|---|
| REQ-paths-020、EX-paths-044・045、REQ-paths-016 | S1・S7 | 失敗の型と訳し方の単体テスト（OS の誤りを手で作って渡す）と、三つの経路で同じ種類になることの契約テスト |
| REQ-cli-079（上限を超える） | S1・S5 | 上限の判定の単体テストと、既存の大きさの上限の契約テスト |
| TBL-ssh-001、REQ-ssh-024・027、EX-ssh-056・059 | S2 | 選び方の関数の単体テスト（表の行ごと）と、sudo=true・`[agent] enabled` が false の契約テスト |
| REQ-ssh-010、EX-ssh-055 | S2・S3 | エージェントの読み込みが権限がないで失敗しても SSH に回さない契約テスト。既存の EX-ssh-034・035 のテスト |
| REQ-ssh-003・004・022・023、`接続断` の用語 | S3・S7 | 既存の接続断の契約テストと、エージェントの応答のフレームを壊したとき・SSH の答えの目印を欠いたときに接続断として扱う契約テスト |
| 通信の版 12（D9） | S3 | 通信の直列化した形を固定するテストと、版 11 のエージェントを使わない既存のテスト（REQ-ssh-021） |
| REQ-paths-008・009・012、EX-paths-019・020・025・043 | S4 | 既存の三つの経路の確認の契約テスト（`path_routes_agree.rs`）を新しい部品で動かし、検索できないディレクトリの下と、`ls -d` で理由を見分ける場合を足す |
| REQ-paths-013・017・018・019、EX-paths-026〜037・040〜042 | S5・S6 | 既存の三つの経路の操作の契約テスト（`path_operations_agree.rs` ほか） |
| TBL-ssh-002、REQ-ssh-025・026、EX-ssh-057・058 | S5・S6 | PATH を絞った sh と、-printf を受け付けない find を置いた PATH で、道具がないの失敗（誤りの文が要る find とエージェントで扱えることを含む）と代わりの道具を確かめる契約テスト |
| REQ-merge-038・040・041、FLAG-merge-021 | S5 | 既存のテストと how_to_verify の確かめ |
| REQ-merge-039、EX-merge-062 | S7 | 三つの経路でハードリンクのある既存のファイルに書き、inode・所有者・別の名前から読める中身を確かめる契約テスト |

## Left to the implementer

- 失敗の型の名前・形と、置き場所
- 経路の選び方の関数の名前と、入力の型の形
- SSH の答えの目印の書式（NUL で区切ること、問いと一つずつ突き合わせられること、形の壊れた答えを接続断にすることは守る）
- 一回の問いに載せるパスの数の分け方（コマンド行を 64KB を超えない長さに分けることは守る）
- 通信の型の形（版は 12 にする）
- テストが使う経路の部品の新しい形と、テストの呼び出しの直し方（確かめる中身は変えない）

## Stop conditions

- IR の文が二通りに読めて、どちらを選ぶかで利用者に見える振る舞いが大きく変わるとき
- 道具の限界で、三つの経路の結果を揃えられないと分かったとき。特に、SSH で "cannot read" と "permission denied" と存在しないことを `[ -x ]` と `ls -d` で見分けられない場合（D10）。揃えられない理由と、確かめた道具の版を添えて返す
- `remote_merge::cli::*` の `execute_*` の形か出力の型を変える必要があるとき
- 既存のテストを、経路の部品の形を合わせる以上に書き換えないと通らないとき（IR と違う動きを固めていたと説明できる場合を除く）

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

SSH の集め方とエージェントの通信を変えるので、最後に `scripts/run-container-e2e.sh` を走らせる。古い find（findutils 4.2.27）と BusyBox で確かめる手段があれば使う（`bench/legacy-ssh/` を参照）。

## Out of scope

- 比べる流れ（両側の状態 → 判定 → 必要な分だけ読む → もう一度判定）の作り直し（段 4）
- diff・status・merge・sync・rollback を新しい流れに載せ替えること（段 5）
- BusyBox の find だけのサーバを、エージェントなしで扱えるようにすること（D8 で断ると決めた）
- コマンドの入口の検査・新しいディレクトリの権限・diff のディレクトリの扱いの要件から「どの経路でも」を外すこと（D12。段 4・5）

## Steps

### S1: 操作の失敗の型と訳し方を IR から書く

- Purpose: 七つの種類の失敗の型、OS の誤りからの訳し方、大きさの上限の判定、失敗の文言の作り方を、一か所の純粋な関数にする。
- Specification: docs/ir/paths/operations.md#REQ-paths-020, docs/ir/paths/operations.md#REQ-paths-016, docs/ir/cli/size-limit.md#REQ-cli-079, docs/decision/records/2026-10-12-foundation-3.md#D1, docs/decision/records/2026-10-12-foundation-3.md#D2
- Prerequisites: なし
- May change: crates/remote-merge-core/src/
- Done when: 型・訳し方・上限の判定・文言の単体テストが、REQ-paths-020 の種類ごとに一つずつ確かめて通る。ローカルの経路の操作がこの型を返し、全テストが通る
- Shown by: test — 単体テスト（先に書いて落ちることを確かめてから実装する。印 `// @kotowari[REQ-paths-020]`）
- Left to the implementer: 型の名前と置き場所
- Stop and hand back if: 出力の型を変えないと文言を作れないとき

### S2: 経路の選び方を一つの関数にする

- Purpose: TBL-ssh-001 を一つの純粋な関数にし、サーバにつなぐときに一度だけ呼び、その後は選び直さない。
- Specification: docs/ir/ssh/fallback.md#TBL-ssh-001, docs/ir/ssh/fallback.md#REQ-ssh-024, docs/ir/ssh/fallback.md#REQ-ssh-027, docs/decision/records/2026-10-12-foundation-3.md#D19, docs/ir/ssh/fallback.md#REQ-ssh-010, docs/ir/ssh/fallback.md#REQ-ssh-005, docs/ir/ssh/fallback.md#REQ-ssh-021, docs/decision/records/2026-10-12-foundation-3.md#D3, docs/decision/records/2026-10-12-foundation-3.md#D4
- Prerequisites: なし
- May change: crates/remote-merge-core/src/ または crates/remote-merge-engine/src/、crates/remote-merge/src/runtime/
- Done when: 表の行ごとの単体テストと、EX-ssh-055・056・059 の契約テスト（EX-ssh-059 は警告が標準エラーに出て、標準出力の JSON が読めること）が通り、選び方の判断がほかの場所に残らず、全テストが通る
- Shown by: test — 単体テストと契約テスト（先に落ちることを確かめる）
- Left to the implementer: 入力の型の形
- Stop and hand back if: 表にない状態（どの行にも当たらない入力）が見つかったとき

### S3: 通信の版 12 で失敗の種類を運ぶ

- Purpose: エージェントの操作の応答が成功の値か失敗の種類を運び、クライアントがそれをそのまま S1 の型にする。読めない応答は接続断にする。
- Specification: docs/decision/records/2026-10-12-foundation-3.md#D9, docs/decision/records/2026-10-12-foundation-3.md#D2, docs/ir/CONTEXT.md, docs/ir/ssh/reconnect.md#REQ-ssh-023, docs/ir/ssh/fallback.md#REQ-ssh-021
- Prerequisites: S1
- May change: crates/remote-merge-protocol/src/、crates/remote-merge-agent/src/agent/、crates/remote-merge/src/agent/、crates/remote-merge/src/runtime/、docs/remote-tools.md
- Done when: 通信の版が 12 で、通信の直列化した形を固定するテストと、エージェントの応答のフレームを壊したときに接続断として一度だけ起動し直す契約テストが通り、全テストが通る
- Shown by: test — 直列化の形のテストと契約テスト
- Left to the implementer: 通信の型の形
- Stop and hand back if: 走査の応答（版 11 の形）を変える必要があるとき

### S4: SSH のパスの確認をまとめて尋ねる事実集めにする

- Purpose: SSH の確認を、段 1 の純粋な手順に、まとめて尋ねた要素の事実を渡す形にする。POSIX sh で辿る手順を消す。
- Specification: docs/ir/paths/routes.md, docs/ir/paths/resolution.md, docs/decision/records/2026-10-12-foundation-3.md#D5, docs/decision/records/2026-10-12-foundation-3.md#D10, docs/decision/records/2026-10-12-foundation-1.md#B4, docs/remote-tools.md
- Prerequisites: S1
- May change: crates/remote-merge-ssh/src/、crates/remote-merge/src/runtime/、crates/remote-merge/tests/contract/path_routes_agree.rs（部品の呼び方だけ）
- Done when: `path_routes_agree.rs` の三つの経路の確認（生成した symlink の状態を含む）と EX-paths-020・025・043 が新しい部品で通り、"cannot read" と "permission denied" の SSH の例外がなくなり、問いの回数が辿る symlink の段の数に一を足したものまでであることを確かめるテストが通り、古い確認のスクリプトが消える
- Shown by: test — 契約テスト（EX-paths-043 は先に落ちることを確かめる）
- Left to the implementer: 答えの目印の書式と、パスのまとめ方
- Stop and hand back if: `ls -d` と `[ -x ]` で理由を見分けられない状態が見つかったとき

### S5: SSH の大きさ・読み込み・ハッシュをまとめて尋ね、道具の有無を表で扱う

- Purpose: 大きさ・読み込み・ハッシュを複数のパスでまとめて尋ね、パスごとに成功か失敗の種類を返す。TBL-ssh-002 の道具がないときの扱いを入れる。
- Specification: docs/ir/ssh/compatibility.md#TBL-ssh-002, docs/ir/ssh/compatibility.md#REQ-ssh-025, docs/ir/ssh/compatibility.md#REQ-ssh-026, docs/ir/cli/size-limit.md#REQ-cli-079, docs/ir/ssh/reconnect.md#REQ-ssh-003, docs/ir/ssh/reconnect.md#REQ-ssh-022, docs/decision/records/2026-10-12-foundation-3.md#D7, docs/decision/records/2026-10-12-foundation-3.md#D2
- Prerequisites: S1
- May change: crates/remote-merge-ssh/src/、crates/remote-merge/src/runtime/
- Done when: EX-ssh-058 と、sha256sum も shasum もない・stat が -c を受け付けない・ls もない PATH での契約テスト、まとめた問いが接続断になったときにまとめのすべてのパスを同じ失敗にするテスト、既存の大きさの上限と接続断の契約テストが通り、古いまとめ読み・まとめハッシュ・大きさの実装が消える
- Shown by: test — 契約テスト（先に落ちることを確かめる）
- Left to the implementer: まとめる単位の分け方
- Stop and hand back if: 代わりの道具の答えがローカルと揃わないとき

### S6: 子の一覧を走査の集め方に載せる

- Purpose: 子の一覧を、段 2 の走査の集め方を深さ一つにしたものと同じ規則で作る。子の一覧のための find の書き方を消す。
- Specification: docs/ir/paths/operations.md#REQ-paths-017, docs/ir/paths/operations.md#REQ-paths-013, docs/ir/ssh/compatibility.md#REQ-ssh-026, docs/decision/records/2026-10-12-foundation-3.md#D6, docs/decision/records/2026-10-12-foundation-2.md#C6
- Prerequisites: S1
- May change: crates/remote-merge-core/src/scan/、crates/remote-merge-ssh/src/、crates/remote-merge-agent/src/agent/、crates/remote-merge-engine/src/、crates/remote-merge/src/runtime/
- Done when: EX-paths-033〜037 と EX-ssh-057（-printf を受け付けない find を置いた PATH で、走査と子の一覧が道具がないで止まり、誤りの文が要る find とエージェントで扱えることを含む）の契約テストが通り、子の一覧のための SSH の実装とエージェントの一覧の実装が消え、全テストが通る
- Shown by: test — 契約テスト
- Left to the implementer: 深さ一つの指定の仕方
- Stop and hand back if: 子の一覧の結果を走査の規則で作れない要件が見つかったとき

### S7: 三つの経路の操作が同じ種類で失敗し、同じく書くことを見張り、片付ける

- Purpose: 経路の差が新しく入ったら落ちるテストを置き、書き込みをどの経路もその場で書き換える形に揃え、置き換えた古い実装と印を片付ける。
- Specification: docs/ir/paths/operations.md#REQ-paths-013, docs/ir/paths/operations.md#REQ-paths-020, docs/ir/merge/concurrency.md#REQ-merge-039, docs/ir/ssh/reconnect.md, docs/decision/records/2026-10-12-foundation-3.md#D12, docs/decision/records/2026-10-12-foundation-3.md#D18
- Prerequisites: S2, S3, S4, S5, S6
- May change: crates/remote-merge/tests/contract/、crates/remote-merge-engine/src/ と crates/remote-merge-agent/src/agent/ の書き込み（EX-merge-062 が落ちたときだけ）、`docs/ir/` の how_to_verify のテストの場所、PROJECT.md、上の「読まない・写さないもの」のうち残ったもの
- Done when: EX-paths-044・045 の契約テスト（ない・権限がない・ディレクトリでないを三つの経路で読み・削除・権限の変更・大きさ・ハッシュに当てる）と、SSH の答えの目印を欠いたときに接続断として扱う契約テストと、EX-merge-062 の契約テスト（三つの経路でハードリンクのある既存のファイルに書き、inode と所有者が変わらず、別の名前から新しい中身が読める）が通る。`kotowari check` の error が 0、コンテナの e2e が通る
- Shown by: test — 契約テストと `kotowari check`
- Left to the implementer: 状態の作り方
- Stop and hand back if: 道具の限界で揃えられない項目があるとき
