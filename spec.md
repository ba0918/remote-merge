# remote-merge 仕様書

> ローカルとリモートサーバ間のファイル差分をTUIでグラフィカルに表示・マージするツール

---

## 概要

| 項目 | 内容 |
|------|------|
| ツール名 | `remote-merge` |
| 実装言語 | Rust |
| 配布形式 | シングルバイナリ |
| 対応OS | Linux / macOS / Windows |
| 接続プロトコル | SSH exec + SFTP |

### 解決する課題

- 開発・ステージング・本番サーバ間でコードが乖離してしまい「どれが正しいか」わからなくなる
- サーバ上で直接編集されたファイルがgitに残らず検知できない
- 複数サーバを横断して比較するのにSCP・diff・手動コピペが必要で手間がかかる

---

## アーキテクチャ

### 接続方式

```
local                        remote server
  │                               │
  │──── SSH コネクション ──────────│
  │                               │
  │  [ディレクトリ展開時]          │
  │  ← find / ls -la ─────────────│  軽量・ツリー取得のみ
  │                               │
  │  [差分表示時]                  │
  │  ← cat / base64 ──────────────│  オンデマンド取得
  │                               │
  │  [マージ時]                    │
  │  ──── SFTP ────────────────────│  ファイル転送
```

**SSH exec** でツリー取得・diff取得、**SFTP** でファイル転送（マージ時のみ）の使い分け。

### 遅延読み込み戦略

全ファイルを一括取得すると負荷が青天井になる為、遅延読み込みを採用する。

```
起動時:  ルートのディレクトリ一覧のみ取得
展開時:  そのディレクトリの直下のみ取得・比較
差分表示: 選択したファイルの内容を初回のみ取得・キャッシュ
```

### パフォーマンス対策

#### 大量ファイルのハンドリング

リモートの `find` / `ls` コマンドに対して以下の制限を設ける。

| 制限 | デフォルト値 | 説明 |
|------|------------|------|
| ディレクトリ取得タイムアウト | 30秒 | `find` コマンドの実行時間上限 |
| 最大エントリ数 | 10,000件/ディレクトリ | 1ディレクトリあたりの表示上限 |

上限を超えた場合はステータスバーに `[10,000+ items - filtered]` と表示し、
フィルター設定の見直しを促す。

#### キャッシュ戦略

| キャッシュ対象 | 無効化タイミング |
|--------------|----------------|
| ディレクトリ一覧 | `r` キーで手動リフレッシュ / サーバ切替時 |
| ファイル内容（diff用） | `r` キーで手動リフレッシュ / マージ実行後の対象ファイル |
| ファイルメタデータ | ディレクトリ一覧と同時に更新 |

マージ実行後は対象ファイルのキャッシュを自動的に破棄し、最新状態を再取得する。

---

## 設定ファイル

`~/.config/remote-merge/config.toml`

```toml
[servers.develop]
host     = "dev.example.com"
port     = 22
user     = "deploy"
key      = "~/.ssh/id_rsa"
root_dir = "/var/www/app"

[servers.staging]
host     = "staging.example.com"
port     = 22
user     = "deploy"
key      = "~/.ssh/id_rsa"
root_dir = "/var/www/app"

[servers.release]
host     = "release.example.com"
port     = 22
user     = "deploy"
key      = "~/.ssh/id_rsa"
root_dir = "/var/www/app"
sudo     = true               # Agent を sudo で起動（NOPASSWD 必須）
file_permissions = "0o644"    # 新規ファイルのパーミッション（サーバー単位オーバーライド）
dir_permissions  = "0o755"    # 新規ディレクトリのパーミッション

# レガシーサーバ向け設定例（古いSSHバージョン対応）
[servers.legacy]
host     = "legacy.example.com"
port     = 22
user     = "deploy"
auth     = "password"          # "key"（デフォルト）または "password"
# パスワードは設定ファイルに記載しない。接続時にプロンプトで入力するか、
# 環境変数 REMOTE_MERGE_PASSWORD_<SERVER名> で渡す。
root_dir = "/var/www/app"

[servers.legacy.ssh_options]
# 鍵交換アルゴリズム
kex_algorithms         = ["diffie-hellman-group14-sha1", "diffie-hellman-group1-sha1"]
# ホスト鍵アルゴリズム
host_key_algorithms    = ["ssh-rsa", "ssh-dss"]
# 暗号化アルゴリズム
ciphers                = ["aes128-cbc", "3des-cbc", "aes192-cbc", "aes256-cbc"]
# MACアルゴリズム
mac_algorithms         = ["hmac-sha1", "hmac-md5"]

[defaults]
file_permissions = "0o664"    # 新規ファイルのデフォルトパーミッション
dir_permissions  = "0o775"    # 新規ディレクトリのデフォルトパーミッション

[local]
root_dir = "/home/user/projects/app"

[filter]
# ツリー表示・比較から除外するパターン
exclude = ["node_modules", ".git", "dist", "*.log", "*.lock"]
# スキャン対象をホワイトリスト方式で限定（指定時は include に一致するパスのみスキャン）
# include = ["src", "config"]

[ssh]
timeout_sec = 10   # 接続タイムアウト（デフォルト: 10秒）

[backup]
enabled        = true    # バックアップの有効/無効（デフォルト: true）
retention_days = 7       # 自動削除までの日数（デフォルト: 7）

[agent]
enabled              = true       # Agent プロトコルの有効/無効（デフォルト: true）
deploy_dir           = "/var/tmp" # Agent バイナリの配置先（デフォルト: "/var/tmp"）
timeout_secs         = 30         # Agent 応答タイムアウト（デフォルト: 30秒）
tree_chunk_size      = 1000       # ツリー走査のチャンクサイズ（デフォルト: 1000）
max_file_chunk_bytes = 4194304    # ファイル読み込みチャンク上限（デフォルト: 4MB）
```

## コマンドオプション

### 初期化

```bash
remote-merge init
```

プロジェクトルートに `.remote-merge.toml` を生成する。
グローバル設定は `~/.config/remote-merge/config.toml`、
プロジェクト設定は `.remote-merge.toml` を優先して読み込む。

#### 設定のマージ戦略

| セクション | マージ方式 |
|-----------|-----------|
| `[servers.*]` | プロジェクト設定で**上書き**。同名サーバはプロジェクト側が優先 |
| `[local]` | プロジェクト設定で**上書き** |
| `[filter].exclude` | グローバルとプロジェクトを**結合**（和集合） |
| `[filter].sensitive` | グローバルとプロジェクトを**結合**（和集合） |
| `[ssh]` | プロジェクト設定で**上書き** |
| `[backup]` | プロジェクト設定で**上書き** |
| `[agent]` | プロジェクト設定で**上書き** |

#### root_dir が存在しない場合

リモートの `root_dir` が存在しない、またはアクセス権がない場合は
接続時にエラーを表示し、設定の確認を促す。

```
┌─ エラー ─────────────────────────────────────────────┐
│ develop: /var/www/app が見つかりません                  │
│                                                       │
│ 原因の可能性:                                          │
│   - パスが存在しない                                   │
│   - ユーザ "deploy" にアクセス権がない                  │
│                                                       │
│ 設定ファイルの root_dir を確認してください               │
└───────────────────────────────────────────────────────┘
```

### グローバルオプション

全サブコマンド・TUI起動で共通のオプション。

```bash
remote-merge -v                    # ログレベル: info
remote-merge -vv                   # ログレベル: debug
remote-merge -vvv                  # ログレベル: trace
remote-merge --log-level debug     # ログレベルを明示指定
remote-merge --debug               # --log-level debug のショートハンド
```

| オプション | 説明 |
|-----------|------|
| `-v`, `-vv`, `-vvv` | verbosity 段階指定（info / debug / trace） |
| `--debug` | `--log-level debug` のショートハンド |
| `--log-level <LEVEL>` | ログレベルを明示指定（error / warn / info / debug / trace） |

CLIフラグが環境変数（`RUST_LOG`）より優先される。
ログ出力先は `~/.cache/remote-merge/debug.log`（既存動作と同じ）。

### TUI起動

```bash
remote-merge                                  # 設定ファイルを自動検出
remote-merge --right develop                  # サーバを指定して起動
remote-merge --left develop --right staging   # サーバ間比較で起動
```

### 非対話モード（CLIサブコマンド）

