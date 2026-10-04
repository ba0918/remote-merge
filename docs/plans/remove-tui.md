# Plan: TUI を取り除く

## Goal

remote-merge が CLI 専用のツールになり、TUI のコード・テスト・仕様・依存がなくなり、引数なしの起動は使い方を示して止まり、CLI の実行が診断ログを残して logs で読める。

## Specification

IR は `docs/ir/`、根拠の決定は `docs/decision/records/2026-10-04-tui-disposition.md`（A1〜A21）。この計画が扱う要件と例:

- 新しい要件: `docs/ir/cli/safety.md#REQ-cli-076`、`docs/ir/cli/diagnostics.md#REQ-cli-075`、`docs/ir/config/filters.md#REQ-config-031`
- 変更した要件: `docs/ir/cli/diagnostics.md#REQ-cli-014`、`docs/ir/cli/diagnostics.md#REQ-cli-015`、`docs/ir/backup/sessions.md#REQ-backup-025`、`docs/ir/config/precedence.md#REQ-config-027`、`docs/ir/config/loading.md#REQ-config-028`、`docs/ir/config/defaults.md#REQ-config-013`
- 変更した例・表: EX-cli-030、EX-ssh-001、EX-testing-001、EX-testing-002、TBL-config-001、TBL-config-003
- 確認手段のない接続を扱う既存の要件と例（is_tui を除くときに守る）: `docs/ir/ssh/host-key.md#REQ-ssh-001`、`docs/ir/ssh/host-key.md#REQ-ssh-002`、EX-ssh-001、EX-ssh-004
- 仕様から除いた要件と例（実装とテストから取り除く対象。テストを求めない）: REQ-tui-001〜008、EX-tui-001〜018、REQ-scan-001、EX-scan-001、EX-scan-002、REQ-backup-020、EX-cli-028、REQ-merge-010、EX-merge-019、EX-merge-020

## Approach and why

製品 crate の TUI（app・handler・ui・highlight・theme・state・format と runtime・telemetry の TUI 部分）は CLI からほとんど使われていない。CLI が TUI のモジュールから使っているのは `crate::app::Side`（実体は engine の `side` の再公開）と、`telemetry::state_dumper::default_dump_dir`（診断ログの置き場）だけである。そこで最初に、この二つの参照先を TUI でない場所へ移す。続いて、TUI がなくなると失われる振る舞い（診断ログの保存、確認手段のない SSH 接続の拒否）を CLI 側に用意し、テストで確かめてから、TUI の入口と実行時処理とテストを一度に除く。依存・設定・下位 crate の残り・文書はその後に片付ける。

contract の `tui_*` モジュールは cfg で囲まれておらず、TUI の型を直接 import しているので、TUI のコードと同じステップで消さないと契約テストのバイナリがビルドできない。そのため S4 は大きいが一つのステップにする。

除いた要件の ID を印に持つテストは消す。残す要件の唯一のテストが TUI のテストだった場合（EX-cli-030、EX-ssh-001・004 の is_tui の使い方）は、先に CLI のテストに置き換える。

## Scope of change

- `crates/remote-merge/src/`（main.rs、lib.rs、app、handler、ui、highlight、theme、state.rs、format、runtime、telemetry、cli、service、config.rs、local、merge）
- `crates/remote-merge/Cargo.toml`（TUI だけの依存と dev 依存）
- `crates/remote-merge/tests/`（tui_*.rs、common/、contract/ の TUI 部分と除いた ID のテスト、診断ログと SSH ホスト鍵のテスト）
- `crates/remote-merge-config/src/lib.rs`（badge_scan_max_files）
- `crates/remote-merge-settings/src/lib.rs`、`crates/remote-merge-ssh/src/client.rs`、`crates/remote-merge-ssh/src/host_key_verifier.rs`（is_tui）
- `crates/remote-merge-engine/src/`、`crates/remote-merge-core/src/` のうち TUI だけが使っていた項目
- `README.md`、`skills/remote-merge/SKILL.md`、`skills/remote-merge/references/json-schemas.md`、`PROJECT.md`、`scripts/mutants.sh`
- `.kotowari/changes/implementation.yaml`（実装者が書く。`review.yaml` は独立したレビューが書き、この計画のステップでは書かない）

