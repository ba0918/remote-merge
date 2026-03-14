# remote-merge

SSH 経由でローカルとリモートサーバー間のファイル差分を表示・マージする TUI/CLI ツール。

## Features

- 複数サーバー（develop, staging, release など）とのファイルツリー比較
- TUI モード: 2ペインの差分ビューア + hunk 単位マージ
- CLI モード: `status` / `diff` / `merge` / `sync` / `rollback` サブコマンド
- 3-way 比較（`--ref` で参照サーバーを指定）
- マージ時の楽観的ロック（mtime チェック）
- Agent モードによる高速リモート操作

## Install

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/ba0918/remote-merge/main/scripts/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/ba0918/remote-merge/main/scripts/install.ps1 | iex
```

バージョン指定やインストール先の変更も可能:

```bash
VERSION=v0.1.0 INSTALL_DIR=~/.local/bin sh -c "$(curl -fsSL ...)"
```

**ソースからビルド:**

```bash
cargo install --path .
```

## Quick Start

```bash
# プロジェクト設定ファイルを生成
remote-merge init

# .remote-merge.toml を編集してサーバー情報を設定

# TUI を起動
remote-merge

# サーバーを指定して起動
remote-merge --left local --right develop
```

## Usage

```
Usage: remote-merge [OPTIONS] [COMMAND]

Commands:
  init      Initialize project config file
  status    List files with differences
  diff      Show diff for file(s) or directory
  merge     Merge files
  sync      Sync files to multiple servers (1:N synchronization)
  rollback  Restore files from a backup session
  logs      Show debug logs
  events    Show TUI events
  help      Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>  Path to project config file
      --left <LEFT>      Left side of comparison [default: local]
      --right <RIGHT>    Right side of comparison
      --ref <REF>        Reference server for 3-way comparison
  -y, --yes              Auto-accept prompts
      --debug              Shorthand for --log-level debug
      --log-level <LEVEL>  Set log level (error, warn, info, debug, trace)
  -v, --verbose...       Increase log verbosity (-v: info, -vv: debug, -vvv: trace)
  -h, --help             Print help
  -V, --version          Print version
```

## Configuration

設定ファイルは TOML 形式。グローバル設定とプロジェクト設定の 2 階層で管理される。

| ファイル | パス |
|--------|------|
| グローバル | `~/.config/remote-merge/config.toml` |
| プロジェクト | `.remote-merge.toml`（カレントディレクトリ） |

プロジェクト設定はグローバル設定を上書きする（`[filter]` セクションのみ和集合でマージ）。

```toml
[local]
root_dir = "/path/to/local/project"

[servers.develop]
host = "dev.example.com"
port = 22
user = "deploy"
auth = "key"                    # "key" or "password"
key = "~/.ssh/id_ed25519"
root_dir = "/var/www/project"

[filter]
exclude = [".git", "node_modules", "vendor"]
sensitive = [".env", "*.pem"]

[ssh]
timeout_sec = 10
strict_host_key_checking = "ask"  # "yes", "no", "ask"

[backup]
enabled = true
retention_days = 30
```

## License

MIT
