# Plan: 機密ファイルの特別扱いをやめる

## Goal

機密パターン（設定の `[filter] sensitive` と既定の 6 パターン）に当たるファイルも、diff・status・merge・sync・rollback・`--hunks`・TUI で他のファイルと同じに扱われ、設定に残った `sensitive` の指定には警告だけが出る。

## Specification

IR は `docs/ir/`、根拠の決定は `docs/decision/records/2026-10-04-drop-sensitive-file-handling.md`（A1〜A14）。この計画が扱う要件と例:

- 変更した要件: `docs/ir/cli/safety.md#REQ-cli-003`、`docs/ir/cli/symlink-diff.md#REQ-cli-026`、`docs/ir/cli/status-output.md#REQ-cli-031`、`docs/ir/cli/status-output.md#REQ-cli-032`、`docs/ir/cli/merge.md#REQ-cli-048`、`docs/ir/cli/diff-output.md#REQ-cli-053`、`docs/ir/cli/merge.md#REQ-cli-074`、`docs/ir/config/precedence.md#REQ-config-002`
- 新しい要件: `docs/ir/config/filters.md#REQ-config-030`
- 変更した例: EX-cli-004、EX-cli-008、EX-cli-029、EX-cli-055、EX-cli-067、EX-cli-068
- 機密パターンに当たるファイルの扱いを確かめるのに使う既存の要件: `docs/ir/cli/status-targets.md#REQ-cli-035`、`docs/ir/backup/rollback-cli.md#REQ-backup-034`、`docs/ir/tui/export.md#REQ-tui-007`、`docs/ir/tui/export.md#REQ-tui-008`
- 仕様から除いた要件と例（実装とテストから取り除く対象。テストを求めない）: REQ-cli-005、REQ-cli-023、REQ-cli-036、REQ-cli-059、REQ-merge-025、REQ-backup-036、REQ-config-026、REQ-tui-009、TBL-merge-001 の機密の行、EX-cli-009、EX-cli-010、EX-cli-049、EX-cli-050、EX-config-004、EX-tui-017、EX-tui-018

## Approach and why

機密の判定は設定の `FilterConfig` の `sensitive` パターンから始まり、engine の status・diff・merge・sync・rollback の判定と、製品の CLI 出力・TUI へ流れている。一度に型や関数を消すと、どのステップの途中でもワークスペースがビルドできなくなる。

そこで S1 では、設定の読み込みが機密のパターンを一つも返さないようにする（既定の 6 パターンをやめ、利用者の指定は警告して捨てる）。`FilterConfig` の `sensitive` 欄と、それを受け取る関数の引数は S1 では残す。これで S1 の後は、どのコードも機密に当たるファイルを見つけなくなり、振る舞いはすでに仕様どおりになる。S2〜S6 では、使われなくなった判定・欄・表示・確認を領域ごとに取り除く。欄と引数そのものは、全部の利用者を除いた S7 で消す。どのステップの後でもビルド・clippy・テストが通る。

除いた要件の ID を印に持つテストは、消すか、機密に触れない形に直す。いくつかの ID を印に持つテストは、除いた ID だけを印から外す。機密パターンに当たる名前（".env"）のファイルが他と同じに扱われることは、その振る舞いを定めている残りの要件の印付きテストで確かめる。除いた出力（" [SENSITIVE]"、sensitive 欄、機密の警告）は、仕様が定める出力と完全に一致することを確かめる形で確認し、消えた文言の不在を文字列で探すテストは書かない。

TUI は凍結中だが、決定の A10 で機密の扱いを除くことに限って例外が認められている。TUI では除くだけにして、新しい表示や操作は足さない。REQ-config-030 の警告は TUI でも、画面を始める前に標準エラーへ出す。

診断の記録（logs・events）にファイルの中身や認証情報を入れない規則（REQ-cli-015、決定 A3）と、`--follow-external-links` による root_dir の外の扱い（REQ-cli-026）は残す。`--force` の機密以外の役目（決定 A6）も残す。diff の `--force` は受け付けたまま効果を持たないものにし、ヘルプを書き換える（決定 A14）。

## Scope of change

