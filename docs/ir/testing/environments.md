# 再現可能なテスト環境

Linux CI と Unix 系の開発環境での通常テスト、実 OpenSSH 検証、手動の旧 SSH 負荷環境、開発時の自動チェックの保証範囲。

## Requirements

### REQ-testing-001: 通常テストを個人の接続環境なしで実行する
- kind: invariant
- source: docs/decision/records/2026-09-26-reproducible-test-environments.md#A1, docs/decision/records/2026-09-26-reproducible-test-environments.md#A4, docs/decision/records/2026-09-26-reproducible-test-environments.md#A5, docs/decision/records/2026-09-26-reproducible-test-environments.md#A13, docs/decision/records/2026-09-26-reproducible-test-environments.md#A19
- verification: review
- how_to_verify: Linux の隔離した HOME で cargo test --all-features と cargo nextest run --all-features を実行し、CI が少なくとも一方を実行することを確認する。Docker、個人の SSH 鍵、localhost:22 がなくても必要な CLI・TUI・SSH テストと旧鍵交換方式・サーバー別設定を検証するケースが外部環境不足でスキップされないことを実行ログとテスト一覧で確認する。コンテナ専用の Rust テストは主パッケージと独立したパッケージの専用コマンドでのみ収集する。

「通常テスト」はテスト自身が起動した接続先を使い、個人の鍵や常設の SSH サーバーを必要としない。

### REQ-testing-002: 隔離 SSH で実際の操作結果を検証する
- kind: invariant
- source: docs/decision/records/2026-09-26-reproducible-test-environments.md#A5, docs/decision/records/2026-09-26-reproducible-test-environments.md#A15
- verification: review
- how_to_verify: 必要な SSH テストが各テストで動的ポートと一時ディレクトリを使い、実際の CLI・TUI の表示とファイルの読み書き結果を確認することをテストと実行結果から確かめる。コマンド文字列や fixture の模擬応答だけを成功条件にしない。

通常の SSH テストは試行ごとに独立した接続先とファイルを持ち、失敗と成功を他のテストや利用者のファイルに波及させない。

### REQ-testing-003: 既存のスキップ事例を検知力で選別する
- kind: state_driven
- source: docs/decision/records/2026-09-26-reproducible-test-environments.md#A2, docs/decision/records/2026-09-26-reproducible-test-environments.md#A10, docs/decision/records/2026-09-26-reproducible-test-environments.md#D1
- verification: review
- how_to_verify: 移行時点の外部依存でスキップされた 108 件の各々について、残す・統合する・削除する理由、実際に見逃し得る不具合、移行先のテストまたは既存の代替検証を対応表で追跡し、必要な振る舞いが通常テストまたは実 OpenSSH 検証で実行されることを確認する。

件数を維持するために、実処理を呼ばない検証や同じ振る舞いの重複検証を残さない。

### REQ-testing-004: OS 固有の接続を実 OpenSSH で必ず検証する
- kind: invariant
- source: docs/decision/records/2026-09-26-reproducible-test-environments.md#A3, docs/decision/records/2026-09-26-reproducible-test-environments.md#A7, docs/decision/records/2026-09-26-reproducible-test-environments.md#A8, docs/decision/records/2026-09-26-reproducible-test-environments.md#A18, docs/decision/records/2026-09-26-reproducible-test-environments.md#A20, docs/decision/records/2026-09-25-spec-migration.md#A9, docs/decision/records/2026-09-25-spec-migration.md#A40
- verification: review
- how_to_verify: pull request と main 向け push の双方で必須 CI の独立ジョブがイメージを変更不能な識別子で固定した現行 OpenSSH コンテナを起動することを確認する。非対話 sudo ありの場合は旧内容のバックアップと更新後の root 所有・モードを、sudo なしの場合は書き込み先が変化しないことと非ゼロ終了を実ファイルと CI 実行ログから確かめる。

「実 OpenSSH 検証」は旧 SSH 負荷環境とは別のジョブであり、実際の sshd でマージと権限昇格の成功・拒否を対象にする。

