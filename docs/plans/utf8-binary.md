# Plan: ファイル全体の UTF-8 の正しさでテキストとバイナリを分ける

## Goal

先頭 8,192 バイトより後ろにだけ不正な UTF-8 のバイトがあるファイルが、diff では SHA-256 のハッシュで比べるバイナリとして示され、--hunks の merge ではバイナリとして止まる。置換文字に置き換えた中身で「差分なし」と判定したり、選ばなかった部分を書き換えたりしなくなる。

## Specification

IR は `docs/ir/` にある。この計画が扱うのは次のとおり。

- `docs/ir/cli/binary.md#REQ-cli-061`（テキストとバイナリの判定。根拠は `docs/decision/records/2026-10-05-quality-cli.md#A8`）
- `docs/ir/merge/hunks.md#REQ-merge-028` と、その表 TBL-merge-001 の「読み込み元か書き込み先がバイナリ」の行
- Examples: EX-cli-079、EX-cli-080

## Approach and why

判定の規則は core の `crates/remote-merge-core/src/diff/engine.rs` の `is_binary` 一か所にある。いまは先頭 8,192 バイトだけを UTF-8 として確かめるので、後ろの不正なバイトを見逃す。また、8,192 バイトの境目で切れた多バイトの文字を不正とみなしてしまう（EX-cli-080 に反する）。

diff（`crates/remote-merge/src/cli/diff.rs`）も hunk merge（`crates/remote-merge-engine/src/service/merge_flow.rs`）も、バイト列のまま `is_binary` を呼び、テキストと判定したときだけ `String::from_utf8_lossy` で文字列にしている。だから規則を一か所直せば、テキストとして扱われる中身は常に正しい UTF-8 になり、置換は起きなくなる。呼び出し側に別の判定を足さない。

S1 で規則を直し、diff と hunk merge の両方で振る舞いを確かめる。

S2 では、どの製品コードからも呼ばれていない文字列の読み込み（ローカルと SSH は置換文字に置き換え、エージェントはエラーにする経路差を持つ）を消す。これは S1 とは独立した片付けで、振る舞いは変えない。経路差を持つ未使用の入口を残すと、後で誰かが使ったときに経路差が表に出るので、ここで消す。

## Scope of change

- `crates/remote-merge-core/src/diff/engine.rs`
  - `is_binary` とその単体テスト
- `crates/remote-merge/tests/contract/diff_file_kinds.rs`（EX-cli-079・EX-cli-080 の契約テスト）
- `crates/remote-merge/tests/contract/merge_hunks.rs`（hunk merge の契約テスト）
- S2 だけが触るもの: 文字列を返す読み込みの入口とその実装
  - `CoreRuntime::read_file`・`read_files_batch`（`crates/remote-merge/src/runtime/side_io.rs`）
  - `TargetIo` の同名のメソッド（`crates/remote-merge/src/runtime/target_io.rs`）
  - SSH の `read_file`・`read_files_batch`（`crates/remote-merge-ssh/src/client.rs`、`crates/remote-merge/src/runtime/remote_io.rs`）
  - エージェントの文字列の読み込み（`try_agent_read_file`・`try_agent_read_files_batch`）
  - engine の `local_io::read_file`・`read_files_batch` と `merge/executor.rs` の `read_local_file`、およびそれらだけを使うテスト

## Step order and prerequisites

S1 と S2 は互いに独立している。S1 を先にする。S1 は利用者に見える振る舞いを変え、S2 は消すだけだからである。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-cli-061、REQ-merge-028（TBL-merge-001 のバイナリの行） | EX-cli-079、EX-cli-080 |
| S2 | なし（使われていない入口の削除。振る舞いを変えない） | なし |

## Left to the implementer

- `is_binary` の中の書き方（全体を `std::str::from_utf8` で確かめるなど）
- 契約テストの fixture の作り方と、テストの名前

## Stop conditions

- `is_binary` を経ずに、バイト列を `from_utf8_lossy` でテキストとして比べたり書き戻したりする製品コードの経路が、diff と hunk merge のほかに見つかった（参照先の中身は Out of scope のとおり除く）
- 判定を全体に広げたことで、既存のテストが REQ-cli-061 と矛盾する振る舞いを固定していることが分かった。直す前に報告する
- 既存のテストが今回の変更と関係のない理由で失敗した

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