- `crates/remote-merge-config/src/lib.rs`
- `crates/remote-merge-engine/src/service/`（`types.rs`、`status.rs`、`diff.rs`、`merge.rs`、`merge_flow.rs`、`sync.rs`、`rollback.rs`、`max_files.rs`、`path_resolver.rs`）
- `crates/remote-merge/src/cli/`（`diff.rs`、`status.rs`、`merge.rs`、`sync.rs`、`rollback.rs`）、`crates/remote-merge/src/service/`（`output.rs`、`merge_flow.rs`）、`crates/remote-merge/src/main.rs`
- `crates/remote-merge/src/app/`、`crates/remote-merge/src/handler/`、`crates/remote-merge/src/ui/`、`crates/remote-merge/src/runtime/`、`crates/remote-merge/src/telemetry/` のうち機密の判定・表示・確認
- `crates/remote-merge/tests/`
- `skills/remote-merge/SKILL.md`、`skills/remote-merge/references/json-schemas.md`、`README.md`
- `.kotowari/changes/implementation.yaml`（実装者が書く。`.kotowari/changes/review.yaml` は実装と別の独立したレビューが書き、この計画のステップでは書かない）

"sensitive" の文字列でも、検索の大文字小文字（case-sensitive）や SSH・エージェントの検証の文脈など、機密ファイルと関係ないものは変えない。

## Step order and prerequisites

S1 → S2 → S3 → S4 → S5 → S6 → S7 → S8 → S9 の順に一つずつ進める。S1 で設定が機密のパターンを返さなくなることが S2〜S6 の前提になる。S2〜S6 はそれぞれ別の領域の利用者を除くが、共有の型（engine の `service/types.rs` や出力の `service/output.rs`）を順に触るので、並べて進めない。S7 は S2〜S6 で全部の利用者がなくなってから欄と引数を消す。S8 は文書、S9 は全体の確認。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-config-030, REQ-config-002 | EX-config-003 |
| S2 | REQ-cli-031, REQ-cli-032, REQ-cli-035 | なし |
| S3 | REQ-cli-053, REQ-cli-026 | EX-cli-055, EX-cli-067, EX-cli-068 |
| S4 | REQ-cli-003, REQ-cli-048, REQ-cli-074 | EX-cli-004, EX-cli-008, EX-cli-005, EX-cli-006 |
| S5 | REQ-backup-034 | なし |
| S6 | REQ-tui-007, REQ-tui-008 | EX-tui-013, EX-tui-015 |
| S7 | なし（欄と引数の削除。既存のテストが通ること） | なし |
| S8 | なし（決定 A12 の文書） | なし |
| S9 | この計画の変更した要件と新しい要件 | EX-cli-029 |

## Left to the implementer

- 使われなくなった関数・型・定数・テスト用の補助をどこまで小さくするか（振る舞いが変わらない範囲）
- 除いた ID だけを印に持つテストを、消すか、機密に触れない同じ振る舞いのテストに直すか（直す場合は残る要件の ID を印にする）
- REQ-config-030 の警告を出す場所（設定の読み込み時か、読み込んだ直後の呼び出し側か）。一回の実行で一度だけという条件と、TUI では画面を始める前に出すことは守る

## Stop conditions

- 機密の判定が、診断の記録の規則（REQ-cli-015、決定 A3）や root_dir の外の扱い（REQ-cli-026）と同じコードを共有していて、片方だけを除けない
- 機密を除くと、残すと決めた `--force` の役目（決定 A6）が変わる
- 除くために TUI へ新しい表示や操作を足す必要が出る（決定 A10 の範囲外）
- 通信の形（エージェントの protocol）を変えないと除けない
- 除いた ID を印に持つテストの中に、残す要件の唯一のテストが含まれていて、消すとその要件のテストがなくなる

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G cargo nextest run --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

## Out of scope

- 診断の記録（logs・events）にファイルの中身や認証情報を入れない規則
- `--follow-external-links` と root_dir の外の扱い
- diff の 100MB を超えるファイルの扱い（未決の FLAG-cli-039）
- 版を上げることとリリース（決定 A5 の版の変更はリリースのときに行う）
- 照合で見つかった、この変更と関係ない根拠不足（REQ-cli-022、EX-cli-028、EX-tui-016）

## Steps

### S1: 設定が機密のパターンを返さないようにし、残った指定に警告を出す