```bash
# 差分があるファイルの一覧
remote-merge status
remote-merge status --right develop            # サーバ指定（localとの比較）
remote-merge status --left develop --right staging  # サーバ間比較
remote-merge status --left develop --right staging --ref release  # 3way比較
remote-merge status --format json
remote-merge status --format json --summary
remote-merge status --all                      # equal を含む全ファイル表示
remote-merge status --checksum                 # コンテンツ比較を強制

# 特定ファイル・ディレクトリの差分
remote-merge diff src/config.ts --left local --right develop
remote-merge diff src/config.ts --format json
remote-merge diff src/                         # ディレクトリ再帰diff（テキスト出力のみ）
remote-merge diff src/ --max-files 20          # 出力ファイル数を制限（デフォルト: 100）
remote-merge diff .env --force                 # --force: セーフティガードを解除（sensitive ファイルの内容表示を許可）

# マージ実行（--left の内容で --right を上書き）
remote-merge merge src/config.ts --left develop --right local
remote-merge merge src/ --left develop --right local --dry-run   # 実行せず確認のみ
remote-merge merge src/ --left develop --right local --force     # 確認プロンプト省略（LLMエージェント向け）
remote-merge merge src/ --left develop --right local --delete    # --right のみのファイルを削除（完全同期）
remote-merge merge src/ --left develop --right local --with-permissions  # パーミッションもコピー

# 1:N マルチサーバ同期（--left の内容を複数 --right へ）
remote-merge sync --left local --right server1 server2 server3 --dry-run
remote-merge sync src/ --left local --right server1 server2
remote-merge sync --left local --right server1 server2 --delete --force
remote-merge sync --left local --right server1 server2 --format json

# ロールバック（バックアップからの復元）
remote-merge rollback --list                              # バックアップ一覧表示
remote-merge rollback --list --target develop             # 特定サーバのバックアップ一覧
remote-merge rollback --target develop                    # 直近セッションを復元
remote-merge rollback --target develop --session 20240115-140000  # 特定セッション復元
remote-merge rollback --target develop --dry-run          # プレビュー
remote-merge rollback --target develop --force            # 確認スキップ
remote-merge rollback --list --format json                # JSON出力

# ログ・イベント表示
remote-merge logs                              # デバッグログ表示
remote-merge logs --level warn                 # レベルフィルタ
remote-merge logs --since 1h                   # 期間フィルタ（5m, 1h, 30s）
remote-merge logs --tail 50                    # 最新N行
remote-merge logs --format json                # JSON出力

remote-merge events                            # TUIイベント表示
remote-merge events --type merge               # イベント種別フィルタ
remote-merge events --since 2024-01-15 --tail 20

# Agent サーバー起動（内部使用 — SSH経由で自動起動）
remote-merge agent --root /var/www/app
```

---

## LLMエージェント連携

非対話モードを使うことで、Claude Code などのLLMエージェントにコマンドを直接実行させられる。
TUIを起動せずに **「調査 → 判断 → マージ」を自然言語で丸投げ** できる。

```
ユーザー: 「developとlocalの差分を調べて、
            安全そうなファイルだけlocalに取り込んで」

Claude Code:
  1. remote-merge status --format json    # 差分ファイル一覧を取得
  2. remote-merge diff [各ファイル]       # 差分内容を確認・判断
  3. remote-merge merge [安全なファイル]  # マージ実行
  4. 結果を報告
```

### コンテキストウィンドウ対策

巨大なJSONを一度に渡すとLLMのコンテキストが埋め尽くされる問題がある。
以下の工夫で出力量をコントロールする。

**① statusはサマリーのみ返す（デフォルト）**

```bash
remote-merge status --format json
# → ファイルパスと状態のみ。diff内容は含まない
```

```bash
remote-merge status --format json --summary
# → ファイル一覧すら省略。集計数のみ返す
```

**② diffはファイル指定を必須にする**

```bash
# ディレクトリ指定は --format json では使用不可
remote-merge diff src/ --format json  # → エラー。ファイルを指定してください

# 1ファイルずつ取得させる
remote-merge diff src/config.ts --format json
```

**③ diffにトークン上限オプション**

```bash
remote-merge diff src/config.ts --format json --max-lines 100
# → 差分が大きい場合は先頭100行で打ち切り、truncatedフラグを立てる
```

**④ LLM向け推奨フロー**

```
Step 1: status --summary        # まず全体像を把握（最小出力）
Step 2: status                  # 差分ファイル一覧を取得
Step 3: diff [file] × 1ファイルずつ  # 必要なファイルだけ深掘り
Step 4: merge                   # 判断済みのファイルをマージ
```

LLMには最初から全差分を渡さず、**必要になったら1ファイルずつ取得**させる設計にする。

### JSON出力スキーマ

**`remote-merge status --format json`**

```json
{
  "left":  { "label": "local",   "root": "/home/user/app" },
  "right": { "label": "develop", "root": "dev.example.com:/var/www/app" },
  "ref":   { "label": "staging", "root": "stg.example.com:/var/www/app" },
  "agent": { "status": "connected" },
  "files": [
    {
      "path": "src/config.ts",
      "status": "modified",
      "sensitive": false,
      "hunks": 2,
      "ref_badge": "differs"
    },
    {
      "path": ".env",
      "status": "modified",
      "sensitive": true,
      "hunks": 1
    }
  ],
  "summary": {
    "modified":     2,
    "left_only":    0,
    "right_only":   1,
    "equal":        10,
    "ref_differs":  1,
    "ref_only":     0,
    "ref_missing":  0
  }
}
```

> **Note:** `ref`, `agent` は該当する場合のみ出力。`ref_badge` は 3way 比較時のみ。
> `ref_badge` の値: `"differs"` / `"exists_only_in_ref"` / `"missing_in_ref"`。
> `agent.status` の値: `"connected"`（Agent プロトコル使用）/ `"fallback"`（SSH exec フォールバック）。

**`remote-merge status --format json --summary`**

```json
{
  "left":  { "label": "local",   "root": "/home/user/app" },
  "right": { "label": "develop", "root": "dev.example.com:/var/www/app" },
  "summary": {
    "modified":   2,
    "left_only":  0,
    "right_only": 1,
    "equal":      10
  }
}
```

**`remote-merge diff src/config.ts --format json`**

```json
{
  "path": "src/config.ts",
  "left":  { "label": "local",   "updated_at": "2024-01-15T14:00:00Z" },
  "right": { "label": "develop", "updated_at": "2024-01-15T03:22:00Z" },
  "sensitive": false,
  "truncated": false,
  "conflict_count": 1,
  "hunks": [
    {
      "index": 0,
      "left_start": 10,
      "right_start": 10,
      "lines": [
        { "type": "context", "content": "  function hello() {" },
        { "type": "removed", "content": "-   const API_URL = \"https://api.example.com\"" },
        { "type": "added",   "content": "+   const API_URL = \"https://dev.example.com\"" },
        { "type": "context", "content": "  }" }
      ]
    }
  ],
  "ref_hunks": [ ... ],
  "conflict_regions": [
    {
      "ref_range": [10, 12],
      "left_lines": ["const x = 2"],
      "right_lines": ["const x = 3"]
    }
  ]
}
```

> **Note:** `sensitive`, `binary`, `symlink` は `false` 時省略（`skip_serializing_if`）。
> `conflict_count` は `0` 時省略。`ref_hunks`, `conflict_regions` は 3way 比較時のみ。
> `left_hash`, `right_hash` はバイナリ時のみ（SHA-256 hex）。
> `note` フィールドはセンシティブマスク理由等を格納（通常は省略）。

**`remote-merge merge --format json`**

```json
{
  "merged": [
    { "path": "src/config.ts", "status": "ok", "backup": "20240115-140000", "ref_badge": "differs" }
  ],
  "skipped": [
    { "path": ".env", "reason": "sensitive" }
  ],
  "deleted": [
    { "path": "src/old.ts", "status": "ok", "backup": "20240115-140000" }
  ],
  "failed": []
}
```

> **Note:** `deleted` は `--delete` 使用時のみ内容あり。`ref_badge` は 3way 時のみ。

**`remote-merge sync --format json`**

```json
{
  "left": { "label": "local", "root": "/home/user/app" },
  "targets": [
    {
      "target": { "label": "server1", "root": "s1:/var/www/app" },
      "merged": [...],
      "skipped": [...],
      "deleted": [...],
      "failed": [...],
      "status": "success"
    }
  ],
  "summary": {
    "total_servers": 2,
    "successful_servers": 2,
    "total_files_merged": 5,
    "total_files_deleted": 0,
    "total_files_failed": 0
  }
}
```

> **Note:** `status` の値: `"success"` / `"partial"` / `"failed"`。

---

## Agent プロトコル

リモートサーバとの通信を高速化するための専用プロトコル。
従来の SSH exec では 1ディレクトリにつき1回のコマンド実行が必要だったが、
Agent プロトコルでは **ツリー走査全体を3回のリクエストで完了** させる。

### 背景と効果

```
従来: find コマンド × N ディレクトリ = 1,200+ SSH exec 呼び出し
Agent: ListTree × 1-3チャンク + ReadFiles × 1-2バッチ = 3-5 呼び出し
```

Agent が利用できない環境（バイナリ配置不可、古いOS等）では
自動的に SSH exec フォールバックに切り替わる。JSON 出力の `agent.status` で確認可能。

### プロトコル仕様

| 項目 | 内容 |
|------|------|
| バージョン | 2 |
| ハンドシェイク | `"remote-merge agent v2"` |
| エンコーディング | MessagePack（rmp_serde） |
| トランスポート | SSH exec 経由の stdin/stdout |

### Agent デプロイ

Agent バイナリはリモートサーバに自動配置される。

```
1. リモートサーバ上の既存バイナリを検索
   → /usr/local/bin, ~/.local/bin, /opt/ 等
2. 見つからない場合: クロスコンパイル済み musl バイナリを SSH 経由でデプロイ
   → 配置先: [agent].deploy_dir（デフォルト: /var/tmp）
   → アトミックライト + SHA-256 チェックサム検証
3. Agent 起動: `remote-merge agent --root <dir>` を SSH exec で実行
```

