# Plan: このツールの古い版との互換の分岐を捨てる

## Goal

以前の版の設定のキー・--ref の別名・以前の識別で保存したバックアップのセッション・書き込み先の ".remote-merge-backup/" を特別に扱う分岐がなくなり、知らないキーやセクションのある設定はエラーで止まる。

## Specification

IR は `docs/ir/` にある。決定は `docs/decision/records/2026-10-11-drop-compat.md` の A1〜A6 と P1・P2。もとの方針は `docs/decision/records/2026-10-11-agent-operations.md` の A3（互換の分岐はすべて捨てる）と A4（サーバの道具の違いを吸収する分岐は残す）。この計画が扱うのは次のとおり。

- 知らないキーとセクションのエラー: `docs/ir/config/loading.md#REQ-config-039`（例 EX-config-015・EX-config-021・EX-config-022）
- ".remote-merge-backup/" をほかのディレクトリと同じに扱う: `docs/ir/config/filters.md#REQ-config-003`（例 EX-config-023）
- 今の識別だけでセッションを探す: `docs/ir/backup/sessions.md#REQ-backup-021`（例 EX-backup-023）
- IR に書かない決定で、この計画が実装するもの: A6（--reference を捨てる）と、A3 の「コードの中の互換の名残」

仕様のコミットで IR から消した要件と例は REQ-config-030・REQ-config-031・REQ-config-042・REQ-backup-043・REQ-backup-016・EX-backup-024・EX-backup-028 で、これらの ID を指す印 `// @kotowari[...]` が契約テストに残っている（`kotowari check` の unresolved_reference）。その印の付いたテストは、消した振る舞いを確かめるものなので、テストごと消す。

## Approach and why

設定、バックアップ、CLI と名残、の順にする。三つは互いの影響がないが、設定の変更（知らないキーをエラーにする）は、テストや雛形の設定ファイルに残った古いキーを全部表に出すので、最初に片付けて後の段の試験を安定させる。

設定（S1）。今の `crates/remote-merge-config/src/lib.rs` の `RawConfig` と各 `Raw*Config` は `Deserialize` だけで、知らないキーは serde の既定の動きで黙って捨てられている。REQ-config-039 はまだどこにも実装されていない。五つの古いキー（`RawConfig.badge_scan_max_files`、`RawFilterConfig.sensitive`、`RawAgentConfig` の `timeout_secs`・`tree_chunk_size`・`max_file_chunk_bytes`）のフィールドと、`specifies_sensitive`・`specifies_badge_scan_max_files`・二つの警告の定数・`load_config_from_paths` の警告を消す。`[agent]` の三つのキーの警告が今どこで出ているかは、同じファイルの中を読んで消す。設定の構造体（`lib.rs` の 177 行目付近の `timeout_secs` など）に、古いキーのためだけに残った値があれば消す。エージェントの配置の内部の値（`crates/remote-merge-agent/src/agent/deploy/mod.rs` の `timeout_secs`）は設定のキーではないので触らない。
知らないキーとセクションは、グローバル設定とプロジェクト設定のどちらでも、読み込みの段でエラーにして終了コード 2 で止める。比較は始めない。エラーの文言は英語で書き、テストは文言を固定しない（REQ-config-039）。
キーを知らないものにすると、契約テスト・e2e・`init` の雛形・テストの中の TOML に古いキーや書き間違いが残っていれば、そのテストが落ちる。落ちたものは、テストの意図を変えずに古いキーを消して直す。

バックアップ（S2）。以前の識別で探す分岐を消す。`crates/remote-merge-config/src/lib.rs` の `LocalConfig.root_dir_as_written`、`crates/remote-merge-engine/src/backup/mod.rs` の `previous_local_target_identity`、`crates/remote-merge-engine/src/backup_store.rs` の `target_keys`・`configured_target_keys`・`session_records_dir` のうち以前の識別を足す部分、そこからしか呼ばれない `lexically_normalized` が対象になる。`target_keys` が一つしか返さなくなったら、並びを持つ形をやめてよい。
`BackupStore.startup_directory` と `crates/remote-merge/src/runtime/mod.rs` の受け渡しは、今の識別（`local_target_identity`）も使っている。相対の root_dir は設定の読み込みで設定ファイルの場所を基準に絶対パスにされるので、今の識別で相対の枝に入るのは `LocalConfig::default()` か手で作った値のときだけである。残すか消すかは S2 の実装者に任せるが、今の識別の値は変えない。
EX-backup-023（"/srv/app/" と "/srv/app" が同じ書き込み先）は、これまで以前の識別の探し方でも拾われていた可能性がある。調べでは、リモートの今の識別は `config.root_dir` の `display()` をそのまま使っていて、末尾の "/" を除いているかを確かめていない。ローカルの絶対パスの root_dir も `resolve_local_root` が正規化せずに返す。EX-backup-023 の契約テストを先に書き、今の識別が REQ-backup-021 の字面の正規化を満たしていなければ、今の識別を直す。これは承認済みの REQ-backup-021 を満たすための直しで、仕様は変えない。