- Purpose: 既定の 6 パターンをやめ、`[filter] sensitive` の指定は警告して捨て、設定の読み込みが常に空の機密パターンを返すようにする
- Specification: `docs/ir/config/filters.md#REQ-config-030`, `docs/ir/config/precedence.md#REQ-config-002`
- Prerequisites: none
- May change: `crates/remote-merge-config/src/lib.rs`, `crates/remote-merge/src/main.rs`, `crates/remote-merge/tests/contract/config_filters.rs`, `crates/remote-merge/tests/contract/config_precedence.rs`, `crates/remote-merge/tests/contract/config_values_cli.rs`, `crates/remote-merge/tests/contract/config_loading_cli.rs`, 機密パターンに依存して S1 の後に失敗するようになったテスト（そのテストが確かめる要件が除かれたものなら消し、残る要件なら機密に触れない形に直す）
- Done when: グローバル設定かプロジェクト設定の `[filter]` に sensitive を書いて CLI を実行すると、標準エラーに "Warning: [filter] sensitive is no longer used and is ignored" がちょうど一度出て処理が続き、両方に書いても一度、書かなければ出ない。設定の読み込みが返す機密パターンが空である。REQ-config-026 と EX-config-004 の印を持つテストが残っていない。ワークスペースのテストと clippy が通る
- Shown by: test — REQ-config-030 の印を付けた契約テスト（片方・両方・なしの三通り）
- Left to the implementer: none
- Stop and hand back if: sensitive を受け付けたまま捨てると、`[filter]` の他のキーの読み込みが変わる

### S2: status から機密の判定と印と欄を除く

- Purpose: status の判定・--ref の比較・テキストの印・JSON の欄から機密の扱いを除く
- Specification: `docs/ir/cli/status-output.md#REQ-cli-031`, `docs/ir/cli/status-output.md#REQ-cli-032`, `docs/ir/cli/status-targets.md#REQ-cli-035`
- Prerequisites: S1
- May change: `crates/remote-merge-engine/src/service/types.rs`, `crates/remote-merge-engine/src/service/status.rs`, `crates/remote-merge/src/cli/status.rs`, `crates/remote-merge/src/service/output.rs`, `crates/remote-merge/src/runtime/scanner.rs`, `crates/remote-merge/tests/contract/status_targets.rs`, `crates/remote-merge/tests/contract/status_output.rs`, `crates/remote-merge/tests/contract/status_results.rs`, `crates/remote-merge/tests/contract/merge_paths.rs`, `crates/remote-merge/tests/contract/backup_storage.rs`, `crates/remote-merge/tests/cli_status.rs`
- Done when: ".env" を含む status のテキストの行が TBL-cli-003 の記号とパスだけと完全に一致し、JSON の files の各項目の欄が "path" と "status" だけである。--ref の status で ".env" も他のファイルと同じく参照先と中身を比べて印が付く。REQ-cli-036 の印を持つテストが残っておらず、他の印も持つテストからは REQ-cli-036 だけが外れている。ワークスペースのテストと clippy が通る
- Shown by: test — REQ-cli-031 と REQ-cli-032 の印を付けた契約テスト（".env" を含む status のテキストと JSON の完全一致）、REQ-cli-035 の印を付けた契約テスト（".env" を含む --ref の status）
- Left to the implementer: none
- Stop and hand back if: status の JSON の他の欄の形も変わってしまう

### S3: diff から機密の非表示と symlink の連鎖の判定を除く

- Purpose: diff が機密パターンに当たるファイルと symlink の参照先も他と同じく中身・ハッシュ・差分を出すようにし、JSON から sensitive 欄を除き、diff の --force を効果のないオプションにする
- Specification: `docs/ir/cli/diff-output.md#REQ-cli-053`, `docs/ir/cli/symlink-diff.md#REQ-cli-026`
- Prerequisites: S2
- May change: `crates/remote-merge-engine/src/service/diff.rs`, `crates/remote-merge-engine/src/service/types.rs`, `crates/remote-merge-engine/src/service/max_files.rs`, `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge/src/service/output.rs`, `crates/remote-merge/src/main.rs`, `crates/remote-merge/tests/cli_diff.rs`, `crates/remote-merge/tests/contract/diff_file_kinds.rs`, `crates/remote-merge/tests/contract/diff_support.rs`, `crates/remote-merge/tests/contract/cli_results.rs`, `crates/remote-merge/tests/contract/diff_format.rs`, `crates/remote-merge/tests/contract/diff_max_files.rs`
- Done when: --force なしの diff で ".env" と ".env" を指す symlink の中身の差分が出て、JSON の files の各項目の欄が REQ-cli-053 の列挙と一致し sensitive 欄がない。diff の --force を付けても付けなくても出力が同じで、diff の --force のヘルプが効果のないことを述べる（決定 A14）。EX-cli-055・067・068 のテストが機密に触れない形で通る。REQ-cli-005・023・059 と EX-cli-009・010・049・050 の印を持つテストが残っていない。ワークスペースのテストと clippy が通る
- Shown by: test — REQ-cli-053 の印を付けた契約テスト（".env" の diff の JSON の欄と中身の差分）と、EX-cli-055・067・068 の印のテスト
- Left to the implementer: none
- Stop and hand back if: 機密の連鎖の判定を除くと root_dir の外の判定（REQ-cli-026）の結果が変わる、または diff の --force が機密の表示以外の役目を持っていた