### リクエスト・レスポンス

| リクエスト | レスポンス | 用途 |
|-----------|-----------|------|
| `ListTree { root, exclude, max_entries }` | `TreeChunk { nodes, is_last, total_scanned }` | ディレクトリツリー走査（ストリーミング） |
| `ReadFiles { paths, chunk_size_limit }` | `FileContents { results }` | バッチファイル読み込み |
| `WriteFile { path, content, is_binary, more_to_follow }` | `WriteResult { success, error }` | ファイル書き込み（チャンク対応） |
| `StatFiles { paths }` | `Stats { entries }` | メタデータ一括取得 |
| `Backup { paths, backup_dir }` | `BackupResult` | バックアップ作成 |
| `Symlink { path, target }` | `SymlinkResult` | シンボリックリンク作成 |
| `ListBackups { backup_dir }` | `BackupList { sessions }` | バックアップ一覧 |
| `RestoreBackup { backup_dir, session_id, files, root_dir }` | `RestoreResult { results }` | バックアップ復元 |
| `Ping` | `Pong` | 接続確認 |
| `Shutdown` | — | Agent 終了 |

### 設定

```toml
[agent]
enabled              = true       # false でSSH exec のみ使用
deploy_dir           = "/var/tmp" # バイナリ配置先
timeout_secs         = 30         # 応答タイムアウト
tree_chunk_size      = 1000       # ツリーチャンクサイズ
max_file_chunk_bytes = 4194304    # ファイルチャンク上限（4MB）
```

---

## レガシーSSH対応

古いSSHサーバ（OpenSSH 6.x以前など）は、モダンなクライアントのデフォルト設定では接続拒否されることがある。
サーバごとに使用するアルゴリズムを明示的に指定することで対応する。

### 背景と問題

```
モダンなSSHクライアント（デフォルト）
  → 古い/脆弱なアルゴリズムを無効化している
  → レガシーサーバが「共通のアルゴリズムがない」として接続拒否
  → "no matching key exchange method found" などのエラー
```

### 対応するアルゴリズム種別

| 種別 | 設定キー | レガシー向け例 |
|------|----------|---------------|
| 鍵交換 | `kex_algorithms` | `diffie-hellman-group1-sha1` |
| ホスト鍵 | `host_key_algorithms` | `ssh-rsa`, `ssh-dss` |
| 暗号化 | `ciphers` | `aes128-cbc`, `3des-cbc` |
| MAC | `mac_algorithms` | `hmac-sha1`, `hmac-md5` |

### 接続失敗時の自動ヒント

接続失敗時に、サーバが提示したアルゴリズムリストをエラーとして表示し、
設定ファイルへの記述例を自動提示する。

```
┌─ 接続エラー ─────────────────────────────────────────┐
│ legacy.example.com への接続に失敗しました             │
│                                                      │
│ 原因: 共通の鍵交換アルゴリズムが見つかりません          │
│                                                      │
│ サーバが提示したアルゴリズム:                          │
│   kex: diffie-hellman-group1-sha1                    │
│                                                      │
│ 設定ファイルに以下を追記してください:                   │
│                                                      │
│   [servers.legacy.ssh_options]                       │
│   kex_algorithms = ["diffie-hellman-group1-sha1"]    │
│                                                      │
│ [c]onfigを開く  [r]etry  [q]uit                      │
└──────────────────────────────────────────────────────┘
```

### `russh` での実装方針

`russh` はアルゴリズムのネゴシエーション設定を `Preferred` 構造体で制御できる。
設定ファイルの `ssh_options` を `russh::Preferred` にマッピングして接続時に渡す。

```rust
// 概念コード
let preferred = russh::Preferred {
    kex: vec![kex_algorithms...],
    key: vec![host_key_algorithms...],
    cipher: vec![ciphers...],
    mac: vec![mac_algorithms...],
    ..Default::default()
};
```

---

## 3way diff

3サーバ以上を比較する場合も基本は**2ペイン表示を維持**し、ペアを切り替えて比較する。
3つ目のサーバの状態はバッジで常時表示することで、ペインを切り替えずに全体像を把握できる。

### ペア切り替え

`s` キーでサーバ選択メニューを開き、比較ペアを切り替える。
remote ↔ remote の組み合わせも同様に選択可能。

```
┌─ サーバ選択 ──────────────────┐
│ LEFT          RIGHT           │
│ ──────────    ──────────      │
│ ● local       ○ develop  ✓   │
│ ○ develop     ○ staging       │
│ ○ staging     ○ release       │
│                               │
│ [現在] local ↔ develop        │
└───────────────────────────────┘
```

### 3つ目のサーバの状態バッジ

2ペイン表示のまま、各行に3つ目のサーバとの差異をバッジで表示する。

```
┌─ local ──────────────────┬─ develop ──────────────────┐
│   const x = 1    [≠STG] │   const x = 2      [≠ALL] │
│   hello()        [===]  │   hello()          [===]  │
│                  [+DEV] │   newFeature()     [+DEV] │
└──────────────────────────┴────────────────────────────┘
```

| バッジ | 意味 |
|--------|------|
| `[===]` | 全サーバ同一 |
| `[≠ALL]` | 全サーバで異なる |
| `[≠STG]` | 表示中の2サーバは同じ、stagingだけ違う |
| `[+DEV]` | developにのみ存在する行 |

比較対象が2サーバのみの場合はバッジを非表示にする。

### 3way サマリーパネル

`W` キーで3way サマリーパネルをトグル表示。
不一致箇所を一覧で確認し、詳細は2ペインで確認するという使い方を想定。

```
┌─ 3way サマリー: src/config.ts ──────────────────────────┐
│                                                         │
│  行10:  local="x=1"  develop="x=2"  staging="x=1"      │
│  行15:  local="foo"  develop="foo"  staging="bar"       │
│  行23:  local=[ 空 ] develop="newFeature()" staging=[ 空 ] │
│                                                         │
│ [Enter] 選択行にジャンプ  [W] 閉じる                     │
└─────────────────────────────────────────────────────────┘
```

---

## TUIレイアウト

```
┌──────────────────────────────────────────────────────────────────┐
│ remote-merge  [local] ←→ [develop ▼]        q:quit  ?:help      │
├─────────────────────┬────────────────────────────────────────────┤
│ File Tree           │ Diff View                                  │
│                     │                                            │
│ > src/              │  ┌─ local ──────────┬─ develop ──────────┐ │
│   ├ index.ts    [M] │  │ const x = 1      │ const x = 2        │ │
│   ├ config.ts   [M] │  │ // comment       │                    │ │
│   └ utils.ts    [=] │  │                  │ // new line        │ │
│ > nginx/            │  └──────────────────┴────────────────────┘ │
│   └ nginx.conf  [M] │                                            │
│ > .env          [M] │  更新日時                                   │
│                     │  local:   2024-01-15 14:00                 │
│                     │  develop: 2024-01-15 03:22 ← 深夜          │
├──────────────────────────────────────────────────────────────────┤
│ [L]eftMerge [R]ightMerge [c]クリップボードコピー [Tab]切替 [↑↓]移動 │
└──────────────────────────────────────────────────────────────────┘
```

### パネル構成

| パネル | 説明 |
|--------|------|
| **File Tree** | 左ペイン。差分状態をバッジで表示。ディレクトリは開閉式 |
| **Diff View** | 右ペイン上部。左右2ペインでインラインdiff表示 |
| **Metadata** | 右ペイン下部。ファイルの更新日時・パーミッションなど |

### フォーカスモデル

TUIには2つのフォーカス状態があり、`Tab` キーで切り替える。
現在のフォーカス先はハイライト表示で明示する。

| フォーカス | 有効なキー | 説明 |
|-----------|-----------|------|
| **File Tree** | `↑` `↓` `Enter` `Space` `L` `R` `/` | ファイル選択・ディレクトリ操作 |
| **Diff View** | `↑` `↓` `→` `←` | ハンク間移動・ハンク単位マージ |

- `→` `←`（ハンク単位マージ）は **Diff View フォーカス時のみ** 有効
- `L` `R`（ファイル/ディレクトリ全体マージ）は **File Tree フォーカス時のみ** 有効
- `s` `c` `f` `W` `q` `?` はフォーカスに関係なくグローバルに有効

---

## ファイルツリーの差分バッジ

| バッジ | 意味 |
|--------|------|
| `[M]` | Modified - 差分あり |
| `[=]` | Equal - 差分なし |
| `[+]` | Local Only - ローカルにのみ存在 |
| `[-]` | Remote Only - リモートにのみ存在 |
| `[?]` | Unchecked - 未比較（未展開） |

---

## マージ機能

### ファイル単位マージ

```
[L]eftMerge:  local の内容で remote を上書き
[R]ightMerge: remote の内容で local を上書き
```

### 行単位マージ

ファイル全体ではなく、差分ハンク（変更のかたまり）単位・行単位で選択してマージできる。
サーバ側にのみ先行実装された変更を部分的に取り込むケースなどに対応。

