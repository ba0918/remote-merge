# Project Context

## What this is

SSH 経由でローカルと複数のリモートサーバのファイルを比較・マージする Rust ツール。対話型 TUI と、JSON 出力に対応した非対話 CLI がある。正規化した仕様は `docs/ir/` に置く。旧仕様は `docs/archive/` に移行元として保存しており、内容を現行の動作と同一視しない。

## Stack and layout

- Rust。主な依存は tokio、russh、ratatui、similar、serde。
- ルートは Cargo ワークスペース。members と default-members は `crates/remote-merge/`、`crates/remote-merge-core/`、`crates/remote-merge-protocol/`、`crates/remote-merge-settings/`、`crates/remote-merge-ssh/`、`crates/remote-merge-config/`、`crates/remote-merge-agent/`、`crates/remote-merge-engine/`。`Cargo.lock`、`.cargo/`、release profile と `target/` はルートに置く。
- `crates/remote-merge-core/src/`: tree・diff・error・filter とその単体テスト。製品は同じ型を `remote_merge::{tree, diff, error, filter}` から再公開する。
- `crates/remote-merge-protocol/src/lib.rs`: Agentの通信型・シリアライズ・ハンドシェイクとその単体テスト。製品は `remote_merge::agent::protocol` から同じ型・定数・関数を再公開する。
- `crates/remote-merge-protocol/src/framing.rs`: 長さプレフィクス付きフレームの読み書きとその単体テスト。製品は既存の `remote_merge::agent::framing` から再公開する。
- `crates/remote-merge-settings/src/lib.rs`: 標準ライブラリだけに依存する接続設定の5型と既存のDebug・Default実装。configと製品は同じ型を再公開する。
- `crates/remote-merge-config/src/lib.rs`: 設定の読み込み・認証の解決・パースと既存89単体テスト。製品の `src/config.rs` は既存公開パスを明示的に再公開する。expand_tildeは下位のconfigに所属し、SSHから参照する。バックアップ除外に使うBACKUP_DIR_NAMEの正本もここに置き、製品の `backup` は同じ定数を再公開する。
- `crates/remote-merge-ssh/src/`: 全SSH実装と既存132単体テスト。製品は `remote_merge::ssh::{client, batch_read, hint, host_key_verifier, passphrase_provider}` を公開のまま再公開し、`ssh::tree_parser` は製品crate内だけの再公開を保つ。known_hosts・known_hosts_io・preferredはSSH crate内だけに置く。設定型はsettings、ホーム展開と既定の上限はconfig、error・filter・treeはcoreの同じ項目を使う。strict実行のinherent methodはcrate内に保ち、製品の三つの呼び出しは内部再公開した委譲関数を使う。製品のtest-utils featureはSSHの同名featureへ転送する。
- `crates/remote-merge-agent/src/agent/`: Agentの実行処理・SSH転送と配置処理、既存191単体テスト。製品は既存 `remote_merge::agent` の同じ公開パスから再公開する。通信型とframingはprotocol、filter/treeはcore、shell escapeは通常依存のSSHを使う。配置の四つの版依存関数は製品側の旧signatureのwrapperから製品CLI_VERSIONを受け取る。版に関わる既存88テストとtest-only SHA helperは製品側に残す。両crateのbuild.rsはCargoのTARGETをそのままcompile時定数へ渡す。
- `crates/remote-merge-engine/src/`: local・backup・backup_store・merge/executor・merge/optimistic_lock・merge/execution・merge/mtime、共有Side、service/types・max_files・path_resolver・fast_path・diff・status・merge・merge_flow・sync・rollbackと既存388単体テスト。Sideの正本は `side` に置き、製品の `app::Side` と `app::side` は同じ型・関数を再公開する。serviceの九モジュールも製品の同じパスから同じ項目を再公開する。three_wayは三者の存在・内容比較だけを持ち、製品appのbadge型は下位の比較結果を同じbadgeへ変換する。executorは既存公開項目を明示的に再公開する。validate_path_within_rootの利用はengine内にあり、製品側に未使用のprivate再公開を置かない。backup_storeは製品runtimeからcrate内だけに再公開する。coreとconfigの同じ型・定数を参照し、backup定数の正本はconfigのまま変えない。serdeの既存JSON表現は維持し、serde_jsonは保存レコードの読み書きにも使う通常依存に置く。
- `crates/remote-merge-engine/src/local_io.rs`: ローカルの読み書き・バッチ・ハッシュ・mtime・権限・削除・symlink・ツリー走査とパス検査。protocolの同じFileHashResultとcoreの同じFileNode/FileTreeを使う。製品runtimeはhelpersとTargetPathをcrate内だけに再公開し、LocalTargetIoの検査と十五操作を具体的な自由関数に委譲する。製品側の利用が混在テストだけになった五helperはtest-only再公開に保つ。製品側にTargetIoのdispatch、LocalTargetIoの構築と接続no-op、RemoteTargetIo、CoreRuntimeとSideIOの混在する既存116テストを保つ。
- `crates/remote-merge-engine/src/service/merge_flow.rs`: ソース存在検査、hunk対象検査とバイト列からのhunk準備。製品service/merge_flowは二validatorを同じ公開パスから再公開し、両側read・dry-run・backup・writeと既存十八テストを保つ。engineのmerge/execution・merge/mtimeは旧handlerの純粋判定と既存三十テストを持ち、製品handlerの二moduleは八公開項目を同じ型のまま明示的に再公開する。
- `crates/remote-merge-engine/src/service/rollback.rs`: 復元パス判定に加え、検査失敗または既存の二つの拒否理由を要求全体の結果へ展開する純粋なrestore_request_refusalを持つ。backup_storeは保存レコードを同じBackupPathRecordへ変換するFrom実装を持つ。製品SideIOはprivate変換wrapperと既存116混在テストを保ち、第一passの逐次read・検査、session予約、第二passのread・dry-run・pre-backup・write、最後のfinishを担当する。
- `crates/remote-merge/src/app/`: TUI の状態とロジック。`crates/remote-merge/src/handler/` と `crates/remote-merge/src/ui/`: 入力処理と描画。
- `crates/remote-merge/src/cli/` と `crates/remote-merge/src/service/`: CLI と共通処理。`crates/remote-merge/src/runtime/`: TUI の実行時処理。`crates/remote-merge/src/ssh/` と `crates/remote-merge/src/agent/`: リモート接続と転送。
- CLIの出力とruntimeのI/Oは製品側に残す。app/three_wayのbadge型・label/styleと既存20テストも製品側に保ち、paletteや表示用Conflict variantは下位へ移さない。
- 通常テストは `crates/remote-merge/tests/`。Docker E2E は `tests/container-e2e/` の独立ワークスペースに残す。proptest の失敗入力はルートの `proptest-regressions/` に保存する。
- 製品バージョンの正本は `crates/remote-merge/Cargo.toml` の `package.version`。
- core の内部バージョンは `crates/remote-merge-core/Cargo.toml` の `package.version`。core は `publish = false` とし、製品のバージョンとは独立に扱う。
- protocol の内部バージョンは `crates/remote-merge-protocol/Cargo.toml` の `package.version`。protocol は `publish = false`。通信版の正本は `define_protocol_version` の単一リテラルで、製品側の `CLI_VERSION` は製品版と通信版を組み合わせた定数とする。
- settings の内部バージョンは `crates/remote-merge-settings/Cargo.toml` の `package.version`。settings は版0.1.0・`publish = false` とし、製品版とは独立に扱う。
- ssh の内部バージョンは `crates/remote-merge-ssh/Cargo.toml` の `package.version`。ssh は版0.1.0・`publish = false` とし、製品版とは独立に扱う。
- config の内部バージョンは `crates/remote-merge-config/Cargo.toml` の `package.version`。configは版0.1.0・`publish = false` とし、製品版とは独立に扱う。
- agent の内部バージョンは `crates/remote-merge-agent/Cargo.toml` の `package.version`。agentは版0.1.0・`publish = false` とし、製品版とは独立に扱う。
- engine の内部バージョンは `crates/remote-merge-engine/Cargo.toml` の `package.version`。engineは版0.1.0・`publish = false` とし、製品版とは独立に扱う。
- 設定はグローバルの `~/.config/remote-merge/config.toml` と、プロジェクトの `.remote-merge.toml`。後者が優先し、`[filter]` は和集合でマージする。