### S4: merge・sync・--hunks・削除から機密の除外と警告を除く

- Purpose: merge と sync が機密パターンに当たるファイルも --force なしで書き込み・削除し、--hunks が止まらず、機密の警告を出さないようにし、merge・sync のヘルプから機密の記述を除く
- Specification: `docs/ir/cli/safety.md#REQ-cli-003`, `docs/ir/cli/merge.md#REQ-cli-048`, `docs/ir/cli/merge.md#REQ-cli-074`
- Prerequisites: S3
- May change: `crates/remote-merge-engine/src/service/merge.rs`, `crates/remote-merge-engine/src/service/merge_flow.rs`, `crates/remote-merge-engine/src/service/sync.rs`, `crates/remote-merge-engine/src/service/path_resolver.rs`, `crates/remote-merge/src/cli/merge.rs`, `crates/remote-merge/src/cli/sync.rs`, `crates/remote-merge/src/service/merge_flow.rs`, `crates/remote-merge/src/service/output.rs`, `crates/remote-merge/src/main.rs`, `crates/remote-merge/tests/contract/merge_support.rs`, `crates/remote-merge/tests/contract/merge_links.rs`, `crates/remote-merge/tests/contract/merge_results.rs`, `crates/remote-merge/tests/contract/merge_force_cli.rs`, `crates/remote-merge/tests/contract/merge_hunks.rs`, `crates/remote-merge/tests/contract/sync_cli.rs`, `crates/remote-merge/tests/contract/sync_results.rs`, `crates/remote-merge/tests/cli_merge.rs`
- Done when: --force なしの JSON の merge で ".env" が merged に "ok" で入って書き込まれ、--delete で書き込み先にだけある ".env" が deleted に入って削除され、--hunks の merge が ".env" で止まらず、merge のテキストの標準エラーに機密の警告の行がない。リモート間の merge は --force がなければ今までどおり止まる。REQ-merge-025 の印を持つテストが残っていない。ワークスペースのテストと clippy が通る
- Shown by: test — REQ-cli-048 の印を付けた契約テスト（".env" を含む merge と --delete の JSON の merged・deleted）と、EX-cli-005・006 の既存テスト
- Left to the implementer: none
- Stop and hand back if: 機密を除くと、リモート間の merge の止め方（REQ-cli-073）や --ref の確認（REQ-cli-051）が変わる

### S5: rollback から機密の除外を除く

- Purpose: rollback が機密パターンに当たるファイルも --force なしで書き戻すようにし、rollback のヘルプから機密の記述を除く
- Specification: `docs/ir/backup/rollback-cli.md#REQ-backup-034`
- Prerequisites: S4
- May change: `crates/remote-merge-engine/src/service/rollback.rs`, `crates/remote-merge/src/cli/rollback.rs`, `crates/remote-merge/src/main.rs`, `crates/remote-merge/tests/contract/backup_rollback_cli.rs`, `crates/remote-merge/tests/contract/backup_rollback_cli_e2e.rs`, `crates/remote-merge/tests/cli_rollback_local.rs`
- Done when: ".env" を含むセッションを --force なしで（確認に yes で答えて）rollback すると ".env" も restored に入って書き戻される。REQ-backup-036 の印を持つテストが残っていない。--force が確認を省く役目と期限切れのセッションを戻せる役目は変わらない。ワークスペースのテストと clippy が通る
- Shown by: test — REQ-backup-034 の印を付けた契約テスト（".env" を含むセッションの rollback）
- Left to the implementer: none
- Stop and hand back if: rollback の終了コード（REQ-backup-038）の扱いが変わる

### S6: TUI から機密の確認と表示を除く