```
┌─ local ──────────────────┬─ develop ─────────────────┐
│   function hello() {     │   function hello() {       │
│     console.log("hi")    │     console.log("hi")      │
│   }                      │   }                        │
│                          │                            │
│ ·························│···························│ ← ハンク境界
│                          │                            │
│                          │   // 先行実装              │◄── カーソル
│                          │   function newFeature() {  │  [→] このハンクを取り込む
│                          │     return true            │
│                          │   }                        │
│                          │                            │
│ ·························│···························│
│   const x = 1            │   const x = 2             │◄── 別ハンク
│                          │                            │  [→] このハンクを取り込む
└──────────────────────────┴───────────────────────────┘
```

ハンクにカーソルを当てて `→` / `←` キーで該当箇所のみマージ。
「このハンクだけ右から取り込む」「この行だけ左を残す」が可能。

### 対象の選択

| 操作 | 対象 |
|------|------|
| ファイル選択時に `L` / `R` | ファイル全体 |
| ハンク選択時に `→` / `←` | ハンク単位 |
| ディレクトリ選択時に `L` / `R` | 配下のすべての差分ファイル（確認ダイアログあり） |

### ディレクトリマージ時の削除セマンティクス

ディレクトリ単位でマージする際、**片方にしか存在しないファイル**の扱いは以下の通り。

| ファイル状態 | デフォルト動作 | 説明 |
|-------------|---------------|------|
| `[M]` Modified | **上書き** | マージ元の内容でマージ先を上書き |
| `[+]` マージ元のみ存在 | **追加（コピー）** | マージ先に新規作成 |
| `[-]` マージ先のみ存在 | **保持（何もしない）** | マージ先のファイルを削除しない |
| `[=]` Equal | **スキップ** | 差分がないためマージ不要 |

デフォルトは **「上書き＋追加のみ（削除しない）」** 。
レガシー環境では「誰かが一時的に置いた原因不明のファイル」が存在することが多く、
意図しない削除による事故を防ぐためこの設計とする。

> **`--delete` オプション（実装済み）:** `--delete` オプションで rsync の `--delete` 相当の完全同期（マージ先のみに存在するファイルを削除）を実行可能。`merge` および `sync` サブコマンドで利用可能。削除前にバックアップを自動作成する。

### マージ前確認

```
┌─ 確認 ──────────────────────────────┐
│ 以下のファイルをマージします:         │
│                                     │
│  src/config.ts  local → develop     │
│  nginx/nginx.conf  local → develop  │
│                                     │
│ バックアップを作成しますか？ [Y/n]   │
└─────────────────────────────────────┘
```

### バックアップポリシー

マージ時のバックアップは以下のルールで管理する。

| 項目 | 内容 |
|------|------|
| 保存先 | マージ先の `.remote-merge-backup/` ディレクトリ |
| 命名規則 | `<元のパス>.<タイムスタンプ>.bak`（例: `src/config.ts.20240115-140000.bak`） |
| 自動削除 | 7日経過したバックアップを起動時に自動削除（設定で変更可能） |
| 設定 | `[backup]` セクションで `retention_days`、`enabled` を制御 |

```toml
[backup]
enabled        = true    # バックアップの有効/無効（デフォルト: true）
retention_days = 7       # 自動削除までの日数（デフォルト: 7）
```

### センシティブファイル警告

`.env`、`credentials.*`、`*.pem`、`*.key` など機密性の高いファイルを検知した場合、
マージ・diff表示の前に警告を表示する。

```
┌─ ⚠️  センシティブファイル検知 ─────────────────────┐
│ 以下のファイルには機密情報が含まれる可能性があります │
│                                                    │
│  .env                                              │
│  config/credentials.json                           │
│                                                    │
│ 続行しますか？ [y/N]                               │
└────────────────────────────────────────────────────┘
```

デフォルトのセンシティブパターンは組み込みで持ち、設定ファイルで追加・上書きできる。

```toml
[filter]
sensitive = [".env", ".env.*", "*.pem", "*.key", "credentials.*", "*secret*"]
```

CLI の `--format json` 出力では、センシティブファイルに `"sensitive": true` フラグを付与する。
LLMエージェントはこのフラグを見て自動マージ対象から除外できる。

### CLI diff での sensitive マスク

- デフォルトで sensitive ファイルの diff 内容は非表示（hunks 空 + note フィールドで通知）
- `--force` オプションで解除可能
- `sensitive && binary` の場合はハッシュもマスクされる
- JSON 出力では `note` フィールドにマスク理由を格納（`skip_serializing_if` で通常は省略）

---

## サーバ切替

ヘッダのサーバ名をタブで切り替え、または `s` キーで選択メニューを表示。

```
[local] ←→ [develop]
[local] ←→ [staging]
[local] ←→ [release]
[develop] ←→ [staging]   # サーバ間比較も対応
```

**接続はオンデマンド。** 切り替えた時点でSSH接続を確立。

### サーバ間比較（remote ↔ remote）

ローカルを介さずサーバ同士を直接比較できる。
両サーバのファイル内容をメモリ上に取得してdiffするため、一時ファイルは不要。

```
develop ──SSH──→ [メモリ上でdiff] ←──SSH── staging
                        │
                     表示・マージ
```

**マージ時の注意:**

サーバ間マージはマージ先サーバに直接SFTP書き込みを行う。
誤操作防止のため、リモート間マージ時のみ以下の制限を設ける。

```
┌─ ⚠️  警告 ──────────────────────────────────────────────┐
│ リモートサーバ間のマージを実行しようとしています           │
│                                                         │
│  src/config.ts                                          │
│  develop → staging                                      │
│  ※ staging サーバに直接書き込みます                      │
│  ※ ローカルには変更が反映されません                       │
│                                                         │
│ staging のバックアップを作成しますか？ [Y/n]             │
│                                                         │
│ 続行するには "staging" と入力してください: [          ]  │
└─────────────────────────────────────────────────────────┘
```

サーバ名の入力確認はリモート間マージ時のみ要求する。
`--force` オプションで確認プロンプトを省略可能（スクリプト・LLMエージェント向け）。

```bash
remote-merge merge src/config.ts --left develop --right staging --force
```

---

## 除外フィルター

`node_modules/` や `.git/` などをツリー表示・比較対象から除外する。
設定ファイルの `[filter]` セクションで glob パターンを指定する。

```toml
[filter]
exclude = ["node_modules", ".git", "dist", "*.log", "*.lock"]
```

`.remote-merge.toml` に記述することでプロジェクトごとに設定を切り替えられる。
TUI起動時に `f` キーでフィルター一覧を確認・一時的にトグルできる。

---

## バイナリファイルの扱い

画像・PDF・実行ファイルなどdiffできないファイルは内容比較をスキップし、
**ファイルサイズとSHA-256ハッシュ**のみで同一性を判定する。

**ハッシュ計算の実行場所:**
リモートファイルのハッシュ計算は **リモート側で SSH exec** を用いて実行する（`sha256sum` 等）。
ファイルをローカルにダウンロードしてからハッシュ計算するのは禁止。
これにより巨大ファイル（数GB）でもネットワーク転送量をハッシュ値の数バイトに抑える。

```
┌─ local ──────────────────┬─ develop ──────────────────┐
│  [binary file]           │  [binary file]             │
│  size: 102,400 bytes     │  size: 98,304 bytes        │
│  sha256: a1b2c3...       │  sha256: d4e5f6...         │
│                          │                            │
│  ※ 内容のdiffは表示できません                          │
└──────────────────────────┴────────────────────────────┘
```

マージ（ファイル丸ごとコピー）は通常通り実行可能。

---

## エラーハンドリング・エッジケース

### SSH接続断リカバリ

TUI操作中にSSH接続が切れた場合の動作を定義する。

| 状態 | 動作 |
|------|------|
| ツリー閲覧中 | ステータスバーにエラー表示。`r` キーで再接続を試行 |
| diff表示中 | キャッシュ済みの内容はそのまま表示。未取得の場合はエラー表示 |
| マージ実行中 | 転送を中断し、バックアップからの復元を提案 |

自動再接続は行わない（意図しない接続を防ぐため）。
ステータスバーに接続状態インジケータを常時表示する。

```
[local] ←→ [develop 🔴 切断]     r:再接続
```

### マージコンフリクト検知

ハンク単位マージで、ローカルとリモートが同じ行を変更している場合は
コンフリクトとしてマーク表示する。自動マージは行わない。

```
┌─ local ──────────────────┬─ develop ──────────────────┐
│   const x = 1            │   const x = 2              │
│ ▓▓▓ CONFLICT ▓▓▓▓▓▓▓▓▓▓ │ ▓▓▓ CONFLICT ▓▓▓▓▓▓▓▓▓▓▓ │
│   const y = "local"      │   const y = "remote"       │
│ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ │ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ │
└──────────────────────────┴────────────────────────────┘
```

コンフリクト箇所はハンク単位マージの `→` / `←` で**どちらか一方を選択**して解決する。

### 楽観的ロック（同時書き込み防止）

マージ実行時にリモートファイルの更新日時を再チェックし、
diff取得時点から変更されていた場合はマージを中断する。

```
┌─ ⚠️  ファイル変更検知 ──────────────────────────────┐
│ マージ先のファイルが更新されています                   │
│                                                      │
│  src/config.ts                                       │
│  diff取得時:  2024-01-15 14:00                       │
│  現在:       2024-01-15 14:23  ← 別の変更あり        │
│                                                      │
│ [r]eload（再読み込み）  [f]orce（強制上書き）  [c]ancel │
└──────────────────────────────────────────────────────┘
```

