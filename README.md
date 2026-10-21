# remote-merge

SSH 経由でローカルとリモートサーバー間のファイル差分を表示・マージする CLI ツール。

## Features

- 複数サーバー（develop, staging, release など）とのファイルツリー比較
- `status` / `diff` / `merge` / `sync` / `rollback` サブコマンド（`--format json` で機械可読な出力）
- hunk 単位マージ（`merge --hunks`）
- 3-way 比較（`--ref` で参照サーバーを指定）
- マージ時の楽観的ロック（書き込み先が比べた後に変わっていないかを、種類・リンク文字列・中身のハッシュで確かめる）
- Agent モードによる高速リモート操作

## Install

### mise（推奨）

Linux x86_64 / macOS（Intel・Apple Silicon）向けの配布バイナリを、[mise](https://mise.jdx.dev/getting-started.html) でインストールできます。
mise の導入とシェルの有効化を済ませてから実行してください。

```bash
mise use --global github:ba0918/remote-merge@0.3.3
remote-merge --version
```

別の版を使う場合は、`@0.3.3` をその版に置き換えてください。
`@latest` は mise 側のバージョン情報の反映が遅れると旧版を選ぶことがあるため、版を明示しています。
プロジェクトごとに版を管理する場合は、そのディレクトリで `--global` を外して実行すると `mise.toml` に記録されます。

### インストールスクリプト（mise を使わない場合）

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/ba0918/remote-merge/main/scripts/install.sh | sh
```

Windows はまだ配布していません（バイナリの配布は Linux と macOS だけです。`scripts/install.ps1` は Windows 対応のときに使います）。

バージョン指定やインストール先の変更も可能:

```bash
curl -fsSL https://raw.githubusercontent.com/ba0918/remote-merge/main/scripts/install.sh | VERSION=v0.3.3 INSTALL_DIR="$HOME/.local/bin" sh
```

### ソースからビルド

```bash
cargo install --path crates/remote-merge
```

ソースからのインストールには、同じ checkout の全体（`Cargo.toml`・`Cargo.lock` と 8 つのクレート）が必要です（`crates/remote-merge` は同じ workspace のクレートを path で参照します）。

## Quick Start

```bash
# プロジェクト設定ファイルを生成
remote-merge init

# .remote-merge.toml を編集してサーバー情報を設定

# 差分のあるファイルを一覧する
remote-merge status --left local --right develop

# 差分を表示する
remote-merge diff path/to/file --left local --right develop

# マージする
remote-merge merge path/to/file --left local --right develop
```

サブコマンドを付けずに起動すると、使い方を標準エラーに出して終了コード 2 で終わる。

## Usage

```
Usage: remote-merge [OPTIONS] <COMMAND>

Commands:
  init      Initialize project config file
  status    List files with differences
  diff      Show diff for file(s) or directory
  merge     Merge files
  sync      Sync files to multiple servers (1:N synchronization)
  rollback  Restore files from a backup session
  logs      Show debug logs
  agent     Start agent server (used internally via SSH)
  help      Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>        Path to project config file [overrides .remote-merge.toml in CWD]
  -y, --yes                    Auto-accept prompts (e.g., unknown host key verification)
  -v, --verbose...             Increase log verbosity (-v: info, -vv: debug, -vvv: trace)
      --debug                  Shorthand for --log-level debug
      --log-level <LOG_LEVEL>  Set log level explicitly (error, warn, info, debug, trace)
  -h, --help                   Print help
  -V, --version                Print version
```

## Configuration

設定ファイルは TOML 形式。グローバル設定とプロジェクト設定の 2 階層で管理される。

| ファイル | パス |
|--------|------|
| グローバル | OS の設定ディレクトリの下の `remote-merge/config.toml`（Linux は `$XDG_CONFIG_HOME` か `~/.config`、macOS は `~/Library/Application Support`） |
| プロジェクト | `.remote-merge.toml`（カレントディレクトリ） |

プロジェクト設定はグローバル設定を上書きする（`[filter]` は和集合、`[content]` は下記の規則で合成する）。

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

[ssh]
timeout_sec = 300
strict_host_key_checking = "ask"  # "yes", "no", "ask"

[backup]
enabled = true
retention_days = 7
```

### ファイル内容の文字コード

diff と部分マージは、接続先と相対パスごとに文字コードを指定できる。
SSH の接続設定には加えず、内容の設定として書く。

```toml
[content.defaults]
develop = "euc-jp"

[[content.rules]]
side = "develop"
path = "old/**"
encoding = "cp932"

[[content.rules]]
side = "develop"
path = "new/**"
encoding = "utf-8"
```

指定できる名前は、大文字小文字を区別せず `utf-8`、`euc-jp`、`shift_jis`、`cp932`、`windows-31j`、`binary`。
Shift_JIS 系の三つの名前は encoding_rs の CP932（Windows-31J 相当）を使い、厳密な JIS 版ではない。
`side` は `local` または設定にあるサーバ名で、`path` は exclude と同じ照合方法を使う。

既定値はプロジェクトにあるキーだけを優先する。
ルールはグローバル、プロジェクトの順につなぎ、最後の一致を使う。
そのため、継承したパスルールもプロジェクトの既定値より優先する。
diff と merge の `--left-encoding`、`--right-encoding`、`--ref-encoding` は、その側だけを全ルールより優先して上書きする。

```bash
remote-merge diff old/a.php --left local --right develop --right-encoding cp932 --format json
remote-merge merge old/a.php --left local --right develop --left-encoding cp932 --right-encoding cp932 --hunks 1 --dry-run
```

未指定時は従来どおり、非 UTF-8 と先頭 8,192 バイトに NUL がある内容をバイナリとして扱う。
明示した文字コードでデコードできない場合は、置換せずエラーにする。
部分マージは既存の左右が同じ文字コードのテキストのときだけ使え、選んだ変更には読み込み元のバイトを使い、それ以外の文字と改行と BOM は書き込み先のまま残す。
ない側には文字コードを持たせず、新規作成先には読み込み元のバイトと BOM を引き継ぐ。
全体の merge と sync は文字コードを変換せず元バイトをそのまま転送し、バックアップと rollback も元バイトを保存、復元する。

JSON の diff は `encodings` に実際に使った側ごとの文字コードを示す。
本文が同じで元バイトだけが違う場合は `byte_only_difference: true`、空の `hunks`、元バイトのハッシュと説明を出す。
全体コピーの dry-run の `merged` 項目には、変換しないことを示す `copy_note` が入る（部分マージには入らない）。
出力の詳細は [JSON の手引き](skills/remote-merge/references/json-schemas.md#diff) を参照。

## License

MIT