## Step order and prerequisites

S1 → S2 → S3 → S4 → S5 → S6 → S7 → S8 → S9 の順に一つずつ進める。S1 で CLI が TUI のモジュールを参照しなくなり、S2・S3 で TUI がなくなると失われる振る舞いが CLI 側で確かめられてから、S4 で TUI を除く。S5〜S7 は S4 で利用者がいなくなったものを片付ける。S8 は文書、S9 は全体の確認。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | なし（参照先の付け替え。既存のテストが通ること） | なし |
| S2 | REQ-cli-075, REQ-cli-014, REQ-cli-015 | EX-cli-027, EX-cli-029, EX-cli-030 |
| S3 | REQ-ssh-001, REQ-ssh-002 | EX-ssh-001, EX-ssh-004 |
| S4 | REQ-cli-076, REQ-backup-025 | なし |
| S5 | なし（依存の削除。既存のテストが通ること） | なし |
| S6 | REQ-config-031, REQ-config-013, REQ-config-027 | なし |
| S7 | なし（残りの削除。既存のテストが通ること） | なし |
| S8 | なし（決定 A11 の文書） | なし |
| S9 | この計画の新しい要件と変更した要件 | EX-testing-001, EX-testing-002 |

## Left to the implementer

- `Side` と診断ログの置き場の関数をどこへ移すか（振る舞いが変わらない範囲）
- 引数なしの起動を clap の機能で止めるか、`None` の分岐で止めるか。REQ-cli-076 の出力先と終了コードは守る
- 診断ログの書き方（既存の `JsonLogLayer` を CLI でも使うなど）。レベル（A16）、開けないときの扱い（A17）、10MB の上限と切り詰め（A18）、REQ-cli-015 の内容の規則は守る
- 使われなくなった関数・型・再公開・テスト補助をどこまで消すか

## Stop conditions

- CLI が TUI のモジュールから、`Side` と診断ログの置き場以外の振る舞いを使っていて、付け替えでは済まない
- 診断ログを CLI で保存すると、標準出力の JSON（REQ-cli-018）や標準エラーの既存の出力が変わる
- is_tui を除くと、確認できる場面（対話の CLI）で未知のホスト鍵の確認の挙動が変わる
- 除くために engine・agent・protocol の通信の形を変える必要が出る
- 除いた ID を印に持つテストの中に、残す要件の唯一のテストが含まれていて、置き換えられない

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G cargo nextest run --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

## Out of scope

- Web（WebView 方式）を作るかどうか（決定の U7、未決）
- 判断待ちの FLAG のうち TUI に関わらないもの
- 版を上げることとリリース（決定 A10 の版の変更はリリースのときに行う）
- 製品 crate の Cargo.toml にある、TUI と関係なく使われていないように見える依存の整理

## Steps

### S1: CLI が参照する Side と診断ログの置き場を TUI の外へ移す

- Purpose: CLI と共有テストが `crate::app` と `telemetry::state_dumper` を参照しないようにする
- Specification: `docs/decision/records/2026-10-04-tui-disposition.md#A1`
- Prerequisites: none
- May change: `crates/remote-merge/src/lib.rs`, `crates/remote-merge/src/cli/`, `crates/remote-merge/src/service/`, `crates/remote-merge/src/runtime/core.rs`, `crates/remote-merge/src/runtime/target_io.rs`, `crates/remote-merge/src/runtime/side_io.rs`, `crates/remote-merge/src/telemetry/`, `crates/remote-merge/tests/contract/`
- Done when: `crates/remote-merge/src/cli`、`src/service`、`src/runtime/{core,target_io,side_io,remote_io,remote_path}.rs` と、TUI のテストを含まない共有テストに `crate::app` / `remote_merge::app` と `state_dumper` の参照がなく、ワークスペースのテストと clippy が通る。TUI のテストと混ざっている backup_failure.rs と backup_sessions.rs の TUI 部分は S4 で除くので対象外とする
- Shown by: check — `rg -n "crate::app|remote_merge::app|state_dumper" crates/remote-merge/src/cli crates/remote-merge/src/service crates/remote-merge/src/runtime/core.rs crates/remote-merge/src/runtime/target_io.rs crates/remote-merge/src/runtime/side_io.rs crates/remote-merge/src/runtime/remote_io.rs crates/remote-merge/src/runtime/remote_path.rs crates/remote-merge/tests --glob "!**/tui_*"` の結果を読み（backup_failure.rs と backup_sessions.rs の TUI 部分だけが残ってよい）、テストコマンドと clippy を実行する
- Left to the implementer: 移す先のモジュール名
- Stop and hand back if: none

