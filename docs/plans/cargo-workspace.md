# Cargo ワークスペースへの配置変更

## Goal

既存機能を変えず、既存パッケージをcrates/remote-mergeに置いたCargoワークスペースとしてビルド・検査・ソースインストールできるようにする。

## Specification

根拠は承認・コミット済みの[決定記録](../decision/records/2026-10-02-cargo-workspace.md#Agreements)。A2〜A6を対象とする。新しいIRは作らない。

配置と検証手順の具体化は[実装・検証手順の決定記録](../decision/records/2026-10-02-cargo-workspace-verification.md#Delegated)に記録した。こちらは計画と同時に承認を受ける草稿であり、承認前に実装を開始しない。

既存テストIRの配置指定だけを更新する対象は[REQ-testing-010](../ir/testing/methods.md#REQ-testing-010)、[REQ-testing-013](../ir/testing/methods.md#REQ-testing-013)、[EX-testing-014](../ir/testing/methods.md#Examples)。検証方針や要件IDは変更しない。

## Approach and why

- パッケージ配置はCargoの標準ワークスペースを採用する。独自のビルドラッパーや新しい依存は不要であり、配置変更を責務分割から切り離せる。
- パッケージ・テスト・build scriptは既存のものをそのまま再利用する。Rustコードで内容を変えるのは、A5に従ったproptestの保存先指定だけとする。
- 検証はCargo、既存テスト、kotowari、既存の変異テスト用スクリプトを再利用する。方針やディレクトリ構造を固定する新しいテストは作らない。
- ドキュメントは既存のPROJECT.md、README.md、リリース手順、決定記録を利用する。過去の判断や実行結果に書かれたパス・行番号は書き換えない。

## Scope of change

- ルートCargo.tomlと新しいcrates/remote-merge/Cargo.toml。Cargo.lockと.cargo/config.tomlはルートに残し、依存とlockfileの内容は維持する。
- trackedのbuild.rsとsrc/全体をcrates/remote-mergeへ移す。trackedのtests/はcontainer-e2e/を除いて同じパッケージへ移す。未追跡のファイル・生成物・個人設定は移さない。
- 移動後のcrates/remote-merge/tests/contract/backup_sessions.rsのfailure_persistenceのパス指定。
- .kotowari/config.yamlのtests.filesとchanges.files、.kotowari/mutants-equivalents.yamlのfile指定。
- README.mdのソースインストール、PROJECT.mdの配置説明、.claude/commands/release.mdのバージョン読取・変更先。
- docs/ir/testing/methods.mdの具体的な配置表記と、その配置更新を裏付けるsource。必要な訂正は新しい決定記録に追加し、既存の承認済みの決定は改変しない。
- scripts/mutants.shは、隔離したHOME・XDG_CONFIG_HOME・XDG_DATA_HOME・TMPDIRを内側のsystemdサービスへ引き継ぐ補正と、新配置で不整合が確認された対象選択・パス処理の補正だけを許可する。既存の実行制限・テスト選択・判定は緩めない。
- .kotowari/changes/implementation.yamlとreview.yaml。担当者はそれぞれ実装者と独立したレビュー担当に分ける。S5では実装者がimplementation.yamlを作成して実装結果を返し、その後のreview.yamlの作成と最終照合は呼出元が独立した担当へ渡す。実装者にS5の独立レビューを完遂させない。
- ローカルの検証ログと比較用データは.agents/artifacts/配下だけに置き、コミットしない。

## Step order and prerequisites

- 実装ブランチはrefactor/cargo-workspace。計画承認後のmainから作成し、未pushのローカル作業を基にする。比較元は0700f9f64e35de5b91a09beda48c7b85a37f289bに固定する。比較元以降の承認済み計画も含めてブランチ全体を比較する。
- この作業のwriterは一人とし、現在のcheckoutを使用する。調査・レビュー担当は書き込まない。ただし最終review.yamlの作成だけは実装者の作業終了後に独立したレビュー担当へ渡す。
- S1で現状を採取してからS2を行う。S2の配置と設定をそろえた後にS3の説明を更新する。S4で移動前後の検証を比較し、S5で独立レビューと変更照合を完了する。
- nextestとcargo-mutantsは必須の検証ツールとする。scripts/mutants.shがnextestを使うため、どちらかが使えなければ移動前に戻し、別のテスト選択やツールの導入で補わない。
- 全てのローカルCargo実行はsystemd-run --userのMemoryMax=40%、MemorySwapMax=0、CPUWeight=idle、Nice=19の中で行う。必要な非機密のPATH、RUSTUP_TOOLCHAIN、RUSTUP_HOME、CARGO_HOME、MISE_GLOBAL_CONFIG_FILEと、隔離先のHOME・XDG_CONFIG_HOME・XDG_DATA_HOME・TMPDIRだけを引き継ぐ。認証情報は渡さない。
- テストとインストール検証は隔離したHOME・XDG_CONFIG_HOME・XDG_DATA_HOMEで行う。CARGO_HOME・RUSTUP_HOMEは隔離前の場所を維持する。保持するログ・比較資料・隔離した設定とインストール先・TMPDIRは.agents/artifacts/cargo-workspace/配下とする。Cargoと変異検査の生成物は従来のルートtargetへ出力し、既存テストがパッケージのCWDなどに一時ファイルを作ることは許可するが、終了後に生成物がtracked差分へ混ざっていないことを確認する。個人の設定やインストール先は使わない。
- scripts/mutants.shの内側のsystemdサービスは呼出元の環境を自動継承しない。S1の最初に隔離先の環境変数を明示的に引き継ぐ補正を行い、シェル構文と実行時の環境受渡しを確認してから、同じ補正済みスクリプトで移動前後を検証する。ユーザーマネージャの共有環境を変更して回避しない。
- git commitはnice -n 19で、Cargoを実行する既存フックごと上記の制限内で行う。フックを無効化・置換・再インストールしない。1関心1コミット、git addは個別パスを指定する。

## Verification map

| 根拠 | 確認する内容 | ステップ |
|---|---|---|
| 決定記録A2 | パッケージ、依存、ターゲット、テスト収集、製品コードの同一性、既存動作 | S1、S2、S4 |
| 決定記録A3 | 新しいIR・要件・テストがないこと、開発方針の記録先 | S3、S5 |
| 決定記録A4 | 独立E2Eの配置・独立ワークスペース・専用コマンドの維持 | S2、S4 |
| 決定記録A5 | proptestの保存先を新旧パスの解決結果と既存テストで確認 | S2、S4 |
| 決定記録A6 | 既存テストIRのパス更新だけで、ID・方針・印の対応を維持 | S2、S3、S5 |
| 変更照合 | 比較元以降の変更を両担当者が根拠付きで照合 | S5 |

## Left to the implementer

- ファイル移動とコミットの分割順、ローカル検証ログのファイル名。内容・機能・テストは変えない。
- ワークスペースは唯一のmemberとdefault-memberをcrates/remote-mergeにする。edition 2021に対応するresolverを明示し、release profileはルートに置く。workspace.dependenciesなどの共通化は不要。
- tests.filesは新しい通常テストの配置に合わせ、tests/container-e2e/tests/sudo.rsは維持する。changes.filesにはcrates/**を追加し、旧パスの削除とルートの独立E2Eも比較対象に残す。
- 同等変異のfile指定は実際に新配置で得た変異一覧のパスと照合する。change、text、class、whyは変更しない。既存の同等変異やFLAGを今回再判定しない。
- 変更記録のグループ・IDは担当者に任せる。新しい配置の根拠はこの決定記録を参照し、IRを含むエントリは更新したmethods.mdの内容を照合する。全ての旧パスの削除・新パスの追加・関連設定を含め、リネーム表示だけで片側を省略しない。

## Stop conditions

- 実装ロジック、依存バージョン、公開API、テストの期待値・印・選択範囲を変えないと通らない場合は、原因と証拠を示して戻す。負荷で落ちた疑いだけなら同じ対象を再実行し、既存の失敗と今回の変更を区別する。
- 既存のCargo.lockに依存更新を伴う差分が出た場合は、resolverと実行条件を調べる。原因を説明できないままlockfile更新を採用しない。
- 設定・同等変異のパスを新配置へ追従させても、kotowariやcargo-mutantsの対応が成立しない場合は結果を示して戻す。検査対象や除外を広げて回避しない。
- Docker専用テストを通常ワークスペースへ入れる、保存先を変える、開発用IRや新しいテストを追加するなど、A2〜A6から外れる必要が出た場合は戻す。
- Docker・muslなどの実行環境が利用できない場合は、実施できた検証と未検証を分けて報告する。権限変更、環境への導入、push、リリースで補わない。
- 統合は独立レビュー、kotowari check、review-phaseのkotowari changesが全て通るまで行わない。実行できなかった配布環境の検証は明記し、結果を成功扱いしない。

## Test command

以下のコマンドは前述のsystemdと環境隔離の条件内で実行する。通常テストはリポジトリルートからdefault-memberを対象にする。

```sh
cargo metadata --no-deps --format-version 1
cargo fmt --all --check
cargo fmt --manifest-path tests/container-e2e/Cargo.toml --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo test --locked --all-features
cargo nextest list --all-features --message-format json
cargo nextest run --all-features
kotowari check --format json
git diff --check
```

- テスト集合は移動前後で同じnextestとfeaturesを使い、各ターゲットのテスト名・無視状態を比較する。Cargoのtest --listは補助として使用できるが、nextestを必要とする変異検査の代替にはしない。絶対パスとpackage IDの配置部分だけを正規化し、件数だけで同一と判定しない。
- 移動前のmetadataからpackage名・version・edition・依存指定・features・ターゲット名/種別/required-featuresを採取する。release profileはmetadataに含まれないため、移動前のmanifestから採取し、移動後のルートmanifestと比較する。移動後はこれらが同じで、workspaceの唯一のmember/default-memberとルートtargetが正しいことを確認する。build.rsの出力とAgentのバージョン表示も維持する。
- rootとmemberでcargo metadataを実行し、target-directoryが共通のルートtargetであることを確認する。独立E2Eのmetadataはtests/container-e2e/Cargo.tomlを指定し、通常workspaceに混ざっていないことを確認する。
- ソースインストールは移動前にcargo install --path . --root <隔離先> --locked、移動後にcargo install --path crates/remote-merge --root <別の隔離先> --lockedで実際に行い、生成されたremote-mergeの--version・--help・agent --helpを比較する。個人のbinは変更しない。
- musl環境が使える場合はcargo build --locked --release --target x86_64-unknown-linux-musl --bin remote-mergeを実行し、ルートtarget内の生成物を確認する。macOSのrelease matrixはこのLinux環境では未検証として残す。
- Dockerが使える場合だけscripts/run-container-e2e.shを制限内で実行し、TMPDIRを前述の隔離先へ指定する。利用できないときは、独立E2Eとスクリプトの不変性・manifestの成立までを確認し、実行結果は未検証とする。
- 変異一覧は移動前後ともcargo mutants --list --all-featuresで、src/backup/mod.rsのbackup_timestampと対応する新パスの同じ関数に絞って取得する。workspaceの一覧パスと同等変異のfile指定の一致を確認し、実際の実行はscripts/mutants.sh --re ' in backup_timestamp$' --re 'replace backup_timestamp -> ' <対象ファイル>で行う。移動後のテスト収集とkotowariによる同等変異の照合を確かめる目的であり、移動していない他の関数の変異は回さない。

## Out of scope

- core・agent・ssh・tuiなどのcrate抽出、依存循環の解消、リファクタリング、バグ修正。
- 依存更新、バージョン変更、workspace依存の共通化、新しいテスト・IR・要件の追加、既存テストの期待値や印の変更。
- Docker専用パッケージの移動・統合、全変異の実行、既存FLAGの解消、同等変異の再判定。
- 過去のdecision recordsやdocs/testingのパス・行番号の一括更新、個人フックの置換、push・マージ・タグ・リリース。

## Steps

### S1: 移動前の比較資料を採取する

- Purpose: 配置変更で失われたターゲットやテストと、以前からある問題を見分けるための比較資料を採取する。
- Specification: docs/decision/records/2026-10-02-cargo-workspace.md#Agreements
- Prerequisites: 承認済みの計画がコミットされ、実装ブランチと比較元が固定され、作業ツリーがきれいであること。
- May change: .agents/artifacts/cargo-workspace/配下のローカル検証資料と、scripts/mutants.shの内側のsystemdサービスへ隔離先の環境変数を引き継ぐ処理だけ。製品コードやテストは変更しない。
- Done when: 移動前のmetadata、テスト集合、通常テスト結果、インストールしたbinaryの表示、限定した変異の一覧・実行結果、trackedファイルのモードとblobが記録されている。
- Shown by: check 検証ツールの存在を確認し、scripts/mutants.shの環境受渡しを補正してbash -nと実行時の確認を行う。Cargo metadata、manifestのprofile採取、通常テスト、ソースインストール、限定した変異の一覧とscripts/mutants.shをTest commandの条件で実行し、出力と終了コードを保存する。
- Left to the implementer: ローカル検証資料のファイル名と機密のない要約方法。
- Stop and hand back if: nextestまたはcargo-mutantsが使えない、移動前から検証が失敗して原因を区別できない、比較元が履歴から取得できない、個人設定の変更なしでは検証できない。

### S2: パッケージと検査設定を新配置へ移す

- Purpose: 既存パッケージの内容を保ち、Cargo・テスト・kotowariが同じ対象を新配置から読めるようにする。
- Specification: docs/decision/records/2026-10-02-cargo-workspace.md#Agreements
- Prerequisites: S1の比較資料があること。
- May change: Cargo.toml、新規crates/remote-merge/Cargo.toml、trackedのbuild.rs・src/・tests/の移動先と移動元（tests/container-e2e/を除く）、移動後backup_sessions.rsの保存先指定、.kotowari/config.yaml、.kotowari/mutants-equivalents.yaml。
- Done when: 通常パッケージは唯一のmember/default-memberであり、Docker E2Eは独立のまま、全ての移動ファイルの内容とモードは許可した保存先補正以外同じで、設定が新配置を検査対象に含み、proptestの保存先はリポジトリ直下のままである。
- Shown by: check 移動前後のmetadataとblob/modeの比較、通常と独立E2Eのmetadata、cargo fmt、kotowari checkを実行し、失敗入力の新旧指定の解決先を確認する。
- Left to the implementer: ファイル移動と設定更新の作業順。依存・機能・テスト内容は変えない。
- Stop and hand back if: 相対参照が保存先補正以外でも壊れる、未追跡ファイルの移動が必要、lockfileや依存指定の変更なしではmetadataが成立しない。

### S3: 現行の手順とパス表記を更新する

- Purpose: ソースインストール・リリース時の版の参照先・検査範囲の説明を実際の配置に一致させる。
- Specification: docs/decision/records/2026-10-02-cargo-workspace.md#Agreements
- Prerequisites: S2の配置と検査設定が成立していること。
- May change: README.md、PROJECT.md、.claude/commands/release.md、docs/ir/testing/methods.mdのパス表記とsource、必要な追加の決定記録。
- Done when: 現行の手順が新しいパッケージを指し、バージョンの正本はmemberのmanifestだけで、既存テストIRの方針・IDは変わらず、新しいIRや要件は増えていない。
- Shown by: artifact 更新した文書の差分とkotowari checkの結果。機械的なパス追従であることを独立レビューが確認できる状態にする。
- Left to the implementer: 同じ意味を保つ説明の言い回しと追加記録のファイル名。承認済みの決定を改変しない。
- Stop and hand back if: パス追従以外のIR改訂や、未承認の開発方針・配布仕様を追加する必要がある。

### S4: 既存の検証と配布経路を新配置で確かめる

- Purpose: 配置変更によって通常検査・限定した変異検査・ソースインストールが欠けていないことを移動前の結果と比較する。
- Specification: docs/decision/records/2026-10-02-cargo-workspace.md#Agreements
- Prerequisites: S2とS3が完了していること。
- May change: 実際の不整合が確認された場合のscripts/mutants.shのパス・対象選択処理、.agents/artifacts/cargo-workspace/配下の検証資料だけ。
- Done when: Test commandのローカルで可能な検証が通り、テスト名と無視状態の集合・binaryの表示・依存とターゲットが一致し、限定した変異が新配置で実行・照合でき、未実行の環境検証は別に記録されている。
- Shown by: check Test commandに記載した移動後の検証を順に実行し、S1の記録と比較する。生成物はコミットしない。
- Left to the implementer: 比較用データの絶対パスを正規化する方法。テスト名・期待値・featureや検査範囲は正規化で消さない。
- Stop and hand back if: 製品の挙動やテストを変更しないと同一の検証が通らない、変異の一覧と同等変異の照合の不整合を範囲内で直せない。

### S5: 独立レビューとブランチ全体の変更照合を完了する

- Purpose: 配置変更と関連設定・文書が合意に従い、同じ変更を実装者と独立した担当者がそれぞれ照合したことを確認する。
- Specification: docs/decision/records/2026-10-02-cargo-workspace.md#Agreements
- Prerequisites: S4の結果があり、コードと関連文書の変更がコミットされ、候補headが固定されていること。
- May change: .kotowari/changes/implementation.yamlは実装者、review.yamlは独立したレビュー担当が作成する。必要な根拠の補足は新しい決定記録に記録する。レビューによる修正は各先行ステップの許可範囲に限る。
- Done when: 独立レビューで修正すべき指摘が残らず、両役割の記録が固定した比較元から候補headまでの全変更と関連IRをカバーし、checkとreview-phaseのchangesが終了コード0である。
- Shown by: check kotowari check --format jsonとkotowari changes --base 0700f9f64e35de5b91a09beda48c7b85a37f289b --head <記録コミット後の完全なhead> --phase review --format jsonを実行する。同じbaseとhead・検証結果・未検証事項を報告する。
- Left to the implementer: 実装者の記録のIDとグループ分け。独立したreviewerの判断と記録は実装者には委任しない。
- Stop and hand back if: 独立したレビュー担当が記録を書けない、同じ変更について根拠や結論が矛盾する、修正によりA2〜A6の範囲を越える必要がある。