## Out of scope

- 参照先（--ref）の中身の扱い。参照先だけがバイナリのときの ref_hunks と競合の扱いは、未決の FLAG-cli-059・FLAG-cli-060・FLAG-cli-064 に属する。後ろにだけ不正なバイトがある参照先を文字列に置き換えて ref_hunks や競合を求める現状は、この計画では変えない。品質の台帳に後の単位として残す
- 大きいファイルで判定のために全体を読む性能（A8 のトレードオフとして受け入れ済み）
- status の比較（バイト列とハッシュで比べ、テキストの判定を使わない）

## Steps

### S1: ファイル全体の UTF-8 の正しさでバイナリを判定する

- Purpose: REQ-cli-061 の判定規則を `is_binary` に実装し、diff と hunk merge で確かめる
- Specification: `docs/ir/cli/binary.md#REQ-cli-061`, `docs/ir/merge/hunks.md#REQ-merge-028`
- Prerequisites: none
- May change: `crates/remote-merge-core/src/diff/engine.rs`, `crates/remote-merge/tests/contract/diff_file_kinds.rs`, `crates/remote-merge/tests/contract/merge_hunks.rs`
- Done when: 先頭 8,192 バイトが正しい UTF-8 で後ろにだけ不正なバイトがあり、そのバイトだけが左右で違うファイルを diff で比べると、binary と左右で違う SHA-256 のハッシュが出て差分ありになる。全体が正しい UTF-8 で 8,192 バイトの境目で多バイトの文字が切れるファイルは、テキストとして行の差分が出る。前者のファイルを読み込み元にした --hunks の merge は "Hunk merge is not supported for binary files: 'パス'" で止まって終了コード 2 を返し、書き込み先は変わらない。NUL を先頭 8,192 バイトより後ろにだけ持つファイルの既存の扱い（テキスト）は変わらない
- Shown by: test — 契約テスト `// @kotowari[EX-cli-079]`（diff で後ろの不正なバイトだけが違うファイルがバイナリとして違うと示される）、`// @kotowari[EX-cli-080]`（境目で切れた多バイトの文字のファイルがテキストとして比べられる）、`// @kotowari[REQ-merge-028]`（後ろにだけ不正なバイトがあるファイルの --hunks の merge がバイナリとして止まり、書き込み先が変わらない）。どれも変更前の HEAD で RED になることを確かめる
- Left to the implementer: `is_binary` の中の書き方、fixture の作り方、テストの名前
- Stop and hand back if: 既存の単体テストか契約テストが、先頭 8,192 バイトだけで UTF-8 を判定する振る舞い（境目で切れた文字をバイナリにするなど）を REQ-cli-061 の根拠つきで固定していて、直すと REQ-cli-061 と矛盾する場合

### S2: 使われていない文字列の読み込みの入口を消す

- Purpose: 経路ごとに不正な UTF-8 の扱いが違う、製品コードから呼ばれていない文字列の読み込みを取り除く
- Specification: なし（What the IR holds の外の片付け。利用者に見える振る舞いを変えない）
- Prerequisites: S1
- May change: Scope of change の「S2 だけが触るもの」に挙げたファイルと、それらの関数だけを使うテスト
- Done when: 文字列を返す読み込みの入口（ローカル・SSH・エージェント）が製品コードからなくなり、バイト列の読み込み（`read_file_bytes`・`read_files_bytes_batch` とその経路）は変わらず、全テストが通る
- Shown by: check — `rg -n "fn read_file\b|fn read_files_batch\b|fn read_local_file\b|fn try_agent_read_file\b|fn try_agent_read_files_batch\b" crates --type rust` が何も返さないこと、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo clippy --all-targets --all-features -- -D warnings`、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features`
- Left to the implementer: 消す順番、エージェントの通信の形（protocol）に文字列の読み込みの要求があればそれを消すかどうか（エージェントと本体の版の互換を壊さない場合に限る）
- Stop and hand back if: 文字列の読み込みに製品コードの呼び出し元（テストを除く）が見つかった。エージェントの通信の形から要求を消すと、配置済みのエージェントとの互換が壊れる