### S2: CLI の実行でも診断ログを保存する

- Purpose: agent を除く CLI のサブコマンドの実行が診断ログを保存し、logs で読めるようにし、TUI に頼っていた診断ログのテストを CLI のテストに置き換える
- Specification: `docs/ir/cli/diagnostics.md#REQ-cli-075`, `docs/ir/cli/diagnostics.md#REQ-cli-014`, `docs/ir/cli/diagnostics.md#REQ-cli-015`
- Prerequisites: S1
- May change: `crates/remote-merge/src/main.rs`, `crates/remote-merge/src/telemetry/`, `crates/remote-merge/src/cli/logs.rs`, `crates/remote-merge/tests/contract/diagnostics.rs`, `crates/remote-merge/tests/cli_logs_events.rs`, `crates/remote-merge/tests/contract/merge_paths.rs`, `crates/remote-merge/tests/common/mod.rs`, `crates/remote-merge/tests/contract/` のバイナリ起動の補助
- Done when: ログの細かさを指定せずに status や merge を CLI で実行した後に `logs` を実行すると、その実行の情報レベル以上の診断ログが読める。agent サブコマンドは診断ログを保存しない。診断ログの置き場に書き込めないときもコマンドは成功し、標準出力と標準エラーは変わらない。merge した中身と SSH の認証情報が保存された診断ログに含まれない。標準出力の JSON と標準エラーの既定の出力は変わらない。診断ログは実行の始めに 10MB を超えていれば切り詰められる（上限は契約ではないのでテストで値を固定しない）。バイナリを起動するテストの補助は診断ログの置き場を一時ディレクトリへ向け、開発者のキャッシュに書かない。EX-cli-030 のテストが TUI を起動せずに CLI の実行で確かめている。ワークスペースのテストと clippy が通る
- Shown by: test — REQ-cli-075 の印を付けた契約テスト（指定なしの実行の後に logs で記録が読める、agent は保存しない、置き場に書き込めなくてもコマンドが成功する）と、EX-cli-027・029・030 の印のテスト
- Left to the implementer: none
- Stop and hand back if: 診断ログを保存すると標準出力か標準エラーの既存の出力が変わる

### S3: 確認手段のない SSH 接続の拒否を TUI に頼らずに表す

- Purpose: `is_tui` をなくし、「確認手段がないときは未知のホスト鍵を受け入れない」を今の CLI の確認（標準入力から yes か y を読めたときだけ受け入れる）で担う（決定 A20）
- Specification: `docs/ir/ssh/host-key.md#REQ-ssh-001`, `docs/ir/ssh/host-key.md#REQ-ssh-002`
- Prerequisites: S2
- May change: `crates/remote-merge-settings/src/lib.rs`, `crates/remote-merge-config/src/lib.rs`, `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge-ssh/src/host_key_verifier.rs`, `crates/remote-merge/src/main.rs`, `crates/remote-merge/tests/contract/ssh_host_key.rs`
- Done when: settings・config・ssh・製品から is_tui がなくなる。標準入力が空（入力の終わり）の接続では未知のホスト鍵で接続が止まり、標準入力に "yes" を渡せば従来どおり受け入れ、`--yes` か `strict_host_key_checking=no` の明示で続けられる。EX-ssh-001・004 と REQ-ssh-002 のテストが is_tui を使わずに、標準入力が空のときの拒否で通る。ワークスペースのテストと clippy が通る
- Shown by: test — EX-ssh-001・EX-ssh-004 と REQ-ssh-002 の印のテスト（標準入力が空のときの拒否と、明示したときの続行）
- Left to the implementer: none
- Stop and hand back if: 対話の CLI で未知のホスト鍵の確認の挙動が変わる