### シンボリックリンクの扱い

シンボリックリンクはリンク先を辿らず、**リンク自体の情報**を表示する。

| 表示項目 | 内容 |
|----------|------|
| バッジ | `[L]`（Link） |
| diff表示 | リンク先パスを比較。内容のdiffは行わない |
| マージ | リンク先パスの書き換え（`ln -sf` 相当） |

```
┌─ local ──────────────────┬─ develop ──────────────────┐
│  [symlink]                │  [symlink]                 │
│  → ../shared/config.json  │  → /etc/app/config.json   │
└──────────────────────────┴────────────────────────────┘
```

---

## ロールバック（rollback）

マージ操作の取り消しを安全に行うための CLI サブコマンド。

### バックアップ構造（セッションディレクトリ方式）

バックアップはセッションディレクトリ方式で管理する。
1回のマージ操作 = 1セッション（= 1ディレクトリ）として保存される。

```
.remote-merge-backup/
  20240115-140000/          # セッション1（= 1回のマージ操作）
    src/config.ts           # 元ファイルの相対パスをそのまま保持
    src/index.ts
  20240116-100000/          # セッション2
    src/config.ts
```

セッションID = タイムスタンプ。マージ操作の開始時に1度だけ生成し、
同一マージ内の全ファイルに同一セッションIDを適用する。
グルーピングロジックやファイル名パースが不要になり、
セッション単位の削除は `remove_dir_all()` で完結する。

### CLI インターフェース

```bash
# バックアップ一覧表示（セッション単位）
remote-merge rollback --list
remote-merge rollback --list --target develop

# 直近のマージ操作を復元（確認プロンプト付き）
remote-merge rollback --target develop

# 特定セッションを復元
remote-merge rollback --target develop --session 20240115-140000

# プレビュー（何が復元されるか表示、実行しない）
remote-merge rollback --target develop --dry-run

# 確認プロンプトをスキップ
remote-merge rollback --target develop --force

# JSON 出力
remote-merge rollback --list --target develop --format json
```

### CLIオプション

| オプション | 必須 | 説明 |
|-----------|------|------|
| `--target <side>` | `--list` 以外は必須 | 復元先サイド（local / サーバ名） |
| `--list` | - | バックアップ一覧表示（復元しない） |
| `--session <id>` | - | 復元対象セッションID。省略時は直近セッション |
| `--dry-run` | - | 復元対象のプレビュー（実行しない） |
| `--force` | - | 確認プロンプトスキップ + expired/sensitive 強制復元 |
| `--format <fmt>` | - | 出力形式（text / json）。デフォルト: text |

### Exit code

| 状態 | Code | 説明 |
|------|------|------|
| 復元成功（全ファイル） | 0 | 全ファイルの復元に成功 |
| 復元部分成功（一部失敗） | 2 | 一部ファイルの復元に失敗 |
| バックアップなし / セッションなし | 2 | 対象セッションが見つからない |
| `--dry-run`（復元対象あり） | 0 | プレビューのみ |
| `--list` | 0 | 一覧表示のみ |

### テキスト出力フォーマット

**`--list` モード:**

```
Backup sessions for develop:

  20240115-140000 (2 files)
    src/config.ts (1234 bytes)
    src/index.ts (5678 bytes)

  20240114-100000 (1 file) [expired]
    src/old.ts (456 bytes)
```

**復元実行時:**

```
Rollback session 20240115-140000 for develop:

  ✓ src/config.ts (pre-rollback backup: 20240116-090000)
  ✓ src/index.ts (pre-rollback backup: 20240116-090000)
  - src/app.ts (skipped: sensitive)

Restored 2 file(s), skipped 1.
```

### JSON 出力フォーマット

**`--list --format json`:**

```json
{
  "target": { "label": "develop", "root": "dev:/var/www" },
  "sessions": [
    {
      "session_id": "20240115-140000",
      "files": [
        { "path": "src/config.ts", "size": 1234 },
        { "path": "src/index.ts", "size": 5678 }
      ],
      "expired": false
    }
  ]
}
```

**復元実行 `--format json`:**

```json
{
  "target": { "label": "develop", "root": "dev:/var/www" },
  "session_id": "20240115-140000",
  "restored": [
    { "path": "src/config.ts", "pre_rollback_backup": "20240116-090000" },
    { "path": "src/index.ts", "pre_rollback_backup": "20240116-090000" }
  ],
  "skipped": [
    { "path": "src/app.ts", "reason": "sensitive" }
  ],
  "failed": []
}
```

### 動作仕様

| 項目 | 内容 |
|------|------|
| 対象 | `.remote-merge-backup/` 内のセッションディレクトリ |
| デフォルト | 直近の non-expired セッションを一括復元 |
| 確認 | 復元対象を一覧表示し、確認プロンプトを表示（`--force` で省略可） |
| 安全策 | 復元前に現在のファイルのバックアップを新セッションとして自動作成（rollback の rollback が可能） |
| sensitive | merge と同様に `is_sensitive()` でフィルタし、`--force` なしではスキップ |
| expired | 保持期間超過セッションは `--list` で `[expired]` 表示、復元はブロック（`--force` で上書き可） |

### Agent プロトコル統合

rollback はリモートサーバの操作にも対応しており、Agent プロトコル v2 の
`ListBackups` / `RestoreBackup` リクエストで高速化される。

### 制約事項

- バックアップの保持期間（`retention_days`）を超えたセッションは `--force` なしで復元不可
- マージ後にファイルが別途編集されている場合は警告を表示
- TUI の `u` キーは Diff View 内のハンクマージ undo（既存機能）であり、rollback とは別物
- セッションID はタイムスタンプ形式（`YYYYMMDD-HHMMSS`）のみ受け付ける
- パストラバーサル攻撃対策として、Agent 側で `validate_path()` を適用

---

## マルチサーバ同期（sync）

1つのソースを複数のサーバに同時同期する CLI サブコマンド。

### 基本コマンド

```bash
# ドライラン（差分確認のみ）
remote-merge sync --left local --right server1 server2 server3 --dry-run

# 同期実行
remote-merge sync --left local --right server1 server2 server3

# 特定パスのみ同期
remote-merge sync src/ --left local --right server1 server2

# JSON出力（LLMエージェント連携）
remote-merge sync --left local --right server1 server2 --dry-run --format json
```

### 動作仕様

| 項目 | 内容 |
|------|------|
| 実行順序 | **逐次実行**（server1 → server2 → server3 の順） |
| 失敗時 | 失敗したサーバをスキップし、次のサーバに続行。最後にエラーレポート表示 |
| バックアップ | 各サーバごとにバックアップ作成（既存のバックアップ機能を利用） |
| 確認 | 全サーバの差分サマリーを一覧表示後、一括確認プロンプト（`--force` で省略可） |

### 出力例（`--dry-run`）

```
Sync: local → server1, server2, server3

[server1] 3 files to merge (2 modified, 1 added)
  M  src/config.ts
  M  src/index.ts
  +  src/new-feature.ts

[server2] 1 file to merge (1 modified)
  M  src/config.ts

[server3] 3 files to merge (2 modified, 1 added)
  M  src/config.ts
  M  src/index.ts
  +  src/new-feature.ts

Total: 7 merge operations across 3 servers
```

### 制約事項

- `--left` に指定できるのは1サーバのみ（1:N の同期）
- 削除セマンティクスはデフォルト「削除しない」（`--delete` で完全同期可能）
- TUI モードでの sync は将来拡張（まず CLI のみ）

---

## sudo 対応 + ファイルパーミッション管理

### 概要

root 権限が必要なファイルへの書き込みを、Agent プロセスを `sudo` で起動することで実現する。
あわせて、マージ時のファイルパーミッション（owner / group / mode）を自動管理する。

### sudo Agent 起動

サーバー設定に `sudo = true` を指定すると、Agent プロセスを `sudo` で起動する。

| 項目 | 内容 |
|------|------|
| 前提条件 | NOPASSWD 設定済み（`sudo -n true` で事前チェック） |
| 起動方式 | `sudo '/path/to/remote-merge' agent --root <dir> [options]` |
| スコープ | Agent プロセス全体が root で動作。個別操作ごとの sudo は不要 |
| デフォルト | `sudo = false`（明示的に有効化が必要） |

#### Pre-flight チェック（Agent 起動前）

```
1. sudo=true の場合: `sudo -n true` を実行
   → 失敗時: エラーで即停止（フォールバックしない）
   → メッセージで NOPASSWD 設定を案内
2. `id -u {user} && id -g {user}` で SSH ユーザーの uid/gid を取得
3. config からパーミッション設定を resolve
4. Agent 起動コマンドに uid/gid/permissions を引数として渡す
```

### ファイルパーミッション管理

Agent がファイル書き込み後に自動的にメタデータ（owner / group / permissions）を設定する。

#### 挙動ルール

| ケース | owner/group | permissions |
|--------|-------------|-------------|
| 既存ファイル上書き | 元の owner/group を維持（chown） | 元の permissions を維持（chmod） |
| 新規ファイル作成 | SSH 接続ユーザーの uid/gid | 設定値（デフォルト `0o664`） |
| 新規ディレクトリ作成 | SSH 接続ユーザーの uid/gid | 設定値（デフォルト `0o775`） |
| Symlink 作成 | lchown で SSH 接続ユーザーの uid/gid を設定 | symlink 自体に chmod は不可（無視） |
| Backup 作成 | Agent プロセスの uid で作成 | デフォルト umask に従う |
| Backup 復元 | 既存ファイル上書きルールに従う | 同上 |