".remote-merge-backup/"（S3）。`merge_configs` の「バックアップディレクトリは常に除外」の段、`BACKUP_DIR_NAME` の定義（`crates/remote-merge-config/src/lib.rs` の 4 行目）と、使う側のない `crates/remote-merge-engine/src/backup/mod.rs` の再公開、設定の単体テスト `test_backup_dir_always_in_exclude`・`test_backup_dir_no_duplicate_when_user_specifies` を消す。REQ-backup-016 の契約テスト（`crates/remote-merge/tests/contract/backup_storage.rs` の `legacy_backup_directory_is_left_alone_and_hidden_from_list_and_status`）は、EX-config-023 のテストに置き換える。`crates/remote-merge-core/src/filter.rs` の `test_segment_pattern_matches_each_segment` はこの名前をパターンの例に使っているだけなので、残してよい。

CLI と名残（S4）。`crates/remote-merge/src/main.rs` の三つの `#[arg(long, alias = "reference")]` から別名を外す。次の名残も消す。

- `crates/remote-merge-engine/src/local/mod.rs` の `MAX_DIR_ENTRIES`（「後方互換エイリアス」）。使う側を `DEFAULT_MAX_DIR_ENTRIES` に移す
- `crates/remote-merge-ssh/src/known_hosts.rs` の「後方互換」の再公開 `append_known_hosts_entry`。使う側（`crates/remote-merge-ssh/src/client.rs`）を `known_hosts_io` から直接使う形に移す
- `crates/remote-merge-engine/src/service/types.rs` の `test_status_summary_backward_compat_deserialize`（古い出力の JSON を読めることを確かめるテスト）と、`MergeOutput.deleted`・`SyncTargetResult.deleted` の `#[serde(default)]`。この二つの型を JSON から読む本番のコードはないので、`deleted` のない JSON を読むためだけの既定値である。消すとほかのテストが読めなくなるなら、そのテストの JSON に `deleted` を足す

利用者向けの文書 `skills/remote-merge/SKILL.md` に、`[agent]` の `timeout_secs` の例（291 行目付近）と、古いキーを警告して読み飛ばす説明（301 行目付近）が残っている。消したキーを書くと止まることに合わせて直す。

名前に backward_compat が付いた残りのテスト（`--ref` なしの出力や `deleted` が空の出力を確かめるもの）は、今の機能を確かめているので消さない。名前を変えるかは実装者に任せる。

## Scope of change

- `crates/remote-merge-config/src/lib.rs`（設定の読み込み、古いキー、知らないキーのエラー、`BACKUP_DIR_NAME`、`root_dir_as_written`）
- `crates/remote-merge-engine/src/backup/mod.rs`、`crates/remote-merge-engine/src/backup_store.rs`、必要なら `crates/remote-merge/src/runtime/mod.rs`・`crates/remote-merge/src/runtime/core.rs`（以前の識別と `startup_directory`）
- `crates/remote-merge/src/main.rs`（--reference）
- `crates/remote-merge-engine/src/local/mod.rs`・`crates/remote-merge-engine/src/local_io.rs`、`crates/remote-merge-ssh/src/known_hosts.rs`・`crates/remote-merge-ssh/src/client.rs`、`crates/remote-merge-engine/src/service/types.rs`（名残）
- 契約テスト `crates/remote-merge/tests/contract/config_loading_cli.rs`・`backup_sessions.rs`・`backup_storage.rs`、新しい契約テストのファイル（置く場所は実装者に任せる）
- 古いキーが残っていて落ちるテストや雛形の TOML（`crates/` と `tests/container-e2e/` の中）
- `skills/remote-merge/SKILL.md`

