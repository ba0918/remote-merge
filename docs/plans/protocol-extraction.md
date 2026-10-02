# 通信プロトコルの内部crateへの抽出

## Goal

通信動作・既存公開パス・製品版表示とAgent互換性判定を維持し、プロトコルを内部crate remote-merge-protocolへ分離する。

## Specification

根拠は承認・コミット済みの[抽出方針](../decision/records/2026-10-03-next-crate-extraction.md#Agreements)のA2〜A4と[具体化](../decision/records/2026-10-03-next-crate-extraction.md#Delegated)D1〜D2。新しいIRは作らない。
[検証手順](../decision/records/2026-10-03-protocol-extraction-verification.md#Delegated)は計画と同時に承認を受ける草稿である。

## Approach and why

- 通信処理・型・テストは既存protocol.rsを採用し、版構成以外は内容を変えない。
- 互換性はCargoのpath依存とRustの再公開を採用する。型ラッパーや別シリアライズ実装は不要である。
- 版定数は承認済みD1の単一リテラルからのマクロ生成を採用する。新しい依存やbuild scriptを使わず、既存のconst型と文字列を保持する。
- 検証は既存単体・Agentのプロセス/配布・契約テストと前後比較を採用する。新しい構造固定テストは作らない。一時的なコンパイラ確認や比較資料はローカル資料として残す。

## Scope of change

- root Cargo.tomlのmembers/default-members、製品Cargo.tomlの新path依存、新crate Cargo.toml、Cargo.lockの新ローカルpackageと依存辺。
- 新crate src/lib.rsへのprotocol.rs移動と版構成マクロの限定変更、製品agent/protocol.rsの再公開とCLI_VERSION定義。
- PROJECT.mdの配置・公開パスと版の正本、README.mdにcheckout構成の補足が必要な場合だけ。
- .kotowari/changes/implementation.yamlとreview.yamlの今回の比較への置換。独立レビュー担当がreview.yamlを作る。
- .agents/artifacts/protocol-extraction/のローカル検証資料。機密・生成物・比較コードはコミットしない。

## Step order and prerequisites

- 承認・計画コミット後のmainからrefactor/protocol-extractionを作る。比較元は7d2331aをgit rev-parseで完全IDへ解決し、計画以降のブランチ全体のbaseとして固定する。
- 一人のwriterが現在のcheckoutでS1〜S4を行い、S5で実装者の記録だけを作る。独立レビューとreview.yamlは呼出元が別担当へ渡す。
- S1のcheckは前回記録の形と参照だけを確認し、新baseの鮮度を示すものではない。S2開始時に旧両記録を外し、S5で今回の比較だけから再作成する。
- 全Cargo・commit hookはsystemd-run --userのMemoryMax=40%、MemorySwapMax=0、CPUWeight=idle、Nice=19の下で実行する。既存mutants.shの制限・OOM判定も維持する。commitはnice -n19、個別stage、既存フックを有効のままにする。
- HOME・XDG_CONFIG_HOME・XDG_DATA_HOME・TMPDIRはローカル資料配下へ隔離し、元のCARGO_HOME/RUSTUP_HOMEを維持する。TMPDIRはcargo-mutantsのコピー対象から外れるmutants.out/tmp配下に置く。必要な非機密PATH/toolchain変数だけを渡す。
- Gitの既存identityは隔離前に読み、コマンド限定で渡す。個人設定を変更しない。kotowari 0.3.0等は既存の実体を直接選び、miseの管理・一覧コマンドやツール導入は行わない。
- 必須はnextest、cargo-mutants、kotowari 0.3.0、systemd。ログ・比較資料・隔離先はローカル資料、ビルド生成物はroot targetに置く。tracked差分に生成物がないことを確認する。

## Verification map

| 根拠 | 観測する内容 | 手順 |
|---|---|---|
| A2、方針D2 | 通信処理と型の内容、既存公開パス、14単体テストとAgent統合 | S1、S2、S4 |
| A3、方針D1 | 内部版、単一protocol版、const型、製品表示・互換性判定 | S2、S4 |
| A4 | 新IR/要件/テストなし、期待値・印・外部依存維持 | S2、S5 |
| 検証D1〜D4 | 14テストの所在対応、限定変異、両役割の変更照合 | S1、S4、S5 |

## Left to the implementer

- 新crateの通常依存は既存指定のanyhow・serdeのderive・rmp-serde、dev-dependencyは既存指定のserde_jsonに限る。製品側の既存依存は今回削除・縮小しない。workspace依存共通化も行わない。
- 新crateルートに既存内容を置き、版構成だけを変える。product_cli_versionマクロはdefine_protocol_versionの版リテラルを使う引数なしマクロとして生成し、env!("CARGO_PKG_VERSION")は呼出元で展開する。Rustのmacro_rules/concat/stringifyだけを使い、非機密の一時的な別packageコンパイラ確認でこの展開場所を確かめる。
- 比較資料の形式、コミット分割、manifestの整形。テスト名対応は14テストのagent::protocol接頭辞が新crateのルートへ変わる部分だけを正規化し、各本文・末尾の名前と無視状態は維持する。

## Stop conditions

- 版構成以外の処理・型・テスト・期待値・印の変更が必要、外部依存更新や新しい依存・build scriptが必要なら戻す。
- env!が定義元の内部版へ展開される、従来のpub const &str APIや文字列を保てない場合は、許可範囲内のコンパイラ確認で原因を調べる。解決に方針D1の変更が必要なら利用者へ戻す。
- framing・deployment・SSH・設定やbinaryを変更しないと公開パスやAgent統合が成立しないなら戻す。
- 限定変異が抽出前から既存テストで検知されない場合は結果を示して戻す。新しいテストや同等変異を加えない。workspace選択の補正が必要な場合も今回の許可範囲外なので戻す。
- Docker/musl利用不可は未検証とし、導入・権限変更で補わない。macOSは未検証。既存FLAGやnoticeは今回解消しない。
- 最終checkとreview-phase changesが両方終了コード0になるまで統合可能とは報告しない。

## Test command

S1とS4で次を同じ順に、前述の制限・隔離内で実行し出力と終了コードを保存する。

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

- 14単体テストのpackage/library target/test binaryとprotocol接頭辞の所在変更だけを対応付ける。全3109の集合と各無視状態を比較し、他の3095の名前・所属を変えない。Cargo testのdoc-testも確認する。
- 旧protocol.rsと新src/lib.rsは版定義ブロック以外の全内容・Git modeを比較し、既存テスト本文は完全一致させる。製品の他のsrc/testsは内容・modeを維持する。
- lockfileは新ローカルpackageとpath依存辺以外に差分がないこと、manifest/metadataは製品・coreの版と公開target/featuresを維持し、3packagesがdefault-members、Docker E2Eは独立のままであることを確認する。
- 抽出前後でcargo run -- --help、cargo install --path crates/remote-merge --locked --root <別々の隔離先>を行い、生成binaryの--version・--help・agent --helpを比較する。--versionはremote-merge 0.2.13 (protocol v4)のままとする。
- 製品側のCLI_VERSIONがconst文脈で使えることと、旧公開パスからシリアライズ/ハンドシェイク/版検証を利用できることは既存コンパイルとAgentテストを確認し、必要な追加の実行はローカルのコンパイラ確認に限る。新しい永続的な互換性テストは作らない。
- cargo mutants --list --all-featuresで旧新format_handshakeの戻り値を空にする変異を確認し、scripts/mutants.sh --re 'replace format_handshake -> String with String::new\(\)' <旧または新protocol実装ファイル>で同じ1件だけを実行する。既存テストでcaughtとなり、他の変異が混ざっていないことを確認する。新crateの単体と既存両crateの単体、contract・cli_diffが選択されることも既存nextest一覧・実行ログから確認する。
- 利用可能ならcargo build --locked --release --target x86_64-unknown-linux-musl --bin remote-mergeとscripts/run-container-e2e.shを前後に実行する。coverageは既存ツールが使える場合scripts/coverage.sh textを実行し、利用不可なら未検証とする。CI・hooks・coverageのtarget選択は静的に確認するが内容は変更しない。

## Out of scope

- framing/deployment/Agent全体/SSH/設定/binaryの移動、通信形式変更、版のバンプ、互換性判定の変更、製品バグ修正。
- 新しいIR・要件・テスト・期待値や印の変更、外部依存更新・feature縮小、同等変異追加・全変異実行・既存FLAG解消。
- CI/hooksの変更、ツール導入やmise管理、個人設定変更、push・merge・release・branch/plan削除。

## Steps

### S1: 抽出前の検証資料を採取する

- Purpose: 通信と版表示の維持を既存結果と比較できるようにする。
- Specification: docs/decision/records/2026-10-03-next-crate-extraction.md#Agreements, docs/decision/records/2026-10-03-protocol-extraction-verification.md#Delegated
- Prerequisites: 計画と検証記録が承認・コミットされ、branch/base/clean状態と必須ツールを確認していること。
- May change: .agents/artifacts/protocol-extraction/のローカル資料だけ。
- Done when: protocol内容・依存・全テスト集合・版表示・インストール・限定変異の移動前結果が保存されている。
- Shown by: check 必須条件を確認しTest commandの順に実行して結果を保存する。
- Left to the implementer: 資料の形式と非機密の要約方法。
- Stop and hand back if: 必須ツールがない、移動前検証が失敗して原因を区別できない、限定変異を既存テストで検知できない。

### S2: プロトコルを移し製品版の定数を保つ

- Purpose: プロトコル型と処理を独立packageへ所属させ、既存の公開パスとconst版表示を維持する。
- Specification: docs/decision/records/2026-10-03-next-crate-extraction.md#Agreements, docs/decision/records/2026-10-03-next-crate-extraction.md#Delegated, docs/decision/records/2026-10-03-protocol-extraction-verification.md#Delegated
- Prerequisites: S1が完了していること。
- May change: root/member/new protocolのCargo.toml、Cargo.lock、旧protocol.rsと新crate src/lib.rs、旧implementation.yamlとreview.yamlの削除だけ。
- Done when: 版構成以外の移動内容と14テストが同じで、製品公開パス・const型・製品版表示が保たれ、3crateが通常検査で選択される。
- Shown by: check metadata・fmt・Clippy・nextest一覧と版表示、内容/mode比較、ローカルの別packageコンパイラ確認でマクロの展開元を確かめる。
- Left to the implementer: manifest整形と版構成マクロの許可した実装詳細。
- Stop and hand back if: 版構成以外の内容変更、型や公開範囲・通信形式・依存指定の変更が必要である。

### S3: 現在の配置と版の説明を更新する

- Purpose: 利用と開発の説明を新しい内部crate配置に一致させる。
- Specification: docs/decision/records/2026-10-03-next-crate-extraction.md#Agreements
- Prerequisites: S2の配置と再公開が成立していること。
- May change: PROJECT.mdと必要なREADME.mdのcheckout補足だけ。
- Done when: 内部版と製品版の正本、既存公開パス、通常検査の3crate対象が実際の配置を指す。
- Shown by: artifact 更新文書の差分とgit diff --check、kotowari checkで独立レビュー可能な状態を確認する。
- Left to the implementer: 同じ意味を保つ言い回し。
- Stop and hand back if: 新しい仕様・IRや配布方式を追加する必要がある。

### S4: 通信・版表示・検査の前後比較を完了する

- Purpose: 抽出が既存の通信と配布・検査を壊していないことを確認する。
- Specification: docs/decision/records/2026-10-03-next-crate-extraction.md#Agreements, docs/decision/records/2026-10-03-protocol-extraction-verification.md#Delegated
- Prerequisites: S2とS3が完了していること。
- May change: .agents/artifacts/protocol-extraction/の資料だけ。修正は先行ステップの許可範囲へ戻す。
- Done when: Test commandが可能な環境で成功し、14テストの所在以外の集合差分がなく、表示・依存・通信処理と限定変異の結果が一致する。
- Shown by: check Test commandの順に抽出後の検証を行いS1資料と比較し、未実行の環境を別記する。
- Left to the implementer: 許可したテスト所在と実行パスの正規化方法。
- Stop and hand back if: 期待値変更・テスト追加や検査縮小が必要、既存mutantsのworkspace選択が成立しない。

### S5: 独立レビューと変更照合を完了する

- Purpose: ブランチ全体が合意に従うことを両役割で照合する。
- Specification: docs/decision/records/2026-10-03-next-crate-extraction.md#Agreements, docs/decision/records/2026-10-03-protocol-extraction-verification.md#Delegated
- Prerequisites: S4の証拠と実装コミット、固定した完全base/candidate headがあること。
- May change: 実装者はimplementation.yamlだけを作って返し、呼出元が別のレビュー担当へreview.yaml作成を依頼する。
- Done when: 独立レビューに修正すべき指摘がなく、両役割が今回の全変更をカバーしてcheckとreview-phase changesが終了コード0である。
- Shown by: check 呼出元が独立レビューと記録を依頼しkotowari check --format jsonとkotowari changes --base <完全base ID> --head <記録コミット後の完全head ID> --phase review --format jsonを実行する。
- Left to the implementer: 自分の記録のグループとID。独立担当の判断は実装者には委任しない。
- Stop and hand back if: 独立レビューが完了できない、記録の結論が矛盾する、修正が承認範囲を越える。