#### パーミッション解決フロー（Agent 内）

```
WriteFile リクエスト受信
├─ ファイルが存在する？
│  ├─ YES: stat で uid/gid/mode 取得 → 書き込み → (euid==0の場合のみ) chown + chmod で復元
│  └─ NO:  書き込み（create_dir_all で親ディレクトリも作成）
│          → 起動引数の default_permissions で chmod
│          → 起動引数の default_uid/gid で chown（euid==0 の場合のみ）
│          → 親ディレクトリも dir_permissions + chown を適用
└─ euid != 0 の場合: chown はそもそも実行しない（後述「非 root 時の挙動」参照）
```

#### 非 root 時の挙動

euid != 0 の場合（sudo 未使用時）:
- chown は **実行しない**（euid == 0 の場合のみ chown を実行する）
- chmod はファイル所有者であれば成功するため実行する
- chown をスキップした旨は `tracing::debug` でログに記録する

### 設定構造

#### グローバルデフォルト

```toml
[defaults]
file_permissions = "0o664"    # 新規ファイルのデフォルトパーミッション
dir_permissions  = "0o775"    # 新規ディレクトリのデフォルトパーミッション
```

#### サーバー単位オーバーライド

```toml
[servers.production]
host     = "prod-server"
user     = "deploy"
sudo     = true               # Agent を sudo で起動
file_permissions = "0o644"     # このサーバーでのファイルパーミッション
dir_permissions  = "0o755"     # このサーバーでのディレクトリパーミッション
```

#### パーミッション文字列のフォーマット

以下のフォーマットを受け付ける:

| フォーマット | 例 | 説明 |
|-------------|-----|------|
| `0o` prefix + 3桁の8進数 | `0o664` | Rust リテラル形式（推奨） |
| `0` prefix + 3桁 | `0664` | POSIX 慣習形式 |
| 3桁のみ | `664` | 省略形式 |

- 各桁は 0-7 の範囲
- `0o777` を超える値は拒否
- setuid/setgid/sticky bit（4桁目）は許可しない
- 不正な値は config ロード時にエラーで停止

#### 優先順位

```
サーバー設定 > グローバル [defaults] > ハードコードフォールバック（0o664 / 0o775）
```

### デフォルト値の伝搬方式

**Protocol v2 は変更しない。** デフォルト値は Agent 起動時の CLI 引数で渡す。

```bash
# sudo なし
'/var/tmp/remote-merge-deploy/remote-merge' agent --root '/app' \
  --default-uid 1000 --default-gid 1000 \
  --file-permissions 0o664 --dir-permissions 0o775

# sudo あり
sudo '/var/tmp/remote-merge-deploy/remote-merge' agent --root '/app' \
  --default-uid 1000 --default-gid 1000 \
  --file-permissions 0o644 --dir-permissions 0o755
```

WriteFile のペイロードは変更なし。Protocol バージョンも 2 のまま維持。

### SSH フォールバック時の挙動

Agent 非使用時（SSH exec によるファイル書き込み）は **sudo 対応しない**。

| 条件 | 挙動 |
|------|------|
| `sudo = false` + Agent 有効 | 通常動作（パーミッション管理あり、chown は非 root では skip） |
| `sudo = false` + Agent 無効 | SSH exec フォールバック（パーミッション管理なし） |
| `sudo = true` + Agent 有効 | sudo で Agent 起動（フルパーミッション管理） |
| `sudo = true` + Agent 起動失敗 | **エラーで停止（SSH フォールバック禁止）** |
| `sudo = true` + Agent 無効 | **設定エラー（config ロード時にサーバー接続前で拒否）** |

`sudo = true` が指定されている場合、Agent なしでの動作は許可しない。
`sudo = true` かつ `agent.enabled = false` の組み合わせは **config ロード時（サーバー接続前）** に検出し、エラーで即停止する。
エラーメッセージで Agent の有効化と NOPASSWD 設定を案内する。

### `--with-permissions` との関係

CLI の `--with-permissions` フラグとの棲み分けは以下の通り。

| 経路 | パーミッション管理 | `--with-permissions` |
|------|-------------------|---------------------|
| Agent 経由（sudo あり/なし問わず） | **常時有効** — Agent が自動的にパーミッション管理を行う | 不要（指定しても無視） |
| SSH フォールバック（Agent 未使用時） | `--with-permissions` 指定時のみ chmod を実行 | 従来通り明示的に指定 |

Agent 経由の場合は `--with-permissions` の有無にかかわらず常にパーミッションが保存される。
SSH フォールバック経由の場合は `--with-permissions` を明示的に指定した場合のみ chmod が走る。

### Agent デプロイ時の sudo 対応

Agent バイナリのデプロイ時（`mkdir -p`, `mv` 等のファイル操作）も `sudo = true` の場合は `sudo` prefix を付与する。
これにより、デプロイ先ディレクトリに root 権限が必要な場合でもバイナリ配置が可能になる。

### セキュリティ考慮事項

- `sudo` 起動は設定ファイルで明示的に `sudo = true` の場合のみ（デフォルト false）
- `sudo -n true` 事前チェック必須 — NOPASSWD 未設定ならエラーで即停止
- chown / chmod は **validate_path が返す canonical path に対してのみ実行** — root_dir 外へのパストラバーサルを防止
- Agent プロセスが root で動作する場合も validate_path によるパストラバーサル防止は維持
- sudo 使用時は `tracing::warn` でログを出力（意図的な使用であることの記録）
- パーミッション設定値のバリデーション（不正な値は起動時に拒否）
- **TOCTOU リスクの許容:** stat → write → chown/chmod の間に TOCTOU（Time-of-Check-to-Time-of-Use）リスクがあるが、Agent が単一プロセスでファイルを操作する前提のため許容する。外部プロセスが同時にファイルを操作するケースは楽観的ロック（mtime 再チェック）で検出する

---

## LLM連携（TUI側）

TUI内にAIチャットパネルは持たない。
LLMとの連携は以下の2つの手段で行う。

### クリップボードコピー

`c` キーで選択中のファイルのdiff情報をクリップボードにコピー。
Claude Code やブラウザなど任意のLLMにそのまま貼り付けて使う。

コピーされる内容：

```markdown
## remote-merge diff context

### ファイル
`src/config.ts`

### 比較対象
- left:  local        (更新: 2024-01-15 14:00)
- right: develop      (更新: 2024-01-15 03:22)

### diff
```diff
- const API_URL = "https://api.example.com"
+ const API_URL = "https://dev.example.com"
- const TIMEOUT = 5000
+ const TIMEOUT = 99999
```

### 質問（任意で編集してください）
（ユーザが自由に記入）
```

更新日時を含めることで「深夜の変更」などの文脈をLLMが読み取れる。
質問欄は空で渡し、貼り付け後にユーザが自由に編集する。

### レポート出力

`Shift+E` キーで現在の調査結果をMarkdownレポートとして出力。
diff情報・更新日時・サマリーをまとめてファイルに書き出す。
LLMへの一括投げ込みや記録用途に使う。

```markdown
## remote-merge 調査レポート 2024-01-15

### 比較対象
- local:   /home/user/projects/app
- develop: dev.example.com:/var/www/app

### 差分サマリー
- 変更あり: 3ファイル

### 詳細
...
```

---

## キーバインド

| キー | フォーカス | 動作 |
|------|-----------|------|
| `j` / `k` / `↑` `↓` | 両方 | File Tree: ファイル移動 / Diff View: 1行スクロール |
| `n` / `Shift+N` | Diff View | 次/前のハンクへジャンプ |
| `Enter` / `Space` | File Tree | ディレクトリ開閉 / ファイル選択 |
| `Tab` | グローバル | File Tree ↔ Diff View フォーカス切替 |
| `Shift+L` | File Tree | LeftMerge（leftの内容でrightを上書き） |
| `Shift+R` | File Tree | RightMerge（rightの内容でleftを上書き） |
| `→` | Diff View | 現在のハンクを right → left に取り込む |
| `←` | Diff View | 現在のハンクを left → right に取り込む |
| `u` | Diff View | undo（直前のハンクマージを取り消し） |
| `/` | File Tree | ファイル名インクリメンタルサーチ |
| `/` | Diff View | diff 内テキスト検索 |
| `Esc` | 両方 | 検索解除 / ダイアログ閉じる |
| `s` | グローバル | サーバ選択メニュー |
| `Shift+X` | グローバル | right↔ref ワンキースワップ（3way 時） |
| `Shift+W` | グローバル | 3way サマリーパネル トグル |
| `c` | グローバル | 選択ファイルのdiffをクリップボードにコピー |
| `f` | グローバル | フィルター一覧表示・トグル |
| `Shift+F` | グローバル | 変更ファイルのみ表示フィルター |
| `r` | グローバル | 選択ディレクトリを再読み込み / 再接続 |
| `Shift+T` | グローバル | カラーテーマ切替 |
| `Shift+S` | グローバル | シンタックスハイライト ON/OFF |
| `Shift+E` | グローバル | Markdownレポート出力 |
| `?` | グローバル | ヘルプ表示 |
| `q` | グローバル | 終了 |