## Step order and prerequisites

S1 を最初にする。知らないキーのエラーで落ちるテストを先に直しておくと、S2〜S4 の試験が設定の読み込みで落ちなくなる。S2・S3・S4 は互いに依存しないが、この順に一つずつコミットする。S5 は最後に全体を確かめる。

## Verification map

| 要件・例 | 示す段 | 確かめ方 |
|---|---|---|
| REQ-config-039、EX-config-015 | S1 | 契約テスト: プロジェクト設定の [filter] に "inclde" があると、比較を始めずに終了コード 2 |
| EX-config-021 | S1 | 契約テスト: グローバル設定の [filter] に sensitive があると終了コード 2 |
| EX-config-022 | S1 | 契約テスト: プロジェクト設定の [agent] に timeout_secs があると終了コード 2 |
| REQ-backup-021、EX-backup-023 | S2 | 契約テスト: root_dir "/srv/app/" の設定で保存したセッションが、"/srv/app" に書き換えた設定の rollback --list に出る |
| REQ-config-003、EX-config-023 | S3 | 契約テスト: 書き込み先の ".remote-merge-backup/old.txt" が exclude なしの status の一覧に出る |
| A6、名残 | S4 | `rg` で別名と名残が残っていないこと、全テストが通ること |

## Left to the implementer

- 知らないキーの検出の作り方（`#[serde(deny_unknown_fields)]` か、読み込んだ表とキーの一覧を比べる形か）。どちらでも、知らないキーとセクションで終了コード 2 で止まり、知っているキーの読み方は変わらない
- エラーの英語の文言。どのファイルのどのキーかが分かることが望ましいが、テストは文言を固定しない
- `startup_directory` の受け渡しを残すか消すか（今の識別の値は変えない）
- 新しい契約テストの置き場所と名前、`target_keys` などの関数の形

## Stop conditions

- 知らないキーをエラーにすると、IR に書かれた知っているキー（`docs/ir/config/` の TBL-config-001 や各要件のキー）のどれかまで読めなくなるとき
- EX-backup-023 を満たすために、今の識別の決め方を REQ-backup-021 に書かれていない形で変える必要があるとき
- 名残として挙げたものが、調べと違ってサーバの道具の違いを吸収する分岐（agent-operations A4）に当たると分かったとき

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

## Out of scope

- サーバの道具の違いを吸収する分岐（ln -s -T がないとき、dd の oflag=nofollow や status=none がないとき、コマンド長の上限、古い SSH のアルゴリズムの設定、sha256sum がないときの shasum、fish のログインシェル。agent-operations A4・drop-compat P2）
- 集約先に残った以前の識別のセッションを消したり移したりすること（drop-compat A4・P1）
- 書き込み先に残った ".remote-merge-backup/" を消したり移したりすること
- 通信の版（agent-operations の単位で 6 にして、古い版の分岐は消してある）

## Steps

### S1: 知らないキーとセクションで止まり、古いキーの警告がなくなる

- Purpose: 設定に製品の知らないキーかセクションがあればエラーで止め、五つの古いキーも知らないキーとして扱う。
- Specification: docs/ir/config/loading.md#REQ-config-039
- Prerequisites: なし
- May change: crates/remote-merge-config/src/lib.rs、crates/remote-merge/tests/contract/config_loading_cli.rs、新しい契約テストのファイル、古いキーや知らないキーが残っていて落ちるテストと雛形の TOML（crates/ と tests/container-e2e/ の中）、skills/remote-merge/SKILL.md
- Done when: EX-config-015・EX-config-021・EX-config-022 の契約テストが通り、REQ-config-030・REQ-config-031・REQ-config-042 の印の付いたテストと警告の定数がなくなり、全テストが通る
- Shown by: test — EX-config-015・EX-config-021・EX-config-022 の契約テスト（印 `// @kotowari[REQ-config-039, EX-config-0nn]`）。RED を確かめてから実装する
- Left to the implementer: 検出の作り方、エラーの文言、テストの名前
- Stop and hand back if: 知らないキーをエラーにすると IR に書かれた知っているキーまで読めなくなるとき

### S2: バックアップのセッションを今の識別だけで探す