### S4: TUI の入口・実行時処理・モジュール・テストと events を除く

- Purpose: 引数なしの起動を使い方の表示と終了コード 2 に変え、TUI の実行時処理、TUI だけのモジュール、events サブコマンドと操作イベントの記録、TUI のテストと疑似端末のテストの道具を除く
- Specification: `docs/ir/cli/safety.md#REQ-cli-076`, `docs/ir/backup/sessions.md#REQ-backup-025`
- Prerequisites: S3
- May change: `crates/remote-merge/src/`, `crates/remote-merge/tests/`
- Done when: サブコマンドなしの起動で標準エラーに使い方が出て終了コード 2 になり何も書き込まない。`events` サブコマンドがなく、トップレベルの --left・--right・--ref がない。app・handler・ui・highlight・theme・state・format と runtime の TUI 部分（TuiRuntime、bootstrap、scanner、badge_scan、merge_scan）と telemetry の TUI 部分（state_dumper の画面ダンプ、event_recorder、event_types、truncate のうち TUI だけのもの）がない。tests/tui_*.rs、tests/tui_session.rs、tests/common の疑似端末の仕組み、contract の tui_* モジュール、backup_failure.rs の TUI 部分（TuiFixture と REQ-backup-020 のテスト）、backup_sessions.rs の tui_start_removes_expired_sessions、diagnostics.rs と cli_logs_events.rs・cli_error_handling.rs・config_loading_cli.rs の events のテスト、cli_test_environment.rs の TUI の起動部分、除いた ID の印を持つテストがない。サブコマンドなしの解析を前提にした main.rs の単体テストはサブコマンドを付けた形に直っている。S2 の診断ログが使う切り詰めの関数は残っている。期限切れのバックアップの整理は merge・sync の開始時にだけ行われる。ワークスペースのテストと clippy が通る
- Shown by: test — REQ-cli-076 の印を付けた契約テスト（引数なしの起動と、-y・--debug のようなグローバルなフラグだけの起動のそれぞれで、標準エラーに使い方、終了コード 2、診断ログの置き場とカレントディレクトリに何も作られない）、REQ-backup-025 の既存の印のテスト
- Left to the implementer: 引数なしの起動の止め方（clap の機能か None の分岐か）
- Stop and hand back if: 除いた TUI のモジュールを CLI が S1 で付け替えた以外の形で使っていた

### S5: TUI だけが使っていた依存を除く

- Purpose: ratatui・crossterm・syntect・arboard と、TUI だけが使っていた依存と dev 依存（unicode-width、製品 crate の similar と libc、expectrl など）を製品の Cargo.toml から除く
- Specification: `docs/decision/records/2026-10-04-tui-disposition.md#A1`
- Prerequisites: S4
- May change: `crates/remote-merge/Cargo.toml`, `Cargo.lock`
- Done when: 除いた依存が製品の Cargo.toml にも Cargo.lock の製品の依存にもなく、ワークスペースのテストと clippy が通る。core と agent が自分で宣言している依存は残る
- Shown by: check — テストコマンドと clippy を実行し、`cargo tree -p remote-merge -e normal,dev` の結果に除いた依存がないことを読む
- Left to the implementer: none
- Stop and hand back if: 除こうとした依存を TUI 以外のコードが使っていた

### S6: badge_scan_max_files を使わずに警告する

- Purpose: badge_scan_max_files を値を検査せずに受け付け、一度だけ警告して使わないようにし、設定の欄と検査を除く
- Specification: `docs/ir/config/filters.md#REQ-config-031`, `docs/ir/config/defaults.md#REQ-config-013`, `docs/ir/config/precedence.md#REQ-config-027`
- Prerequisites: S5
- May change: `crates/remote-merge-config/src/lib.rs`, `crates/remote-merge/src/config.rs`, `crates/remote-merge/src/`, `crates/remote-merge/tests/contract/config_values.rs`, `crates/remote-merge/tests/contract/config_merging.rs`, `crates/remote-merge/tests/contract/config_loading_cli.rs`
- Done when: グローバル設定かプロジェクト設定に badge_scan_max_files を書いて CLI を実行すると、範囲外の値や整数でない値（文字列・負の数）でも止まらず、標準エラーに "Warning: badge_scan_max_files is no longer used and is ignored" がちょうど一度出る。書かなければ出ない。設定の欄と範囲の検査が残っていない。ワークスペースのテストと clippy が通る
- Shown by: test — REQ-config-031 の印を付けた契約テスト（片方・両方・範囲外の値・文字列の値・なし）、REQ-config-013 と REQ-config-027 の既存の印のテスト
- Left to the implementer: none
- Stop and hand back if: 警告の出し方が既存の sensitive の警告（REQ-config-030）の一度だけの規則とぶつかる