> **Note:** `L` `R` `W` `X` `E` `F` `N` `T` `S` はShiftキー必須（大文字入力）。
> 小文字の `l` `r` `w` `e` `n` には別の機能を割り当てない（誤操作防止）。

---

## 技術スタック

| 用途 | クレート |
|------|---------|
| TUI フレームワーク | `ratatui` + `crossterm` |
| 非同期ランタイム | `tokio` |
| SSH / SFTP | `russh` |
| diff 生成 | `similar` |
| シンタックスハイライト | `syntect` |
| クリップボード | `arboard` |
| ファイル走査 | `walkdir` |
| ハッシュ計算 | `sha2` + `sha1` |
| 設定ファイル | `toml` + `serde` |
| JSON出力 | `serde_json` |
| MessagePack（Agent） | `rmp-serde` |
| エラーハンドリング | `anyhow` + `thiserror` |
| ログ | `tracing` + `tracing-subscriber` |
| 日時処理 | `chrono` |
| CLI引数 | `clap` |
| テストカバレッジ | `cargo-llvm-cov` |

---

## ディレクトリ構成

```
remote-merge/
├── src/
│   ├── main.rs              # CLIエントリポイント + TUI起動
│   ├── lib.rs               # モジュール宣言
│   ├── tree.rs              # FileTree/FileNode データ構造
│   ├── config.rs            # TOML設定パーサー（グローバル・プロジェクト・フィルター）
│   ├── filter.rs            # パス除外フィルター（include/exclude、パス全体マッチ）
│   ├── error.rs             # カスタムエラー型
│   ├── init.rs              # 対話的初期化（.remote-merge.toml 生成）
│   ├── state.rs             # TUI状態ダンプ（JSON/テキスト）
│   │
│   ├── app/                 # ドメイン層 — アプリケーション状態・純粋ロジック
│   │   ├── mod.rs           #   AppState 構造体 + 初期化
│   │   ├── types.rs         #   Badge, Focus, DiffMode, FlatNode 等の型定義
│   │   ├── badge.rs         #   バッジ計算エンジン
│   │   ├── selection.rs     #   ファイル選択・フォーカス管理
│   │   ├── tree_ops.rs      #   ツリー平坦化・展開・パス操作
│   │   ├── navigation.rs    #   カーソル移動・スクロール・フォーカス
│   │   ├── hunk_ops.rs      #   ハンク単位マージ + undo
│   │   ├── dialog_ops.rs    #   ダイアログ操作（マージ・フィルタ・ヘルプ）
│   │   ├── server_switch.rs #   サーバ切替・再接続時の状態復元
│   │   ├── search.rs        #   ファイル名インクリメンタルサーチ
│   │   ├── diff_search.rs   #   差分検索
│   │   ├── three_way.rs     #   3way diff 状態管理
│   │   ├── ref_swap.rs      #   right↔ref スワップ
│   │   ├── report.rs        #   Markdownレポート出力
│   │   ├── merge_collect.rs #   マージ対象ファイル集約
│   │   ├── cache.rs         #   ツリーキャッシュ
│   │   ├── clipboard.rs     #   クリップボード操作
│   │   ├── scan.rs          #   スキャン状態管理
│   │   └── undo.rs          #   undo 実装
│   │
│   ├── runtime/             # サービス層 — I/O・非同期処理
│   │   ├── mod.rs           #   TuiRuntime 構造体
│   │   ├── core.rs          #   CoreRuntime（CLI/TUI共通の SSH・I/O 基盤）
│   │   ├── bootstrap.rs     #   TUI 初期化
│   │   ├── scanner.rs       #   ツリー非同期走査スレッド
│   │   ├── remote_io.rs     #   リモート I/O（SSH/SFTP）
│   │   ├── side_io.rs       #   Side-agnostic I/O API
│   │   └── merge_scan/      #   マージスキャン管理
│   │       ├── mod.rs
│   │       ├── task.rs      #     マージタスク
│   │       ├── poll.rs      #     ポーリング
│   │       └── apply.rs     #     マージ適用
│   │
│   ├── handler/             # イベントハンドラ層 — キー入力 → サービス呼び出し
│   │   ├── mod.rs           #   イベントルーティング
│   │   ├── tree_keys.rs     #   ツリービューのキー処理
│   │   ├── diff_keys.rs     #   diff ビューのキー処理
│   │   ├── dialog_keys.rs   #   ダイアログのキー処理
│   │   ├── search_keys.rs   #   検索時のキー処理
│   │   ├── diff_search_keys.rs # diff 内検索のキー処理
│   │   ├── merge_batch.rs   #   バッチマージ実行
│   │   ├── merge_content.rs #   マージ内容決定ロジック
│   │   ├── merge_exec.rs    #   マージ実行エンジン
│   │   ├── merge_mtime.rs   #   mtime 再チェック（楽観的ロック）
│   │   ├── merge_file_io.rs #   ファイル書き込みラッパー
│   │   ├── merge_tree_load.rs # マージ後のツリー再読み込み
│   │   ├── symlink_merge.rs #   シンボリックリンクマージ
│   │   └── reconnect.rs     #   再接続・サーバ切替実装
│   │
│   ├── service/             # ビジネスロジック層（CLI サブコマンド用）
│   │   ├── mod.rs
│   │   ├── types.rs         #   JSON 出力型定義（StatusOutput, DiffOutput, MergeOutput 等）
│   │   ├── status.rs        #   ステータス集計
│   │   ├── diff.rs          #   diff 出力
│   │   ├── merge.rs         #   マージ制御
│   │   ├── merge_flow.rs    #   マージフロー（削除計画・実行）
│   │   ├── rollback.rs      #   ロールバック純粋関数（mark_expired, plan_restore, rollback_exit_code）
│   │   ├── sync.rs          #   1:N マルチサーバ同期ロジック
│   │   ├── output.rs        #   JSON/テキスト出力
│   │   ├── path_resolver.rs #   パス解決
│   │   └── source_pair.rs   #   ソースペア解析
│   │
│   ├── cli/                 # CLI 実装層
│   │   ├── mod.rs
│   │   ├── status.rs        #   status サブコマンド
│   │   ├── diff.rs          #   diff サブコマンド
│   │   ├── merge.rs         #   merge サブコマンド（--dry-run, --force）
│   │   ├── rollback.rs      #   rollback サブコマンド（--list, --dry-run, --force, --session）
│   │   ├── sync.rs          #   sync サブコマンド（1:N マルチサーバ同期）
│   │   ├── logs.rs          #   logs サブコマンド（構造化ログ取得）
│   │   └── events.rs        #   events サブコマンド（イベント取得）
│   │
│   ├── ui/                  # UI 層 — 描画・ウィジェット
│   │   ├── mod.rs
│   │   ├── render.rs        #   TUI 全体描画
│   │   ├── tree_view.rs     #   ツリービューウィジェット
│   │   ├── layout.rs        #   レイアウト定義
│   │   ├── metadata.rs      #   ファイルメタデータ表示
│   │   ├── diff_view/       #   diff ビュー
│   │   │   ├── mod.rs       #     メインレンダリング
│   │   │   ├── content_render.rs # 行内容レンダリング
│   │   │   ├── line_render.rs #   行書式（色・バッジ）
│   │   │   ├── style_utils.rs #   スタイルユーティリティ
│   │   │   ├── search.rs    #     diff 内検索 UI
│   │   │   └── three_way_badge.rs # 3way バッジ表示
│   │   └── dialog/          #   ダイアログ群
│   │       ├── mod.rs       #     ダイアログ基盤
│   │       ├── confirm.rs   #     確認ダイアログ
│   │       ├── batch_confirm.rs # バッチマージ確認
│   │       ├── server_menu.rs #   サーバ選択メニュー
│   │       ├── pair_server_menu.rs # 3way 用 2 列選択 UI
│   │       ├── filter_panel.rs #  フィルターパネル
│   │       ├── hunk_preview.rs #  ハンクプレビュー
│   │       ├── mtime_warning.rs # mtime 警告
│   │       └── help.rs      #     ヘルプダイアログ
│   │
│   ├── ssh/                 # SSH/SFTP 通信層
│   │   ├── mod.rs
│   │   ├── client.rs        #   SSH 接続・認証・コマンド実行
│   │   ├── tree_parser.rs   #   find 出力をツリーに変換
│   │   ├── known_hosts.rs   #   known_hosts 検証・レガシー SSH 対応
│   │   ├── batch_read.rs    #   バッチファイル読み込み
│   │   └── hint.rs          #   SSH アルゴリズムヒント
│   │
│   ├── diff/                # 差分計算エンジン
│   │   ├── mod.rs
│   │   ├── engine.rs        #   similar crate ベースの diff 計算
│   │   ├── binary.rs        #   SHA-256 ハッシュ比較（バイナリ）
│   │   ├── symlink.rs       #   シンボリックリンク比較・マージ
│   │   └── conflict.rs      #   3way コンフリクト検出（ref ベース）
│   │
│   ├── merge/               # マージ実行エンジン
│   │   ├── mod.rs
│   │   ├── executor.rs      #   マージ実行（左/右/中央への上書き）
│   │   └── optimistic_lock.rs # 楽観的ロック実装
│   │
│   ├── local/               # ローカルファイルシステム
│   │   └── mod.rs           #   再帰的ツリースキャン（WalkDir）
│   │
│   ├── backup/              # マージ前バックアップ
│   │   └── mod.rs           #   .remote-merge-backup/ 管理
│   │
│   ├── highlight/           # シンタックスハイライト
│   │   ├── mod.rs
│   │   ├── engine.rs        #   syntect ベースエンジン
│   │   ├── convert.rs       #   syntect → ratatui Color 変換
│   │   └── cache.rs         #   ハイライトキャッシュ
│   │
│   ├── theme/               # カラーテーマシステム
│   │   ├── mod.rs
│   │   └── palette.rs       #   12 色パレット定義
│   │
│   ├── agent/               # Agent プロトコル（高速リモート操作）
│   │   ├── mod.rs
│   │   ├── protocol.rs     #   リクエスト・レスポンス型定義（MessagePack）
│   │   ├── framing.rs      #   メッセージフレーミング（長さプレフィクス）
│   │   ├── client.rs       #   Agent クライアント（リクエスト送受信）
│   │   ├── server.rs       #   Agent サーバー（リクエストハンドラ）
│   │   ├── dispatch.rs     #   リクエストディスパッチ
│   │   ├── deploy.rs       #   Agent バイナリのリモートデプロイ
│   │   ├── ssh_transport.rs #  SSH 経由の Agent トランスポート
│   │   ├── tree_scan.rs    #   ツリー走査（チャンクストリーミング）
│   │   └── file_io.rs      #   ファイル読み書き（バッチ処理）
│   │
│   └── telemetry/           # ログ・イベント記録
│       ├── mod.rs
│       ├── event_types.rs   #   イベント型定義
│       ├── event_recorder.rs #  イベント記録
│       ├── structured_log.rs # 構造化ログ
│       ├── log_reader.rs    #   ログファイル読み込み
│       ├── state_dumper.rs  #   定期的な状態ダンプ
│       └── truncate.rs      #   ログローテーション
│
├── Cargo.toml
├── CLAUDE.md
├── spec.md
├── .github/workflows/       # CI/CD
│   ├── ci.yml               #   push/PR 時: fmt + clippy + test
│   └── release.yml           #   v* タグ時: Linux/macOS/Windows クロスビルド
└── docs/
    ├── status.md             # 進捗管理
    └── cycles/               # サイクル別実装計画
```

