# Plan: 再現可能な SSH テストと開発時チェック

## Goal

個人の SSH 設定なしに CLI・TUI の必要な振る舞いを通常テストで検証し、CLI diff の symlink とディレクトリ指定を一貫させ、実 OpenSSH と sudo は必須 CI で検証し、開発中の検査を共有設定から起動できるようにする。

## Specification

IR は `docs/ir/` にある。この計画は `docs/ir/testing/environments.md#REQ-testing-001` から `#REQ-testing-008`、`docs/ir/cli/symlink-diff.md#REQ-cli-020` から `#REQ-cli-024` および `#REQ-cli-026`、`docs/ir/cli/directory-paths.md#REQ-cli-025` とそれぞれの例を対象とする。SSH とバックアップの既存の期待結果は `docs/ir/ssh/compatibility.md`、`docs/ir/ssh/sudo.md`、`docs/ir/merge/permissions.md`、`docs/ir/backup/storage.md` を参照する。判断の背景とログ検査の扱いは `docs/decision/records/2026-09-26-reproducible-test-environments.md`、`docs/decision/records/2026-09-26-development-check-hooks.md`、`docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A6` にある。

## Approach and why

スキップ中の108件を分類して隔離 SSH fixture を整えた後、そこで見つかった CLI diff の symlink と末尾スラッシュの不具合を仕様どおりテストファーストで直す。ログ検査の並列失敗は原因を切り分けてから直す。以降は CLI バイナリと PTY 上の TUI の表示・実ファイル更新を検査する。fixture がシェルを再実装することになるケースは実 OpenSSH のコンテナ用独立 Cargo パッケージへ移す。Docker コンテナは sudo・所有者・sshd の相互運用にだけ使い、旧 CentOS 5 の負荷用手順から分離する。Lefthook は自動修正をしない共有設定として通常系の移行後に追加し、CI はローカルフックの有無に関係なく直接チェックを実行する。

## Scope of change

- `tests/common/mod.rs`、`tests/contract/ssh_server.rs`、必要な `tests/cli_*.rs`・`tests/tui_*.rs`・`tests/contract/*.rs` と `tests/contract.rs`
- `src/cli/diff.rs`、CLI 引数の定義箇所、`src/service/diff.rs`、`src/service/types.rs`、`src/service/output.rs`、`src/service/fast_path.rs`、差分に必要な `src/tree.rs`・`src/runtime/` 内の読み取り境界、対応する unit・contract テスト
- `tests/contract/merge_paths.rs` と診断イベントの捕捉・発行箇所（並列失敗の原因が確認できた部分だけ）
- `tests/e2e_testenv.rs`、`tests/contract/ssh_sudo.rs`、`testenv/sudo_e2e.sh` のコンテナ専用ケースと参照先
- `tests/container-e2e/`（ルートパッケージの自動収集対象外となる独立 Cargo パッケージ）、`scripts/run-container-e2e.sh`（専用ランナー）、`Cargo.toml`、必要な `.gitignore`
- `testenv/` から `bench/legacy-ssh/` へ移す手動負荷環境とその起動・終了手順
- `.github/workflows/ci.yml`、`lefthook.yml`、`.claude/settings.json`、`.claude/hooks/pre-commit-format.sh`、`PROJECT.md`
- `docs/testing/ignored-test-migration.md`（108件の検知力と移行先の対応表）

## Step order and prerequisites