- Purpose: TUI のクリップボードとレポートへの持ち出し確認、一括マージと単一マージの確認での機密の扱い、ツリーの機密の表示を除く
- Specification: `docs/ir/tui/export.md#REQ-tui-007`, `docs/ir/tui/export.md#REQ-tui-008`
- Prerequisites: S5
- May change: `crates/remote-merge/src/app/`, `crates/remote-merge/src/handler/`, `crates/remote-merge/src/ui/`, `crates/remote-merge/src/runtime/`, `crates/remote-merge/src/telemetry/`, `crates/remote-merge/tests/contract/tui_export.rs`, `crates/remote-merge/tests/contract/tui_confirm.rs`, `crates/remote-merge/tests/tui_merge.rs`, `crates/remote-merge/tests/tui_integration.rs`
- Done when: ".env" の差分を選んでコピーすると確認なしにクリップボードへ本文が入り、".env" を含むレポートにも確認なしに本文が含まれる。一括マージの確認に ".env" の機密の印が出ず、".env" の単一マージに他のファイルより多い確認が要らない。機密の持ち出し確認のダイアログの状態が残っていない。REQ-tui-009 と EX-tui-017・018 の印を持つテストが残っていない。ワークスペースのテストと clippy が通る
- Shown by: test — REQ-tui-007 と REQ-tui-008 の印を付けた契約テスト（".env" の差分のコピーとレポート）。一括マージと単一マージの確認は、既存の TUI のテストを機密の扱いがない形に直したものが通ること
- Left to the implementer: none
- Stop and hand back if: 除くために TUI に新しい表示や操作を足す必要が出る

### S7: 使われなくなった機密の欄と引数を消す

- Purpose: S2〜S6 で利用者がなくなった `FilterConfig` の `sensitive` 欄、機密パターンを受け取る引数、機密の判定の関数を消す
- Specification: `docs/ir/config/filters.md#REQ-config-030`
- Prerequisites: S6
- May change: `crates/remote-merge-config/src/lib.rs`, `crates/remote-merge-engine/src/`, `crates/remote-merge/src/`, `crates/remote-merge/tests/`
- Done when: 製品のコードに機密パターンを持つ欄・引数・判定の関数が残っておらず（設定の生の読み込みで警告のために受け付ける部分を除く）、REQ-config-030 の契約テストを含むワークスペースのテストと clippy が通る
- Shown by: check — `systemd-run --user --scope -q -p MemoryMax=8G cargo nextest run --all-features`、`cargo clippy --all-targets --all-features -- -D warnings`、`rg -n -i "sensitive" crates --type rust` の結果を読み、機密ファイルの扱いの残りがないことを確かめる
- Left to the implementer: none
- Stop and hand back if: none

### S8: 手引きと README から機密の扱いを除く

- Purpose: 利用者とエージェントがツールの中で機密が守られると誤解しないよう、文書を仕様に合わせる
- Specification: `docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A12`
- Prerequisites: S7
- May change: `skills/remote-merge/SKILL.md`, `skills/remote-merge/references/json-schemas.md`, `README.md`
- Done when: 3 つの文書に機密ファイルの特別扱い（--force で機密を含める、sensitive 欄、sensitive の設定キー）の記述がなく、読ませたくないファイルはツールの外側か exclude で外すという案内が SKILL.md にある
- Shown by: check — `rg -n -i "sensitive" skills/remote-merge README.md` の結果を読み、機密ファイルの扱いを説明する行が残っていないことを確かめる
- Left to the implementer: 案内の文の位置と言い回し
- Stop and hand back if: none

### S9: 計画の要件のテストと変更の照合を揃える

- Purpose: この計画の変更した要件・新しい要件・変更した例がすべて印付きのテストを持ち、変更の照合の門を通る状態にする
- Specification: `docs/ir/config/filters.md#REQ-config-030`, `docs/ir/cli/diagnostics.md#REQ-cli-015`
- Prerequisites: S8
- May change: `.kotowari/changes/implementation.yaml`, `crates/remote-merge/tests/`
- Done when: Specification の「変更した要件」「新しい要件」「変更した例」に挙げた ID の `kotowari query ID` がすべて空でない tests を返し（REQ-cli-015 自体の印は求めず、その例 EX-cli-029 の印があればよい）、`kotowari check --format json` に unresolved_reference がなく、このブランチが変えたファイルとこの計画の ID に関する error がなく、実装者の `.kotowari/changes/implementation.yaml` が main からの分岐点から HEAD までのブランチ全体を覆う
- Shown by: check — `kotowari check --format json`、Specification の各 ID の `kotowari query ID`、`kotowari changes --base HEAD --staged --phase implementation --format json`（自己検査）
- Left to the implementer: none
- Stop and hand back if: none
