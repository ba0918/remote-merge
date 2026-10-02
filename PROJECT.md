# Project Context

## What this is

SSH 経由でローカルと複数のリモートサーバのファイルを比較・マージする Rust ツール。対話型 TUI と、JSON 出力に対応した非対話 CLI がある。正規化した仕様は `docs/ir/` に置く。旧仕様は `docs/archive/` に移行元として保存しており、内容を現行の動作と同一視しない。

## Stack and layout

- Rust。主な依存は tokio、russh、ratatui、similar、serde。
- ルートは Cargo ワークスペース。members と default-members は `crates/remote-merge/`、`crates/remote-merge-core/`、`crates/remote-merge-protocol/`。`Cargo.lock`、`.cargo/`、release profile と `target/` はルートに置く。
- `crates/remote-merge-core/src/`: tree・diff・error・filter とその単体テスト。製品は同じ型を `remote_merge::{tree, diff, error, filter}` から再公開する。
- `crates/remote-merge-protocol/src/lib.rs`: Agentの通信型・シリアライズ・ハンドシェイクとその単体テスト。製品は `remote_merge::agent::protocol` から同じ型・定数・関数を再公開する。
- `crates/remote-merge-protocol/src/framing.rs`: 長さプレフィクス付きフレームの読み書きとその単体テスト。製品は既存の `remote_merge::agent::framing` から再公開する。
- `crates/remote-merge/src/app/`: TUI の状態とロジック。`crates/remote-merge/src/handler/` と `crates/remote-merge/src/ui/`: 入力処理と描画。
- `crates/remote-merge/src/cli/` と `crates/remote-merge/src/service/`: CLI と共通処理。`crates/remote-merge/src/runtime/`: TUI の実行時処理。`crates/remote-merge/src/ssh/` と `crates/remote-merge/src/agent/`: リモート接続と転送。
- 通常テストは `crates/remote-merge/tests/`。Docker E2E は `tests/container-e2e/` の独立ワークスペースに残す。proptest の失敗入力はルートの `proptest-regressions/` に保存する。
- 製品バージョンの正本は `crates/remote-merge/Cargo.toml` の `package.version`。
- core の内部バージョンは `crates/remote-merge-core/Cargo.toml` の `package.version`。core は `publish = false` とし、製品のバージョンとは独立に扱う。
- protocol の内部バージョンは `crates/remote-merge-protocol/Cargo.toml` の `package.version`。protocol は `publish = false`。通信版の正本は `define_protocol_version` の単一リテラルで、製品側の `CLI_VERSION` は製品版と通信版を組み合わせた定数とする。
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
- 機密ファイルの diff・merge と、リモート間 merge には確認が必要。CLI の比較先指定は `--left` / `--right`。