S1 の分類と S2 の隔離 fixture は専用ブランチで完了済み。S3 で symlink のリンク情報・内容・外部追跡・秘匿・部分エラーを一貫して検証し、S4 でディレクトリ指定の末尾スラッシュを直す。S5 で並列ログ検査の失敗を診断する。S3–S5 の結果を使って S6 の CLI、S7 の TUI を移し、S8 の実 OpenSSH パッケージを S9 の CI に接続する。S10 の旧負荷環境の移動は S8 に旧環境への依存がないことを確認してから行う。S11 で全層の結果と108件の対応表を照合する。コード変更は不具合を捕まえるテストを先に失敗させ、RED→GREEN→REFACTOR の順に進める。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-testing-003 | EX-testing-003, EX-testing-008 |
| S2 | REQ-testing-001, REQ-testing-002 | EX-testing-002 |
| S3 | REQ-cli-020–024, REQ-cli-026 | EX-cli-039–050, EX-cli-053–059, EX-cli-061 |
| S4 | REQ-cli-025 | EX-cli-051, EX-cli-052, EX-cli-060 |
| S5 | decision record A6 (並列ログ検査) | 既存のログ非漏洩契約テスト |
| S6 | REQ-testing-001, REQ-testing-002, REQ-testing-003 | EX-testing-001, EX-testing-002, EX-testing-003, EX-testing-008 |
| S7 | REQ-testing-001, REQ-testing-002, REQ-testing-003 | EX-testing-001, EX-testing-002, EX-testing-003, EX-testing-008 |
| S8 | REQ-testing-004, REQ-testing-005 | EX-testing-004, EX-testing-005, EX-testing-007 |
| S9 | REQ-testing-001, REQ-testing-004, REQ-testing-005, REQ-testing-007, REQ-testing-008 | EX-testing-001, EX-testing-004, EX-testing-005, EX-testing-007, EX-testing-009, EX-testing-010, EX-testing-011 |
| S10 | REQ-testing-006 | EX-testing-006 |
| S11 | REQ-testing-001–008, REQ-cli-020–026 | EX-testing-001–011, EX-cli-039–061 のうち存在する例 |

## Left to the implementer

- fixture 内の認証方式と小さなヘルパーの配置。ただし鍵・パスワードのテスト値は一時生成または明らかなダミー値とし、実ファイル結果と PTY の表示を検査する。
- 対応表での各テストの維持・統合・削除。ただし108件の全てに見逃し得る不具合と移行先または既存の代替検証を記録し、未定義の製品挙動は決めずに止める。
- 利用可能な現行 OpenSSH イメージの固定ダイジェストとコンテナ用 crate の内部配置。ただし実行時に可変タグやホストの固定ポートを参照しない。
- CLI diff に必要な走査・読み取り手段は既存のツリー・SSH/Agent ターゲット・差分整形を先に検討する。走査と部分エラー・機密判定・JSON 表現は独立して採用または構築の理由を残し、仕様上の追跡・拒否境界は変えない。

## Stop conditions

- fixture のシェル模擬では表示・実ファイルの検証が成立せず、実 OpenSSH に移しても必要な動作を試せないとき。
- sudo 拒否、バックアップ、既存の権限などで製品仕様と既存テストの期待結果が食い違い、仕様変更が必要なとき。
- CI から固定ダイジェストの OpenSSH イメージや公開 kotowari の固定コミットを取得できないとき。
- 既存の利用者ローカルフックや手動環境の生成物を上書き・削除しなければ進められないとき。
- S3・S4 の CLI diff と S5 で原因が確認できたログの発行・捕捉箇所以外に製品コード変更が必要なときは、見つけた不具合と影響を報告して別判断に戻す。

## Test command

標準テストは `cargo test --all-features` と `cargo nextest run --all-features` の両方を使う。専用コンテナテストは `scripts/run-container-e2e.sh` で起動し、ランナーが主バイナリとコンテナで実行する Agent バイナリを準備して `tests/container-e2e/Cargo.toml` を指定した Cargo 実行へ渡し、Docker・sshd の準備失敗を非ゼロ終了にする。最終チェックは `cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`kotowari check --format json`、`git diff --check` とする。CI の kotowari 入手元は公開 Git `https://github.com/ba0918/kotowari.git` のコミット `d17ec08914ce3d87008bb04155dfdd3e447424ec` を指定し、ローカル開発ではインストール済みの kotowari を使う。

## Out of scope

- CentOS 5／OpenSSH 4.3 固有動作を必須 CI の保証に加えること。
- Windows ネイティブに Unix PTY テストを移植すること。
- 既存の `.git/hooks/` にある個人のフックを、インストール手順の実行や自動上書きで置き換えること。
- 利用者の許可なしに CI 用のブランチを push したり、公開設定を変更したりすること。

## Steps

### S1: スキップ中の108件の検知力と移行先を記録する