## Commands

| Purpose | Command |
|---|---|
| Build | `cargo build` |
| Test | `cargo nextest run`（未導入なら `cargo test`） |
| Check | `cargo fmt --all --check` / `cargo clippy --all-targets --all-features -- -D warnings` |
| Run | `cargo run -- --right <server>`（引数なしは TUI、サブコマンドありは CLI） |
| Spec check | `kotowari check` |
| Change conformance | `kotowari changes --base <full-base-id> --head <full-head-id> --phase review --format json` |
| Real OpenSSH/sudo tests | `scripts/run-container-e2e.sh`（Docker 必須、通常テストと別パッケージ） |

## Conventions specific to this project

- ユーザー向けの文言と CLI ヘルプは英語。コード内のコメントは日本語でもよい。
- コミットメッセージは Conventional Commits 形式で、件名・本文は日本語。
- TUI は WebView 方式への移行を想定して凍結中。ただし `docs/ir/scan/directory-links.md` のディレクトリ symlink 展開については対応を認める。それ以外の変更は依頼された機能に関わる最小限にとどめる。
- `lefthook.yml` は fmt・仕様検査を pre-commit、Clippy・通常テストを pre-push で読み取り専用で実行する。既存の個人フックを置き換えないため `lefthook install` は自動実行しない。既存フックの所有者が内容を確認し、必要なら手動で統合する。未ステージの変更も手動検査する場合は `lefthook run pre-commit --force --no-auto-install` / `lefthook run pre-push --force --no-auto-install` を使う。
- 旧 OpenSSH の手動負荷試行は `bench/legacy-ssh/setup.sh` から開始する。試行中のシェルを終了すると専用コンテナ・イメージ・鍵・known_hosts・データが破棄され、個人の SSH 設定は変更されない。CI の保証範囲には含めない。