### S7: TUI だけが使っていた下位 crate の項目と再公開を除く

- Purpose: S4 の後に使われなくなった engine・core の項目と製品の再公開を除く
- Specification: `docs/decision/records/2026-10-04-tui-disposition.md#A1`
- Prerequisites: S6
- May change: `crates/remote-merge-engine/src/`, `crates/remote-merge-core/src/`, `crates/remote-merge/src/`
- Done when: engine の merge/execution・merge/mtime・optimistic_lock、three_way の行の比較、core の compute_conflict_if_complete、製品の local と merge::optimistic_lock の再公開、CoreRuntime の drive_runtime と check_connection のうち、どこからも使われていないものが残っておらず、ワークスペースのテストと clippy が通る
- Shown by: check — テストコマンドと clippy を実行し、除いた項目を `rg` で探して利用者がないことを読む
- Left to the implementer: 一部をまだ使うものが見つかったときに残す範囲
- Stop and hand back if: none

### S8: 文書を CLI 専用のツールとして書き直す

- Purpose: 利用者とエージェントが存在しない画面を使おうとしないよう、文書から TUI の説明と凍結の規約を除く
- Specification: `docs/decision/records/2026-10-04-tui-disposition.md#A11`
- Prerequisites: S7
- May change: `README.md`, `skills/remote-merge/SKILL.md`, `skills/remote-merge/references/json-schemas.md`, `PROJECT.md`, `crates/remote-merge/Cargo.toml`, `crates/remote-merge/src/main.rs`, `scripts/mutants.sh`
- Done when: README.md、SKILL.md（説明文を含む）、json-schemas.md、PROJECT.md、製品の Cargo.toml の description、CLI の about、scripts/mutants.sh のコメントに TUI の説明・TUI の監視・events・state.json・screen.txt・events.jsonl・トップレベルの --left/--right/--ref・TUI 凍結の規約が残っていない
- Shown by: check — `rg -n -i "tui|events|state\.json|screen\.txt" README.md skills/remote-merge PROJECT.md crates/remote-merge/Cargo.toml scripts/mutants.sh` の結果を読み、残っているのが TUI と関係ない語だけであることを確かめる
- Left to the implementer: 書き直す文の言い回し
- Stop and hand back if: none

### S9: 計画の要件のテストと変更の照合を揃える

- Purpose: この計画の新しい要件と変更した要件と例がすべて印付きのテストを持ち、変更の照合の門を通る状態にする
- Specification: `docs/ir/cli/safety.md#REQ-cli-076`, `docs/ir/cli/diagnostics.md#REQ-cli-075`, `docs/ir/config/filters.md#REQ-config-031`
- Prerequisites: S8
- May change: `.kotowari/changes/implementation.yaml`, `crates/remote-merge/tests/`
- Done when: Specification の「新しい要件」「変更した要件」「変更した例・表」の ID の `kotowari query ID` がすべて空でない tests を返し（REQ-cli-014・015 は例 EX-cli-027・029・030 の印でよい）、`kotowari check --format json` に unresolved_reference がなく、このブランチが変えたファイルとこの計画の ID に関する error がなく、実装者の `.kotowari/changes/implementation.yaml` が main からの分岐点から HEAD までのブランチ全体を覆う
- Shown by: check — `kotowari check --format json`、各 ID の `kotowari query ID`、`kotowari changes --base HEAD --staged --phase implementation --format json`（記録をコミットする前の自己検査）、記録をコミットした後の `kotowari changes --base <main からの分岐点の完全な ID> --head <HEAD の完全な ID> --phase implementation --format json`
- Left to the implementer: none
- Stop and hand back if: none