- Purpose: 実行可能にする前に既存テストが何を見逃すかを特定する。
- Specification: `docs/ir/testing/environments.md#REQ-testing-003`
- Prerequisites: 承認済みの IR と現在のテスト一覧。
- May change: `docs/testing/ignored-test-migration.md`
- Done when: スキップ中の108件を漏れなく列挙し、各行に現実の回帰、重複の有無、維持・統合・削除の理由、必要な通常またはコンテナの実行先を記録する。
- Shown by: artifact — `docs/testing/ignored-test-migration.md` と `cargo nextest list --all-features --message-format json` の `rust-suites[].testcases[].ignored` が true の一覧を突き合わせ、対応漏れと代替テストのない削除がないことを確認する。
- Left to the implementer: 実際の振る舞いを共有するケースをまとめる単位。
- Stop and hand back if: テストの意図する製品挙動が IR にないため残すか削除するか判断できない。

### S2: 独立した SSH 接続と実ファイル操作を共有 fixture にする

- Purpose: CLI と TUI が個人鍵や localhost:22 なしで同じ隔離された接続先を使えるようにする。
- Specification: `docs/ir/testing/environments.md#REQ-testing-001`, `docs/ir/testing/environments.md#REQ-testing-002`
- Prerequisites: S1 を完了し、既存の `tests/contract/ssh_server.rs` と `tests/common/mod.rs` の起動・認証方式を確認する。
- May change: `tests/common/mod.rs`, `tests/contract/ssh_server.rs`, `tests/contract.rs`, `tests/ssh_integration.rs`, `tests/agent_ssh_deploy.rs`
- Done when: 同期的な CLI・PTY テストの呼び出しから戻っても fixture が所有する Tokio runtime と SSH サーバータスクが接続終了まで生き、別々のテストが異なる動的ポートと一時ディレクトリを使って実ファイルの読み書き・表示を確認できる。
- Shown by: test — 同期的なテストで fixture のコンストラクタが戻った後に CLI を接続し、二つの隔離環境で同じ相対パスを扱ってそれぞれ別の差分表示と書き込み後の実ファイル内容になる試験を RED→GREEN で追加する。
- Left to the implementer: 既存の鍵認証 fixture を再利用するか、サーバー側にテスト用認証を追加するか。
- Stop and hand back if: fixture では CLI が必要とする操作を実ファイルで再現できず、シェル全体の模擬が必要になる。

### S3: symlink を辿る CLI diff を統一した比較にする

- Purpose: リンク文字列だけでなく解決後の種類・内容を比較し、範囲外追跡と機密表示を別の許可で制御する。
- Specification: `docs/ir/cli/symlink-diff.md#REQ-cli-020`, `docs/ir/cli/symlink-diff.md#REQ-cli-021`, `docs/ir/cli/symlink-diff.md#REQ-cli-022`, `docs/ir/cli/symlink-diff.md#REQ-cli-023`, `docs/ir/cli/symlink-diff.md#REQ-cli-024`, `docs/ir/cli/symlink-diff.md#REQ-cli-026`
- Prerequisites: S2 の隔離 SSH fixture と承認済みの CLI diff IR。CLI を起動する前に fixture の設定がテスト所有の動的ポートであり、ポート22と個人鍵に向かないことを確認する。
- May change: `src/cli/diff.rs`, CLI 引数の定義箇所, `src/service/diff.rs`, `src/service/types.rs`, `src/service/output.rs`, `src/service/fast_path.rs`, `src/tree.rs`, `src/runtime/` の必要な読み取り境界, `tests/cli_diff.rs`, `tests/contract/cli_results.rs` と検査対象モジュールの unit テスト
- Done when: 指定したリンクの左右の存在・種類によらずパスと参照先内容が別々に判定され、ディレクトリの入れ子・循環・全体件数上限、通常と外部 opt-in、機密パターンと --force、部分的な読取失敗が IR どおり動き、既存の JSON 利用者が保持するフィールドは失われない。
- Shown by: test — まず隔離された CLI テストでパスだけ違うリンク、内容だけ違うリンク、種類違い、片側だけのディレクトリリンク、範囲外で opt-in の有無、機密の入れ子、壊れたリンク、循環・件数超過の実際の出力と終了状態を RED にする。純粋な結果分類と整形は unit テストで試し、実 CLI/JSON で GREEN にする。`kotowari query` で対象例の test marks を照合する。
- Left to the implementer: 走査・対象読取・部分エラー・JSON 整形を既存のどの境界に配置するか。既存の走査・SSH/Agent・表示処理を先に調べ、層ごとに採用または構築の理由を記録する。
- Stop and hand back if: 明示したリンクだけでなく status・merge・sync の追跡や書き込み境界を変えないと達成できない、外部参照を安全に隔離したテストで検証できない、または IR の結果形式では種類違いの表示を確定できない。