---

## 実装フェーズ

### Phase 1: MVP（最小限の動作するプロダクト）
- [x] SSH接続（鍵認証）・ファイルツリー取得
- [x] 遅延読み込み（ディレクトリ開閉）
- [x] diff表示（2ペイン）
- [x] 差分バッジ表示（`[M]` `[=]` `[+]` `[-]` `[?]`）
- [x] ファイル全体のLeftMerge / RightMerge
- [x] マージ前確認ダイアログ
- [x] サーバ切替（local ↔ remote）
- [x] `init` コマンド・設定ファイル生成
- [x] 除外フィルター（`[filter]`）
- [x] 接続タイムアウト設定

### Phase 2: 高度なマージ・比較機能
- [x] ハンク単位マージ（Diff View フォーカスモデル）
- [x] 3way diff バッジ表示・ペア切り替え（PairServerMenu 2列選択UI、--ref CLI引数）
- [x] right↔ref ワンキースワップ（`X` キー）・Equal時ref diff自動表示
- [x] ディレクトリバッジの ref 差分色分け
- [x] 3way サマリーパネル（`W` キー）
- [x] サーバ間比較（remote ↔ remote）
- [x] リモート間マージ（サーバ名入力確認・`--force`）
- [x] 更新日時・メタデータ表示
- [x] バックアップ機能
- [x] 楽観的ロック（同時書き込み防止）
- [x] コンフリクト検知・表示（3way ref ベース、conflict_regions JSON 出力）
- [x] バイナリファイル対応（ハッシュ比較）
- [x] シンボリックリンク対応

### Phase 3: UX・堅牢性
- [x] レガシーSSHアルゴリズム対応（接続失敗時ヒント表示）
- [x] パスワード認証対応
- [x] センシティブファイル警告
- [x] ファイルパーミッション制御（`--with-permissions`）
- [x] ファイル名検索（`/` キー）
- [x] Diff View内テキスト検索
- [x] フィルターTUIトグル（`f` キー）
- [x] 変更ファイルフィルター（`Shift+F` キー）
- [x] クリップボードコピー
- [x] レポート出力（`Shift+E`）
- [x] SSH接続断リカバリ・接続状態インジケータ
- [x] root_dir不在時のエラーハンドリング
- [x] シンタックスハイライト（syntect ベース・テーマ切替 `T` キー・ON/OFF `S` キー）
- [x] VSCode準拠スクロールマージン（上下3行）
- [x] ディレクトリ再帰マージ（非同期化 + プログレス表示）
- [x] バッチマージ（複数ファイル同時マージ + 確認フロー）
- [x] サーバ切替時のツリー展開状態・カーソル位置維持
- [x] Side-agnostic I/O API（local/remote 決め打ちの根絶）
- [x] パス全体マッチ対応 exclude パターン（`config/*.toml`, `vendor/legacy/**`）

### Phase 4: CLIサブコマンド（LLMエージェント連携）
- [x] `status` コマンド（テキスト・JSON出力・`--summary`）
- [x] `diff` コマンド（ファイル指定・`--max-lines`・`--max-files`）
- [x] `merge` コマンド（`--dry-run`・`--force`）
- [x] JSON出力へのセンシティブフラグ付与
- [x] コンテキストウィンドウ対策（段階的取得フロー）
- [x] CoreRuntime分離（TuiRuntime → CoreRuntime + TuiRuntime）
- [x] Service層基盤 + 型定義（service/types.rs, source_pair.rs, output.rs）
- [x] TUI監視基盤（state.json / screen.txt / events.jsonl ファイルダンプ）
- [x] telemetry（event_recorder, state_dumper, truncate）
- [x] Skill ファイル（LLMエージェント向けガイダンス）
- [x] `logs` CLIサブコマンド（debug.log のフィルタ・表示: --level, --since, --tail, --format json）
- [x] `events` CLIサブコマンド（events.jsonl のフィルタ・表示: --type, --since, --tail）
- [x] structured_log.rs（tracing JSON Layer — `logs --format json` の基盤）

### Phase 4.5: Agent プロトコル

- [x] Agent プロトコル v2（MessagePack エンコーディング）
- [x] リクエスト・レスポンス型定義（ListTree, ReadFiles, WriteFile, StatFiles 等）
- [x] チャンクストリーミング（ツリー走査の大規模対応）
- [x] Agent クライアント・サーバー実装
- [x] Agent バイナリの自動デプロイ（musl クロスコンパイル + アトミックライト）
- [x] SSH exec フォールバック（Agent 利用不可時の自動切替）
- [x] `agent` CLIサブコマンド（内部使用）
- [x] `[agent]` 設定セクション

### Phase 5: 運用・同期機能

- [x] `--debug` / `-v` / `--log-level` グローバルオプション
- [x] ディレクトリマージ時の削除セマンティクス（デフォルト: 削除しない、`--delete` で完全同期）
- [x] `rollback` CLIサブコマンド（バックアップからの即時復元）
- [x] `sync` CLIサブコマンド（1:N マルチサーバ同期）
- [x] `--delete` オプション（merge / sync でマージ先のみのファイルを削除）

### Phase 6: sudo Agent 起動 + ファイルパーミッション管理

- [ ] `sudo = true` 設定の config バリデーション（`agent.enabled = false` との排他チェック）
- [ ] `sudo -n true` Pre-flight チェック
- [ ] Agent プロセスの sudo 起動
- [ ] Agent デプロイ時の sudo prefix 対応
- [ ] ファイルパーミッション自動管理（既存ファイル維持 / 新規ファイルにデフォルト適用）
- [ ] chown の euid==0 ガード
- [ ] パーミッション文字列のパース・バリデーション
- [ ] `--with-permissions` と Agent 経路の統合

### CI/CD・品質管理
- [x] GitHub Actions CI（push/PR 時: fmt + clippy + test）
- [x] GitHub Actions Release（v* タグ時: Linux/macOS/Windows クロスビルド）
- [x] pre-commit / pre-push フック（fmt + clippy）
- [x] cargo-llvm-cov テストカバレッジ基盤（行カバレッジ 76%+）
- [x] 2,300+ ユニットテスト

### 進捗サマリー

| Phase | 状態 | 残タスク |
|-------|------|---------|
| Phase 1: MVP | 完了 | — |
| Phase 2: 高度なマージ・比較 | 完了 | — |
| Phase 3: UX・堅牢性 | 完了 | — |
| Phase 4: CLI + Skill | 完了 | — |
| Phase 4.5: Agent プロトコル | 完了 | — |
| Phase 5: 運用・同期 | 完了 | — |
| Phase 6: sudo + パーミッション管理 | 未着手 | sudo Agent 起動、パーミッション自動管理、バリデーション |
| CI/CD・品質管理 | 完了 | — |