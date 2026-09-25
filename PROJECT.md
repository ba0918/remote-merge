# Project Context

## What this is

SSH 経由で、ローカルとリモートサーバの間のファイル差分を表示・マージする Rust 製のツール。
local・develop・staging・release のような複数サーバを横断して比較できる。
操作手段は、対話型の TUI と、LLM エージェント連携向けに JSON を出す非対話の CLI の 2 つ。

全体仕様は `spec.md`、機能ごとの仕様は `docs/spec/`、進捗は `docs/status.md`。

## Stack and layout

Rust, ratatui, tokio, russh, similar, toml + serde, anyhow, tracing

レイヤーと対応するディレクトリ（依存は上から下への一方向）:

- ドメイン（`src/app/`）: 純粋ロジック。副作用なし
- サービス（`src/service/`, `src/runtime/`）: ドメインの組み合わせと I/O
- ハンドラ（`src/handler/`）: イベントをサービス呼び出しに変換する薄い層
- UI（`src/ui/`）: 描画のみ

接続モデル:

- ツリー取得・ファイル内容の取得・書き込みは SSH exec で行う（書き込みは `cat >` かエージェントプロトコル）
- 遅延読み込み: ディレクトリの中身は展開したとき、ファイルの中身は必要になったときに取得する

2 つの操作手段:

1. TUI（既定）: ファイルツリー付きの 2 ペイン差分ビューア。hunk 単位でマージできる
2. CLI（サブコマンド `status`, `diff`, `merge`, `sync`, `rollback`, `logs`, `events`, `init`, `agent`）: LLM エージェント連携向けに JSON を出す

設定の階層:

- グローバル: `~/.config/remote-merge/config.toml`
- プロジェクト: `.remote-merge.toml`（グローバルを上書きする。`[filter]` だけは和集合でマージ）

## Commands

| Purpose | Command |
|---|---|
| Install | |
| Build | `cargo build` |
| Test | `cargo nextest run`（推奨）。1 件だけなら `cargo test <name>` |
| Lint | `cargo fmt --all --check` と `cargo clippy --all-targets --all-features -- -D warnings`（CI と同じ） |
| Run locally | `cargo run`。サーバ指定は `cargo run -- --right develop` |

### レガシー環境テスト（testenv/）

CentOS 5.11 の Docker コンテナで、レガシー環境と負荷のテストをする。
前提: WSL2 の `.wslconfig` に `kernelCommandLine = vsyscall=emulate`、Docker。

```
cd testenv && ./setup.sh     # フルセットアップ（10 万ファイル、数分かかる）
docker compose down           # 停止
cargo run -- --config testenv/config.toml diff --right centos5 app/controllers/file_0.php
cargo run -- --config testenv/config.toml status --right centos5
```

環境: CentOS 5.11, bash 3.2, OpenSSH 4.3（ed25519 非対応のため RSA）, ARG_MAX 131072,
リモート 10 万ファイル, ローカル 500 ファイル, RightOnly 約 9.96 万。レガシー kex は config.toml で設定済み。

## Conventions specific to this project

- ユーザーに見える文言（ダイアログ、ステータス、エラー、CLI ヘルプ）は英語。コードコメントと doc コメントは日本語でよい。
- コミットメッセージは Conventional Commits。type は `feat|fix|refactor|docs|test|style|perf|chore`。件名も本文も日本語。
- `git commit --no-verify` は使わない。
- Claude Code から `git commit` すると、`.claude/hooks/pre-commit-format.sh` が `cargo fmt --all` と `cargo clippy --fix` を実行し、変更のある追跡済みファイルを `git add -u` でステージする。コミット前に作業ツリーを意図したファイルだけにしておく。
- TUI は凍結している。WebView 方式へ移行するときに削除する予定なので、改善や安全性の修正はしない。他の仕様のために必要な最小限だけ触る。

## Constraints

設計上の決定:

- SSH 切断時の自動再接続（読み取り中の接続エラーで 1 回だけ再試行）
- マージ時の楽観ロック（書き込み前にリモートの mtime を再確認）
- symlink はリンク先のパスで比較し、参照先の内容は比較しない
- バイナリファイルは SHA-256 ハッシュで比較
- 機密ファイル（`.env`, `*.pem`）は merge / diff の前に警告
- リモート同士のマージはサーバ名の確認が必要（`--force` で省略）
- CLI は一貫して `--left` / `--right` を使う（`--from` / `--to` は使わない）

## Glossary

`CONTEXT.md` を参照。