### S4: ディレクトリ指定の末尾スラッシュを正規化する

- Purpose: `src` と `src/` の表記差で配下の差分を見逃さない。
- Specification: `docs/ir/cli/directory-paths.md#REQ-cli-025`
- Prerequisites: S2 と S3。symlink 自体の情報を S3 と同じ経路で扱う。
- May change: `src/cli/diff.rs`, `src/service/fast_path.rs`, `tests/cli_diff.rs`, `tests/contract/cli_results.rs` と対応する unit テスト
- Done when: 通常ディレクトリとディレクトリ symlink の双方で、末尾スラッシュの有無が子の結果・リンク情報・終了状態に影響しない。
- Shown by: test — 実際の CLI で同じ左右の一時ファイルを `src` と `src/`、`shared` と `shared/` で比較し、差分あり・差分なしの双方を RED→GREEN で確認する。
- Left to the implementer: 既存の走査方式とパス正規化のどこで同じ対象へ合流させるか。
- Stop and hand back if: パス表記の正規化が他コマンドや symlink の外部追跡の許可を変える。

### S5: 並列で落ちる診断ログ検査を安定させる

- Purpose: `cargo test --all-features` の並列実行時だけ診断ログが空になる問題を原因に沿って直す。
- Specification: `docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A6`
- Prerequisites: S2 の隔離環境が有効で、`tests/contract/merge_paths.rs` の「merge must emit diagnostic records」という失敗を起点にログの発行側と捕捉側を切り分ける。
- May change: `tests/contract/merge_paths.rs` と、原因が直接確認できた診断イベントの発行・捕捉箇所
- Done when: マージ後のファイル更新と診断ログの生成・内容の非漏洩を同じ検査で観測でき、並列実行でもログが空にならない。単に空ログを許可する修正はしない。
- Shown by: test — SSH を起動しない局所テスト同士の並列実行で症状または発行・捕捉の競合を RED として示し、最小修正後に対象テストと `cargo test --all-features` の両方を GREEN にする。原因が再現できない場合は観測結果と未確定事項を報告する。
- Left to the implementer: 実際の原因が発行か捕捉か確かめた後の最小の修正位置。
- Stop and hand back if: 原因を特定できず別の仕組みに置き換える必要がある、または機密ログを出すことなしには検査を通せない。

### S6: CLI の SSH 依存テストを通常実行へ移す

- Purpose: 比較・差分・マージ・終了状態・復元のうち実害を検知する CLI ケースを自動実行に戻す。
- Specification: `docs/ir/testing/environments.md#REQ-testing-001`, `docs/ir/testing/environments.md#REQ-testing-002`, `docs/ir/testing/environments.md#REQ-testing-003`
- Prerequisites: S1–S5。旧 diff ケースの期待結果は S3・S4 の製品規則で判定する。
- May change: `tests/common/mod.rs`, `tests/cli_status.rs`, `tests/cli_diff.rs`, `tests/cli_merge.rs`, `tests/cli_exit_codes.rs`, `tests/cli_rollback.rs`, `tests/contract/*.rs`, `docs/testing/ignored-test-migration.md`
- Done when: 対応表に残すと記載した CLI ケースが ignore なしで実際の CLI 出力・終了状態・ファイル・バックアップの結果を検証し、重複・模擬成功だけのケースは代替検知先とともに整理される。
- Shown by: test — CLI の status・diff・merge・rollback の代表的な成功と拒否を故意に壊した状態で RED とし、`cargo nextest run --all-features --status-level fail` で GREEN にする。
- Left to the implementer: 同じ結果を検証する既存ケースの統合位置。
- Stop and hand back if: 現行 CLI の結果と承認済み製品 IR が食い違い、どちらを正とするか判断が必要になる。

### S7: Unix PTY の TUI テストを通常実行へ移す

