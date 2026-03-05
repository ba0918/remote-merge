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

[local]
root_dir = "/home/user/projects/app"

[filter]
# ツリー表示・比較から除外するパターン
exclude = ["node_modules", ".git", "dist", "*.log", "*.lock"]

[ssh]
timeout_sec = 10   # 接続タイムアウト（デフォルト: 10秒）
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

### TUI起動

```bash
remote-merge                                  # 設定ファイルを自動検出
remote-merge --server develop                 # サーバを指定して起動
remote-merge --left develop --right staging   # サーバ間比較で起動
```

### 非対話モード（CLIサブコマンド）

```bash
# 差分があるファイルの一覧
remote-merge status
remote-merge status --server develop           # サーバ指定（localとの比較）
remote-merge status --left develop --right staging  # サーバ間比較
remote-merge status --format json
remote-merge status --format json --summary

# 特定ファイル・ディレクトリの差分
remote-merge diff src/config.ts --left local --right develop
remote-merge diff src/config.ts --format json
remote-merge diff src/                         # ディレクトリ再帰diff（テキスト出力のみ）
remote-merge diff src/ --max-files 20          # 出力ファイル数を制限（デフォルト: 無制限）

# マージ実行（--left の内容で --right を上書き）
remote-merge merge src/config.ts --left develop --right local
remote-merge merge src/ --left develop --right local --dry-run   # 実行せず確認のみ
remote-merge merge src/ --left develop --right local --force     # 確認プロンプト省略（LLMエージェント向け）
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
  "files": [
    {
      "path": "src/config.ts",
      "status": "modified",
      "left_updated_at":  "2024-01-15T14:00:00Z",
      "right_updated_at": "2024-01-15T03:22:00Z",
      "hunks": 2
    },
    {
      "path": ".env",
      "status": "modified",
      "left_updated_at":  "2024-01-10T10:00:00Z",
      "right_updated_at": "2024-01-15T03:25:00Z",
      "hunks": 1
    }
  ],
  "summary": {
    "modified":   2,
    "left_only":  0,
    "right_only": 1,
    "equal":      10
  }
}
```

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
  "truncated": false,
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
  ]
}
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

## ファイルパーミッションの扱い

マージ時のパーミッション処理はオプションで制御する。

| オプション | 動作 |
|-----------|------|
| デフォルト | パーミッションは変更しない（内容のみ上書き） |
| `--with-permissions` | パーミッションもコピー元に合わせる |

リモート間マージ時は意図しないパーミッション変更を防ぐため、
デフォルトで内容のみ上書きとする。

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
| `↑` `↓` | 両方 | File Tree: ファイル移動 / Diff View: ハンク移動 |
| `Enter` / `Space` | File Tree | ディレクトリ開閉 / ファイル選択 |
| `Tab` | グローバル | File Tree ↔ Diff View フォーカス切替 |
| `Shift+L` | File Tree | LeftMerge（leftの内容でrightを上書き） |
| `Shift+R` | File Tree | RightMerge（rightの内容でleftを上書き） |
| `→` | Diff View | 現在のハンクを right → left に取り込む |
| `←` | Diff View | 現在のハンクを left → right に取り込む |
| `/` | File Tree | ファイル名インクリメンタルサーチ |
| `n` / `Shift+N` | File Tree | 検索結果の次/前へジャンプ |
| `Esc` | 両方 | 検索解除 / ダイアログ閉じる |
| `s` | グローバル | サーバ選択メニュー |
| `Shift+W` | グローバル | 3way サマリーパネル トグル |
| `c` | グローバル | 選択ファイルのdiffをクリップボードにコピー |
| `f` | グローバル | フィルター一覧表示・トグル |
| `r` | グローバル | 選択ディレクトリを再読み込み / 再接続 |
| `Shift+E` | グローバル | レポート出力 |
| `?` | グローバル | ヘルプ表示 |
| `q` | グローバル | 終了 |

> **Note:** `L` `R` `W` `E` `N` はShiftキー必須（大文字入力）。
> 小文字の `l` `r` `w` `e` `n` には別の機能を割り当てない（誤操作防止）。

---

## 技術スタック

| 用途 | クレート |
|------|---------|
| TUI フレームワーク | `ratatui` |
| 非同期ランタイム | `tokio` |
| SSH / SFTP | `russh` |
| diff 生成 | `similar` |
| 設定ファイル | `toml` + `serde` |
| エラーハンドリング | `anyhow` |
| ログ | `tracing` |

---

## ディレクトリ構成（実装想定）

```
remote-merge/
├── src/
│   ├── main.rs
│   ├── app.rs            # アプリ状態管理
│   ├── config.rs         # 設定ファイル読み込み
│   ├── ssh/
│   │   ├── mod.rs
│   │   ├── client.rs     # SSH接続・exec
│   │   └── sftp.rs       # ファイル転送
│   ├── diff/
│   │   ├── mod.rs
│   │   └── engine.rs     # diff計算・バイナリ判定
│   ├── filter.rs         # 除外フィルター
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── status.rs     # statusサブコマンド
│   │   ├── diff.rs       # diffサブコマンド
│   │   └── merge.rs      # mergeサブコマンド
│   └── ui/
│       ├── mod.rs
│       ├── tree.rs       # ファイルツリーパネル
│       └── diff_view.rs  # Diffパネル
├── Cargo.toml
└── README.md
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
- [ ] 3way diff バッジ表示・ペア切り替え
- [ ] 3way サマリーパネル
- [ ] サーバ間比較（remote ↔ remote）
- [ ] リモート間マージ（サーバ名入力確認・`--force`）
- [ ] 更新日時・メタデータ表示
- [ ] バックアップ機能
- [ ] 楽観的ロック（同時書き込み防止）
- [ ] コンフリクト検知・表示
- [x] バイナリファイル対応（ハッシュ比較）
- [x] シンボリックリンク対応

### Phase 3: UX・堅牢性
- [ ] レガシーSSHアルゴリズム対応（接続失敗時ヒント表示）
- [x] パスワード認証対応
- [ ] センシティブファイル警告
- [ ] ファイルパーミッション制御（`--with-permissions`）
- [ ] ファイル名検索（`/` キー）
- [x] フィルターTUIトグル（`f` キー）
- [ ] クリップボードコピー
- [ ] レポート出力（`Shift+E`）
- [x] SSH接続断リカバリ・接続状態インジケータ
- [ ] root_dir不在時のエラーハンドリング

### Phase 4: CLIサブコマンド（LLMエージェント連携）
- [ ] `status` コマンド（テキスト・JSON出力・`--summary`）
- [ ] `diff` コマンド（ファイル指定・`--max-lines`・`--max-files`）
- [ ] `merge` コマンド（`--dry-run`・`--force`）
- [ ] JSON出力へのセンシティブフラグ付与
- [ ] コンテキストウィンドウ対策（段階的取得フロー）