- Purpose: 以前の識別で探す分岐を消し、"/srv/app/" と "/srv/app" が今の識別で同じ書き込み先になることを確かめる。
- Specification: docs/ir/backup/sessions.md#REQ-backup-021
- Prerequisites: S1
- May change: crates/remote-merge-config/src/lib.rs、crates/remote-merge-engine/src/backup/mod.rs、crates/remote-merge-engine/src/backup_store.rs、crates/remote-merge/src/runtime/mod.rs、crates/remote-merge/src/runtime/core.rs、crates/remote-merge/tests/contract/backup_sessions.rs
- Done when: EX-backup-023 の契約テストが通り、REQ-backup-043 と EX-backup-024 の印の付いたテストと `root_dir_as_written`・`previous_local_target_identity` がなくなり、全テストが通る
- Shown by: test — EX-backup-023 の契約テスト（印 `// @kotowari[REQ-backup-021, EX-backup-023]`）。リモートとローカルの両方の root_dir で末尾の "/" を変える
- Left to the implementer: `startup_directory` の受け渡しを残すか、`target_keys` などの関数の形
- Stop and hand back if: EX-backup-023 を満たすために、今の識別の決め方を REQ-backup-021 に書かれていない形で変える必要があるとき

### S3: ".remote-merge-backup/" をほかのディレクトリと同じに扱う

- Purpose: 設定の exclude に常に足していた ".remote-merge-backup" をやめ、書き込み先のそのディレクトリを走査と比較の対象にする。
- Specification: docs/ir/config/filters.md#REQ-config-003
- Prerequisites: S1
- May change: crates/remote-merge-config/src/lib.rs、crates/remote-merge-engine/src/backup/mod.rs、crates/remote-merge/tests/contract/backup_storage.rs、新しい契約テストのファイル
- Done when: EX-config-023 の契約テストが通り、`BACKUP_DIR_NAME` と REQ-backup-016 の印の付いたテストがなくなり、全テストが通る
- Shown by: test — EX-config-023 の契約テスト（印 `// @kotowari[REQ-config-003, EX-config-023]`）
- Left to the implementer: テストの置き場所と名前
- Stop and hand back if: ".remote-merge-backup" を除くことに頼っている今のバックアップの処理が見つかったとき

### S4: --reference の別名とコードの中の互換の名残を消す

- Purpose: --ref の別名 --reference と、Approach に挙げた名残を消す。
- Specification: docs/decision/records/2026-10-11-drop-compat.md#A6
- Prerequisites: S1
- May change: crates/remote-merge/src/main.rs、crates/remote-merge-engine/src/local/mod.rs、crates/remote-merge-engine/src/local_io.rs、crates/remote-merge-ssh/src/known_hosts.rs、crates/remote-merge-ssh/src/client.rs、crates/remote-merge-engine/src/service/types.rs
- Done when: `rg -n 'alias = "reference"|MAX_DIR_ENTRIES\b|backward_compat_deserialize' crates` が `DEFAULT_MAX_DIR_ENTRIES` 以外を返さず、全テストが通る
- Shown by: check — `rg -n 'alias = "reference"|backward_compat_deserialize' crates` が何も返さないこと、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features` が通ること
- Left to the implementer: 名前に backward_compat が付いた残りのテストの名前を変えるか
- Stop and hand back if: 名残として挙げたものが、サーバの道具の違いを吸収する分岐に当たると分かったとき

### S5: 計画の要件と例に印が付き、計画の範囲に kotowari check のエラーがない

- Purpose: 計画の要件と例のすべてに印の付いたテストがあり、消した ID を指す印が残っていないことを確かめる。
- Specification: docs/ir/config/loading.md#REQ-config-039, docs/ir/config/filters.md#REQ-config-003, docs/ir/backup/sessions.md#REQ-backup-021
- Prerequisites: S1、S2、S3、S4
- May change: 印の付け直しに要るテストのファイルだけ
- Done when: `kotowari query` で REQ-config-039・EX-config-015・EX-config-021・EX-config-022・EX-config-023・EX-backup-023 の tests が空でなく、`kotowari check --format json` に unresolved_reference がなく、このブランチが変えたファイルにエラーがない
- Shown by: check — `kotowari query <ID> | jq '.items[0].tests'` を各 ID について、`kotowari check --format json | jq '[.findings[] | select(.kind=="unresolved_reference")] | length'` が 0、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features` が通ること
- Left to the implementer: none
- Stop and hand back if: 計画の範囲の外の ID の findings しか残っていないのに、範囲のものが消えないとき