- Purpose: 対話画面の起動・移動・検索・差分・マージ・三者比較を隔離環境で確認する。
- Specification: `docs/ir/testing/environments.md#REQ-testing-001`, `docs/ir/testing/environments.md#REQ-testing-002`, `docs/ir/testing/environments.md#REQ-testing-003`
- Prerequisites: S1 と S2、S6 で共通 fixture の安定性を確認する。
- May change: `tests/common/mod.rs`, `tests/tui_*.rs`, `tests/contract/*.rs`, `docs/testing/ignored-test-migration.md`
- Done when: 必要な PTY ケースは ignore なしで操作後の画面表示・対象ファイルを確認し、不正なサーバー名を使う1件も個人 SSH 設定なしに実行される。
- Shown by: test — 検索とマージの表示・実ファイル結果、三者比較での対象切替、存在しないサーバー名の起動拒否を RED→GREEN で試し、Unix の通常テスト一覧にスキップが残らないことを確かめる。
- Left to the implementer: PTY 出力から安定して確認できる製品の表示要素とタイムアウトの既存ヘルパーの再利用。
- Stop and hand back if: TUI の凍結範囲を超える製品コード変更が必要になる、または画面断片の固定だけでしか成功を判定できない。

### S8: 実 OpenSSH と sudo のコンテナ専用テストを作る

- Purpose: Rust fixture では証明できない実 sshd・所有者・権限・sudo の動作を別パッケージで確認する。
- Specification: `docs/ir/testing/environments.md#REQ-testing-004`, `docs/ir/testing/environments.md#REQ-testing-005`, `docs/ir/testing/environments.md#REQ-testing-003`
- Prerequisites: S1 と S6、Docker が利用可能な環境での実行は利用者が許可する。
- May change: `tests/container-e2e/`, `scripts/run-container-e2e.sh`, `tests/e2e_testenv.rs`, `tests/contract/ssh_sudo.rs`, `testenv/sudo_e2e.sh`, `Cargo.toml`, `.gitignore`, `docs/testing/ignored-test-migration.md`
- Done when: 独立パッケージの専用コマンドが固定ダイジェストの現行 OpenSSH と一時鍵・動的ポートを使い、sudo 成功時の書き込み・所有者・権限・旧内容バックアップを実ファイルで確認する。sudo を拒否するアカウントでも通常権限なら書けるファイルへの sudo 指定マージが非ゼロ終了し、元のファイルが変わらず非昇格の再試行も起きない。起動中・実行中の失敗で鍵・コンテナ・データが片付き、失敗が非ゼロ終了のまま残る。残すコンテナ依存ケースは旧環境変数なしに動く。
- Shown by: test — `scripts/run-container-e2e.sh` から EX-testing-004 と EX-testing-007 の成功・拒否を RED→GREEN とし、Docker 不足・イメージ不足・sshd 起動失敗・テスト途中失敗で非ゼロ終了と一時資源の撤去を確認する。通常 Cargo の一覧には専用ケースがなく専用パッケージの一覧にはあることを確認する。
- Left to the implementer: コンテナの固定ダイジェストと、旧コンテナ E2E のうち実ファイル結果に価値のあるケースの配置。
- Stop and hand back if: 実 OpenSSH イメージを固定・取得できない、root 所有の結果をホストで安全に検査できない、または必要な Agent 経路を再現できない。

### S9: CI と Lefthook を検証層に合わせる

- Purpose: 検証の失敗が push と CI に伝わり、ローカルの自動修正でコミット対象が変わらないようにする。
- Specification: `docs/ir/testing/environments.md#REQ-testing-001`, `docs/ir/testing/environments.md#REQ-testing-004`, `docs/ir/testing/environments.md#REQ-testing-005`, `docs/ir/testing/environments.md#REQ-testing-007`, `docs/ir/testing/environments.md#REQ-testing-008`
- Prerequisites: S6–S8 で通常テストと `scripts/run-container-e2e.sh` が動き、kotowari の公開 Git コミットを取得できる。
- May change: `.github/workflows/ci.yml`, `lefthook.yml`, `.claude/settings.json`, `.claude/hooks/pre-commit-format.sh`, `PROJECT.md`, `tests/container-e2e/`, `scripts/run-container-e2e.sh`
- Done when: pre-commit は作業ツリーの fmt と kotowari、pre-push は Clippy と通常テストを変更なしで実行し、CI は隔離 HOME で少なくとも一方の通常テストコマンドと、同じ四検査・必須コンテナ検証を PR と main push で実行する。旧鍵交換方式とサーバー別設定の検証は通常系で実行される。既存のローカル Git フックは上書きされず、手動移行方法が明記される。
- Shown by: check — `lefthook run pre-commit`, `lefthook run pre-push`, `kotowari check --format json` と、kotowari 不足・Docker 不足・失敗する通常テストをそれぞれ注入したランナーの非ゼロ終了を確認する。CI 設定で隔離 HOME の通常テストと PR・main push の各ジョブを確認し、既存の旧鍵交換方式・サーバー別設定のケースが実行されたログを確認する。
- Left to the implementer: CI のキャッシュ配置と、ローカルの既存フックを置換しない導入手順の説明文。
- Stop and hand back if: Lefthook の install が既存フックを安全に保持できない、公開 kotowari コミットが使えない、または CI でコンテナ実行権限がない。