### REQ-testing-005: 未実施を成功と区別する
- kind: prohibition
- source: docs/decision/records/2026-09-26-reproducible-test-environments.md#A3, docs/decision/records/2026-09-26-reproducible-test-environments.md#A9, docs/decision/records/2026-09-26-reproducible-test-environments.md#A14, docs/decision/records/2026-09-26-reproducible-test-environments.md#A17
- verification: review
- how_to_verify: Docker、イメージ、sshd の準備を意図的に失敗させて専用コマンドと CI ジョブが失敗することを確認する。途中失敗後に一時資源が残らず、通常テストは別の実行対象として結果が表示されることも確認する。

コンテナで検証すべき操作を、環境がないという理由で成功扱いにしない。

### REQ-testing-006: 手動の負荷環境を利用者の設定から隔離する
- kind: invariant
- source: docs/decision/records/2026-09-26-reproducible-test-environments.md#A8, docs/decision/records/2026-09-26-reproducible-test-environments.md#A12, docs/decision/records/2026-09-26-reproducible-test-environments.md#A16, docs/decision/records/2026-09-26-reproducible-test-environments.md#A17
- verification: review
- how_to_verify: bench/legacy-ssh/ で手動試行を開始・終了し、生成した鍵・設定・データ・コンテナが専用の作業領域で管理され、正常終了時も途中失敗時も破棄されることを確認する。試行前後で利用者の SSH 設定ファイルが変化しないことを確認する。

「旧 SSH 負荷環境」は通常テストや必須 CI の結果として扱わず、一回の手動試行ごとに生成物を分離する。

### REQ-testing-007: 開発時の検査でコミット内容を書き換えない
- kind: prohibition
- source: docs/decision/records/2026-09-26-development-check-hooks.md#A1, docs/decision/records/2026-09-26-development-check-hooks.md#A3, docs/decision/records/2026-09-26-development-check-hooks.md#A4, docs/decision/records/2026-09-26-development-check-hooks.md#A6
- verification: review
- how_to_verify: 共有の Lefthook 設定で pre-commit に fmt --check と kotowari check、pre-push に Clippy と Docker 不要の通常テストが配置され、どの検査もファイルやステージ内容を自動変更しないことを確認する。既存ローカル Git フックの上書きは導入手順に含めず、Claude Code 専用の自動修正フックが残っていないことを確認する。

チェックが不合格なら操作を止め、修正と再ステージは利用者が明示して行う。整形検査は作業ツリーが対象であり、ステージとの差でローカル検査を通過した場合は CI がコミット済みの内容を検査する。

### REQ-testing-008: CI で仕様検査も必須にする
- kind: invariant
- source: docs/decision/records/2026-09-26-development-check-hooks.md#A2, docs/decision/records/2026-09-26-development-check-hooks.md#A5
- verification: review
- how_to_verify: CI が公開 Git の固定コミットから kotowari を導入し、fmt・Clippy・通常テスト・kotowari check のそれぞれの失敗でワークフローが失敗することを確認する。ローカルのフックが未導入でも CI の検証結果が変わらないことを確認する。

CI の合格には通常テストと仕様検査の両方が必要である。

## Examples