## Change conformance

- kotowari 0.3.0 の `changes` を使用する。検査対象と記録先は `.kotowari/config.yaml` の `changes` に定義する。コード、全テスト、配布スキル、スクリプト、ベンチ環境、ビルド設定、フック、CI を対象にし、生成物の除外は現時点では設けない。
- 比較元と対象の完全なコミット ID は、記録の自己申告ではなくブランチの履歴から確定する。最終検査はブランチ全体の比較とし、HEAD の親だけを比較しない。
- `changes` は対象コミットまたはステージ領域の設定と記録を読む。作業ツリーだけにある変更は検査に使われないため、設定の有効化後も未コミットの設定を `--head` で検査することはできない。
- 実装者は `.kotowari/changes/implementation.yaml`、実装と別のレビュー担当は `.kotowari/changes/review.yaml` を作成する。書式とハッシュの算出方法は kotowari スキルの changes 参照に従う。自己レビューを reviewer の記録として扱わない。
- このディレクトリには現在の比較の記録だけを置く。コード・関連 IR・決定の意味や履歴が変わり最終記録が無効になったら、両方の記録を除いて比較全体を照合し直し、独立レビュー後に再作成する。過去の記録は Git 履歴で参照する。
- 統合前に `kotowari check --format json` と上記の Change conformance コマンドの両方で終了コード 0 を確認する。`check` の成功だけでは変更の照合は完了していない。
- 中間コミットには記録を要求しない。任意の自己検査は `kotowari changes --base HEAD --staged --phase implementation --format json`。フックでは `changes` を実行しない。CI の変更照合ゲートはまだ追加していない。
- `.ignore` は通常の検索から機械用記録を外すための設定で、Git 管理からは外さない。照合時はファイルを直接読むか `rg --no-ignore` を使う。

## Constraints

- 読み取り中の SSH 接続エラーは一度だけ再試行する。書き込み前には対象の内容も再確認する（現行の更新時刻だけの検査は IR に対する実装課題）。
- シンボリックリンクは参照先のパスで比較し、バイナリファイルは SHA-256 で比較する。
- リモート間 merge には確認が必要。CLI の比較先指定は `--left` / `--right`。