### S10: 旧 SSH の手動負荷環境を分離して一試行で片付ける

- Purpose: 自動検証と無関係の旧サーバー負荷用途を分かる場所に移し、個人 SSH 設定への副作用をなくす。
- Specification: `docs/ir/testing/environments.md#REQ-testing-006`
- Prerequisites: S8 で旧 `testenv/` に自動テストの依存が残らないことを確かめる。
- May change: `testenv/`, `bench/legacy-ssh/`, `.gitignore`, `PROJECT.md`, `tests/contract/ssh_sudo.rs`, `tests/e2e_testenv.rs`
- Done when: 手動環境の生成物は試行ごとの専用領域に作られ、正常終了時・途中失敗時に鍵・設定・データ・コンテナが破棄され、利用者の既知ホスト情報と個人鍵は変わらない。
- Shown by: external — 利用者が隔離 HOME と Docker の試行環境で起動・終了と故意の途中失敗を確認し、試行前後の SSH 設定と一時資源を比較する。
- Left to the implementer: 旧データ生成器を試行用の一時領域へ移すスクリプトの内部構成。
- Stop and hand back if: 旧 CentOS 5 イメージを再現するためにホストのカーネル設定変更や利用者の個人 SSH ファイル変更が必要になる。

### S11: 対応表と全層の実行結果を照合する

- Purpose: 108件を件数でなく回帰検知力で閉じ、通常テストと専用検証の成功を別々に示す。
- Specification: `docs/ir/testing/environments.md#REQ-testing-001`, `docs/ir/testing/environments.md#REQ-testing-002`, `docs/ir/testing/environments.md#REQ-testing-003`, `docs/ir/testing/environments.md#REQ-testing-004`, `docs/ir/testing/environments.md#REQ-testing-005`, `docs/ir/testing/environments.md#REQ-testing-006`, `docs/ir/testing/environments.md#REQ-testing-007`, `docs/ir/testing/environments.md#REQ-testing-008`, `docs/ir/cli/symlink-diff.md#REQ-cli-020`, `docs/ir/cli/symlink-diff.md#REQ-cli-021`, `docs/ir/cli/symlink-diff.md#REQ-cli-022`, `docs/ir/cli/symlink-diff.md#REQ-cli-023`, `docs/ir/cli/symlink-diff.md#REQ-cli-024`, `docs/ir/cli/directory-paths.md#REQ-cli-025`, `docs/ir/cli/symlink-diff.md#REQ-cli-026`
- Prerequisites: S1–S10 と承認済み CI の権限を得たときの CI 実行結果。
- May change: `docs/testing/ignored-test-migration.md`, `PROJECT.md`, `tests/`, `bench/legacy-ssh/`, `lefthook.yml`, `.github/workflows/ci.yml`, S3–S5 で変更した CLI diff とログのテスト
- Done when: 対応表の108件の判断に代替なしの必要動作がなく、隔離 HOME での二つの標準コマンドは外部環境依存のスキップなし、専用コンテナの成功・失敗ケースは別に結果があり、CI が必須チェックを起動する。追加した CLI diff の全要件と例に振る舞いを検証するテストが紐づく。
- Shown by: check — `cargo test --all-features`, `cargo nextest run --all-features`, `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `scripts/run-container-e2e.sh`, `kotowari check --format json`, `git diff --check` の順に実行し、変更ファイルの指摘をゼロにする。CI の実行結果は push の許可を得た後に別途確認する。
- Left to the implementer: 検出力が重なるケースの最終的な統合位置。
- Stop and hand back if: 必要動作の検知先がなくなる、CI を実行できず必須ジョブの成功を確認できない、または未定義の製品動作が判明する。