```gherkin
@id=EX-testing-001 @about=REQ-testing-001 @source=docs/decision/records/2026-09-26-reproducible-test-environments.md#A13
Scenario: 個人の SSH 接続先を持たない Linux CI
Given Docker と利用者の SSH 鍵がなく localhost:22 にサーバーがない
When 通常テストの二つのコマンドを実行する
Then 必要な CLI・TUI・SSH ケースが接続先不足でスキップされずに実行される

@id=EX-testing-002 @about=REQ-testing-002 @source=docs/decision/records/2026-09-26-reproducible-test-environments.md#A5,docs/decision/records/2026-09-26-reproducible-test-environments.md#A15
Scenario: 別々のテストが SSH で同じ相対パスを扱う
Given 各テストに別の接続先と一時ディレクトリがある
When CLI と TUI がファイルを比較して更新する
Then 各テストの表示と書き込み先の内容は自分の一時ディレクトリだけを反映する

@id=EX-testing-003 @about=REQ-testing-003 @source=docs/decision/records/2026-09-26-reproducible-test-environments.md#A10,docs/decision/records/2026-09-26-reproducible-test-environments.md#D1
Scenario: 模擬応答だけを確認していたテストを移行する
Given 既存テストが実際の比較結果を確認していない
When 対応表で存廃と代替検証を決める
Then 件数合わせでは残さず、必要な振る舞いを確認する検証先を示す

@id=EX-testing-008 @about=REQ-testing-003 @source=docs/decision/records/2026-09-26-reproducible-test-environments.md#A10,docs/decision/records/2026-09-26-reproducible-test-environments.md#A9
Scenario: 実際の失敗を検知する唯一のテストが候補になる
Given 他のテストがその失敗を検出しない
When 移行するテストの存廃を判断する
Then その振る舞いを検証するテストを残して通常かコンテナの実行先に置く

@id=EX-testing-004 @about=REQ-testing-004 @source=docs/decision/records/2026-09-26-reproducible-test-environments.md#A7,docs/decision/records/2026-09-25-spec-migration.md#A9,docs/decision/records/2026-09-25-spec-migration.md#A40
Scenario: 権限が必要なファイルを実 sshd 経由で更新する
Given 一時コンテナ内の書き込み先は root 所有で指定した権限と旧内容を持つ
When 非対話 sudo が使えるアカウントからマージを行う
Then 書き込み先は新内容を持ち root 所有・元の権限を保ち、集約バックアップには旧内容が保存される

@id=EX-testing-007 @about=REQ-testing-004 @source=docs/decision/records/2026-09-26-reproducible-test-environments.md#A7,docs/decision/records/2026-09-26-reproducible-test-environments.md#A20
Scenario: 実 sshd で非対話 sudo が拒否される
Given 一時コンテナ内に非対話 sudo を使えないアカウントと既存ファイルがある
When 権限昇格を指定してマージを行う
Then マージは失敗し既存ファイルは変化せず通常権限への切り替えも行われない

@id=EX-testing-005 @about=REQ-testing-005 @source=docs/decision/records/2026-09-26-reproducible-test-environments.md#A3,docs/decision/records/2026-09-26-reproducible-test-environments.md#A14,docs/decision/records/2026-09-26-reproducible-test-environments.md#A17
Scenario: コンテナを起動できない
Given 実 OpenSSH 検証で Docker を利用できない
When 専用コマンドと CI ジョブを実行する
Then 検証未実施として失敗し一時資源を残さず、通常テストの成功と混同されない

@id=EX-testing-006 @about=REQ-testing-006 @source=docs/decision/records/2026-09-26-reproducible-test-environments.md#A12,docs/decision/records/2026-09-26-reproducible-test-environments.md#A16
Scenario: 手動の負荷試行を終了する
Given 旧 SSH 負荷環境を一回起動し利用者の既知ホスト情報がある
When 専用の終了手順を実行する
Then 生成した鍵・設定・データ・コンテナは残らず利用者の既知ホスト情報は変わらない

@id=EX-testing-009 @about=REQ-testing-007 @source=docs/decision/records/2026-09-26-development-check-hooks.md#A1,docs/decision/records/2026-09-26-development-check-hooks.md#A4,docs/decision/records/2026-09-26-development-check-hooks.md#A6
Scenario: 作業ツリーに未整形のファイルがある状態でコミットする
Given 作業ツリーの Rust ファイルが整形規約に合わない
When Lefthook の pre-commit を実行する
Then コミットは止まり、ファイルとステージ済みの内容は書き換わらない

@id=EX-testing-010 @about=REQ-testing-007 @source=docs/decision/records/2026-09-26-development-check-hooks.md#A1
Scenario: push 前の検査で通常テストが失敗する
Given Docker 不要の通常テストに失敗する変更がある
When Lefthook の pre-push を実行する
Then push は止まり、変更したファイルは書き換わらない

@id=EX-testing-011 @about=REQ-testing-008 @source=docs/decision/records/2026-09-26-development-check-hooks.md#A2,docs/decision/records/2026-09-26-development-check-hooks.md#A5
Scenario: ローカルフックなしの変更で仕様検査が失敗する
Given kotowari check がエラーを報告する変更が push された
When CI が公開 Git の固定コミットから kotowari を導入して検証する
Then 仕様検査の手順が失敗し、CI 全体も失敗する
```
