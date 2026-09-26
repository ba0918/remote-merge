# Project Context

## What this is

SSH 経由でローカルと複数のリモートサーバのファイルを比較・マージする Rust ツール。対話型 TUI と、JSON 出力に対応した非対話 CLI がある。正規化した仕様は `docs/ir/` に置く。旧仕様は `docs/archive/` に移行元として保存しており、内容を現行の動作と同一視しない。

## Stack and layout

- Rust。主な依存は tokio、russh、ratatui、similar、serde。
- `src/app/`: TUI の状態とロジック。`src/handler/` と `src/ui/`: 入力処理と描画。
- `src/cli/` と `src/service/`: CLI と共通処理。`src/runtime/`: TUI の実行時処理。`src/ssh/` と `src/agent/`: リモート接続と転送。
- 設定はグローバルの `~/.config/remote-merge/config.toml` と、プロジェクトの `.remote-merge.toml`。後者が優先し、`[filter]` は和集合でマージする。

## Commands

| Purpose | Command |
|---|---|
| Build | `cargo build` |
| Test | `cargo nextest run`（未導入なら `cargo test`） |
| Check | `cargo fmt --all --check` / `cargo clippy --all-targets --all-features -- -D warnings` |
| Run | `cargo run -- --right <server>`（引数なしは TUI、サブコマンドありは CLI） |
| Spec check | `kotowari check` |
| Real OpenSSH/sudo tests | `scripts/run-container-e2e.sh`（Docker 必須、通常テストと別パッケージ） |

## Conventions specific to this project

- ユーザー向けの文言と CLI ヘルプは英語。コード内のコメントは日本語でもよい。
- コミットメッセージは Conventional Commits 形式で、件名・本文は日本語。
- TUI は WebView 方式への移行を想定して凍結中。ただし `docs/ir/scan/directory-links.md` のディレクトリ symlink 展開については対応を認める。それ以外の変更は依頼された機能に関わる最小限にとどめる。
- `lefthook.yml` は fmt・仕様検査を pre-commit、Clippy・通常テストを pre-push で読み取り専用で実行する。既存の個人フックを置き換えないため `lefthook install` は自動実行しない。既存フックの所有者が内容を確認し、必要なら手動で統合する。未ステージの変更も手動検査する場合は `lefthook run pre-commit --force --no-auto-install` / `lefthook run pre-push --force --no-auto-install` を使う。
- 旧 OpenSSH の手動負荷試行は `bench/legacy-ssh/setup.sh` から開始する。試行中のシェルを終了すると専用コンテナ・イメージ・鍵・known_hosts・データが破棄され、個人の SSH 設定は変更されない。CI の保証範囲には含めない。

## Constraints

- 読み取り中の SSH 接続エラーは一度だけ再試行する。書き込み前には対象の内容も再確認する（現行の更新時刻だけの検査は IR に対する実装課題）。
- シンボリックリンクは参照先のパスで比較し、バイナリファイルは SHA-256 で比較する。
- 機密ファイルの diff・merge と、リモート間 merge には確認が必要。CLI の比較先指定は `--left` / `--right`。
