# 最初のcore crate抽出

## Goal

製品の挙動と既存公開パスを維持し、tree・diff・error・filterを内部crate remote-merge-coreへ分離して既存の検査と配布を継続できるようにする。

## Specification

承認・コミット済みの[core抽出の決定記録](../decision/records/2026-10-02-core-extraction.md#Agreements)のA1〜A5、ProhibitionsのP1、DelegatedのD1〜D3を根拠とする。新しいIRは作らない。

[検証手順の決定記録](../decision/records/2026-10-02-core-extraction-verification.md#Delegated)は計画と同時に承認を受ける草稿である。

## Approach and why

- モジュール実装は既存の4モジュールを採用し、内容をそのまま移す。エラー型を分けず、既存の型と変換を再利用する。
- 互換性はRustのpub useを採用する。同じ型を再公開することで既存のremote_merge::tree等の利用側を変更しない。
- パッケージ境界はCargoのworkspaceとpath依存を採用する。互換ラッパー、新しい実行時依存、workspace.dependenciesへの共通化は不要である。
- 検証は既存の単体・統合・契約テストとCargo・kotowari・変異検査を採用する。構造固定のテストは追加せず、検証用の一時資料はGit管理から外す。

## Scope of change

- ルートCargo.tomlのmembers/default-members、crates/remote-merge/Cargo.tomlのcoreへのpath依存、crates/remote-merge/src/lib.rsの4宣言を再公開へ置換。
- 新しいcrates/remote-merge-core/Cargo.tomlとsrc/lib.rs。既存tree.rs・error.rs・filter.rs・diff/全体をsrc/へ移す。内包する単体テストも内容・モードを変えずに移す。
- Cargo.lockの新しいローカルパッケージと依存辺だけ。既存外部パッケージの版・チェックサムは変えない。製品側の既存依存は今回削除・縮小しない。
- .kotowari/mutants-equivalents.yamlのdiff/engine.rsにある2件のfile指定だけ。
- scripts/mutants.shの両crateと従来の契約テストを選ぶためのworkspace/package選択処理、説明の必要な追従だけ。
- PROJECT.mdの現在の配置・版の正本・公開パスの説明、README.mdにソースインストールで両crateのcheckoutが必要なことの補足が必要な場合の記述。
- .kotowari/changes/implementation.yamlとreview.yamlの今回の比較への置換。実装者と独立レビュー担当がそれぞれ作成する。
- .agents/artifacts/core-extraction/のローカル資料だけ。新しい決定が必要なら別の決定記録へ根拠と役割を記録し、合意から外れる意味変更は利用者へ戻す。

## Step order and prerequisites

- 計画と検証手順の承認・コミット後、mainからrefactor/core-extractionを作る。比較元は7d7b13eの完全なIDをgit rev-parseで確定し、計画コミットを含むブランチ全体で固定する。
- 現在のcheckoutを一人の実装者が使用する。独立レビューは実装者の作業終了後に行い、review.yamlの作成だけを独立担当へ渡す。push・mergeはしない。
- S1の採取、S2の抽出、S3の検査設定と説明、S4の前後比較、S5の独立レビューと変更照合の順に行う。
- S1時点の旧両記録は前回の比較の資料であり、checkは形と参照だけを確認する。新baseでの鮮度・網羅性を示すものではない。S2開始時に旧implementation.yamlとreview.yamlを取り除き、S5で今回の比較から両役割が独立に作成し直す。
- 全Cargo実行はsystemd-run --userでMemoryMax=40%、MemorySwapMax=0、CPUWeight=idle、Nice=19を指定する。既存mutants.shのより細かい制限・OOM判定も維持する。必要な非機密のPATH・toolchain変数のみ引き継ぎ、HOME・XDG_CONFIG_HOME・XDG_DATA_HOME・TMPDIRはローカル資料配下へ隔離する。CARGO_HOME/RUSTUP_HOMEは元の場所を維持する。
- commitもnice -n 19かつ同じsystemd制限で行い、フックは無効化・置換しない。Gitの既存のauthor/committer設定を使い、隔離HOMEで見えない場合は非機密の既存Git設定だけをコマンド限定で渡す。個人設定の書き換えや仮のidentityの捏造はしない。
- Cargoと変異の生成物は既存のroot target、一時資料・隔離設定・インストール先は.agents/artifacts/core-extraction/に置く。既存テストの一時ファイルは許可し、tracked差分へ混ざっていないことを確認する。
- nextest、cargo-mutants、kotowari 0.3.0、systemdの実行条件を先に確認する。利用できない必須ツールを導入・代替して進めず、戻す。

## Verification map

| 根拠 | 検証対象 | 手順 |
|---|---|---|
| A1、D3 | 移動内容・モード、再公開、利用側の型互換性 | S1、S2、S4 |
| A2、P1 | 新しいIR・製品挙動・テストの追加や変更がない | S2、S5 |
| A3、D2 | 既存外部依存の同一性、170単体テストの所属変更、統合テストの維持 | S1、S2、S4 |
| A4 | coreの内部版・publish=false、製品版・インストール・配布 | S2、S4 |
| A5 | 両crateの通常検査、変異検査の選択 | S3、S4 |
| 検証手順D1〜D4 | 前後比較と両役割のブランチ全体の変更照合 | S1、S4、S5 |

## Left to the implementer

- coreの通常依存は承認済みD2の9依存を既存指定で引き継ぎ、serde_jsonはdev-dependencyとする。path依存の相対表記とmanifestの整形はCargoの標準に従う。
- 製品src/lib.rsはpub use remote_merge_core::{tree, diff, error, filter}による同名再公開とする。利用側のimport変更、型ラッパー、feature追加は不要である。
- mutants.shの選択は既存ツールで一覧と実行を試し、coreの単体と製品のcontract・cli_diffが同じ検査へ参加する最小の指定にする。名前付き統合targetを持たないcoreに対し誤ったtarget指定が出ないことを確認する。
- コミットの分割とローカル資料の形式。単位は1関心、stageは個別パス。生ログや機密・環境固有値をコミットしない。

## Stop conditions

- 対象外モジュールへの依存、移動した型への外部trait実装に伴うorphan違反、内容・公開範囲・テスト期待値の変更が必要になった場合は根拠を示して戻す。
- lockfileの既存外部パッケージ更新、依存指定やfeatureの変更が必要なら戻す。新ローカルパッケージの追加だけの差分は許可する。
- default-members変更によって複数binaryの選択や既存のrun/install/buildに不整合が出て、今回の範囲で維持できないなら戻す。
- mutantのパス・テスト選択・同等変異の照合が範囲内の補正で成立しない場合は戻す。検査範囲の縮小や同等変異の追加で回避しない。
- Dockerやmuslの利用不可は未検証として明記し、権限変更・ツール導入で補わない。macOSはこのLinux環境では未検証とする。
- 最終checkとreview-phase changesが両方終了コード0になるまで統合可能と報告しない。既存FLAGによるstatus complete falseは今回解消しない。

## Test command

コマンドは前述の制限と隔離内で、S1・S4に同じ順で実行して出力と終了コードを保存する。

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

- nextestの各package/targetのテスト名と無視状態を比較し、移動した170単体テストだけのpackage・library target・test binaryの所属をremote_mergeからremote_merge_coreへ対応付ける。テスト名と無視状態は変えず、他のpackage/targetは対応変更しない。実行ファイルの環境依存パス・ハッシュだけは正規化できる。総数3109の維持だけでは判断せず、旧テストごとの移動先を確認する。doc-testもcargo testの結果で確認する。
- metadataとmanifest/lockfileから公開target・features・外部依存・製品版・profileが変わらず、新coreとpath依存だけが追加されたことを確認する。両default-members・共通root target・Docker E2Eの独立性を確認する。
- cargo run -- --helpで既存のbinary選択が保たれることを確認する。抽出前後のcargo install --path crates/remote-merge --locked --root <隔離先>を別々の場所に行い、生成binaryの--version・--help・agent --helpを比較する。
- 実行環境が利用できる場合にcargo build --locked --release --target x86_64-unknown-linux-musl --bin remote-merge、scripts/run-container-e2e.shを実行する。通常のroot buildもmetadataとdefault testで両crateが対象になることを確認する。
- cargo mutants --list --all-featuresでapply_selected_hunks_single_passの変異を抽出前後に列挙し、テスト選択も確認する。scripts/mutants.sh --re 'replace match guard tag == replace_tag with true in apply_selected_hunks_single_pass$' --re 'replace < with <= in apply_selected_hunks_single_pass$' <旧または新diff/engine.rs>で同じ2件を実行する。既存の2同等変異と結果の照合を確認し、全変異は回さない。
- 現行CI・lefthook・pre-push・coverageのCargo対象を静的に確認し、default-membersによる両crateの選択と一致させる。これらの内容は今回変更しない。coverage実測はツールが利用可能なら既存scripts/coverage.shの条件で行い、利用不可なら未検証として分ける。

## Out of scope

- 残りのcrate抽出、循環依存解消、coreの純化、エラー体系分割、製品のバグ修正や挙動変更。
- 新しいIR・要件・テスト・構造固定用の検査、既存テストの期待値・印の変更、製品版の変更、外部依存更新や共通化。
- crates.io公開、CIやフックの新しいゲート、全変異・過去FLAGや同等変異の再審査、Docker E2Eの移動、proptest保存先の変更。
- 個人設定変更、push・merge・リリース・計画削除・ブランチ削除。

## Steps

### S1: 抽出前の内容と検証結果を採取する

- Purpose: 抽出による差分と既存の問題を区別できる比較資料を作る。
- Specification: docs/decision/records/2026-10-02-core-extraction.md#Agreements, docs/decision/records/2026-10-02-core-extraction-verification.md#Delegated
- Prerequisites: 計画と検証手順が承認・コミットされ、実装ブランチ・完全なbase ID・cleanな作業ツリーが確認できること。
- May change: .agents/artifacts/core-extraction/のローカル資料だけ。
- Done when: 比較元と対象モジュールのblob/mode・依存・全テスト集合・実行結果・インストール表示・限定した変異の一覧と結果が保存されている。
- Shown by: check 必須ツール確認後にTest commandの順で実行し、前後比較用資料を保存する。
- Left to the implementer: ローカル資料の名前と機密のない要約方法。
- Stop and hand back if: 必須ツールがない、検証が抽出前から失敗して原因を区別できない、個人設定の変更が必要である。

### S2: 内容を維持してcoreへ移し、既存パスへ再公開する

- Purpose: 4モジュールを独立packageに所属させ、利用側の公開パスと型の同一性を保つ。
- Specification: docs/decision/records/2026-10-02-core-extraction.md#Agreements, docs/decision/records/2026-10-02-core-extraction.md#Delegated
- Prerequisites: S1が完了していること。
- May change: root/member/new coreのCargo.toml、Cargo.lock、製品lib.rsの4宣言、新core src/lib.rsの4公開宣言、対象4モジュールの移動元と移動先、旧implementation.yamlとreview.yamlの削除だけ。
- Done when: 両crateがdefault-memberであり、coreは内部版0.1.0、製品版・既存外部依存は同じで、4モジュールの内容とmodeが完全一致し、既存利用側が再公開された同じ型を使用する。
- Shown by: check metadata、fmt、Clippyとblob/mode比較を行い、coreの170単体テストと製品の統合テストが収集されることを確認する。
- Left to the implementer: manifestの整形と移動順。依存・モジュール内容は変えない。
- Stop and hand back if: 内容変更やorphan違反の解消が必要、型の公開範囲が変わる、既存外部依存の版・feature変更が必要である。

### S3: 変異検査の対象と現行の説明を追従する

- Purpose: coreに移った処理も従来の単体・契約テストと同等変異の照合で検査する。
- Specification: docs/decision/records/2026-10-02-core-extraction.md#Agreements, docs/decision/records/2026-10-02-core-extraction-verification.md#Delegated
- Prerequisites: S2の配置と依存が成立していること。
- May change: scripts/mutants.shの対象選択と必要な説明、.kotowari/mutants-equivalents.yamlの対象2パス、PROJECT.md、必要なREADME.mdのソースインストール補足だけ。
- Done when: coreの単体と製品contract・cli_diffが変異検査に参加し、同等変異のパスは実測一覧と一致し、説明が両crateの配置と内部版を指す。
- Shown by: check bash -n scripts/mutants.sh、変異とテストの一覧確認、kotowari check、差分確認を行う。
- Left to the implementer: 最小のCargo対象指定と同じ意味を保つ説明。
- Stop and hand back if: 検査対象の削減や同等変異の追加が必要、CIやフックの対象変更がないと通常検査が成立しない。

### S4: 抽出前後を比較して既存の検証と配布を確かめる

- Purpose: 同じ既存テストと配布経路が両crateを使う新配置でも成立することを確認する。
- Specification: docs/decision/records/2026-10-02-core-extraction.md#Agreements, docs/decision/records/2026-10-02-core-extraction-verification.md#Delegated
- Prerequisites: S2とS3が完了していること。
- May change: .agents/artifacts/core-extraction/の検証資料だけ。修正は該当する先行ステップの許可範囲に戻す。
- Done when: Test commandが利用可能な環境で成功し、170単体テストの所属変更以外にテスト集合差分がなく、インストール表示と内容・mode・依存の比較が一致し、限定した変異が実行・照合できる。
- Shown by: check Test commandの順で抽出後の検証を行いS1と比較し、未実行環境を別記する。
- Left to the implementer: 比較資料の形式と許可した170テストの所属package・library target・test binaryの対応付け、環境パスの正規化方法。
- Stop and hand back if: テストの削除・追加・期待値変更でしか通らない、mutantsの対象選択と結果照合が範囲内で補正できない。

### S5: 両役割の記録と独立レビューを完了する

- Purpose: 今回のブランチ全体が合意と一致することを実装者と独立レビュー担当の両方で確認する。
- Specification: docs/decision/records/2026-10-02-core-extraction.md#Agreements, docs/decision/records/2026-10-02-core-extraction-verification.md#Delegated
- Prerequisites: S4の証拠と実装コミットがあり、比較元と候補headが固定されていること。
- May change: .kotowari/changes/implementation.yamlは実装者、review.yamlは独立したレビュー担当が今回の比較で作成する。旧両記録はS2で削除済みであり、実装者は自分の記録だけを作成して呼出元へ返す。
- Done when: 独立レビューに修正すべき指摘がなく、旧新全パスを両役割がカバーし、最終checkとreview-phase changesが終了コード0である。
- Shown by: check 呼出元が独立レビューとreview.yaml作成を依頼し、kotowari check --format jsonとkotowari changes --base <固定した完全base ID> --head <記録コミット後の完全head ID> --phase review --format jsonを実行する。
- Left to the implementer: 自分の記録のID・グループ。独立レビューとreviewerの記録は呼出元が別の担当へ渡す。
- Stop and hand back if: 独立担当がレビューや記録を完了できない、根拠の結論が矛盾する、修正が承認範囲を越える。
