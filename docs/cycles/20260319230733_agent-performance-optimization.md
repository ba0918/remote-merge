# perf: Agent パフォーマンス最適化

**Cycle ID:** `20260319230733`
**Started:** 2026-03-19 23:07:33
**Status:** 🟡 Planning
**Issue:** 2026-03-19_agent-performance-optimization

---

## 📝 What & Why

Agent 経由の `status` が SSH と比べて有意な速度差がない。`MAX_FRAME_SIZE` (16MB) 制限のため `AGENT_READ_BATCH_SIZE = 100` ファイルずつチャンク分割しており、SSH バッチ読み込みと往復回数が同程度になっている。Agent の本来の優位性（プロトコル効率）を活かすため、ストリーミングレスポンス + ハッシュ比較を導入する。

## 🎯 Goals

- Agent `read_files` をストリーミングレスポンスに対応させ、バッチサイズ制限を撤廃する（案 A）
- `hash_files` コマンドを新設し、status 比較の転送量を激減させる（案 B）
- プロトコルバージョンを bump して後方互換性を維持する

## 📐 Design

### 現状分析

```
現在のフロー (status):
  Client                          Agent
    |-- ReadFiles {100 paths} -->   |
    |<-- FileContents {results} --  |
    |-- ReadFiles {100 paths} -->   |  ← 往復が SSH と同程度
    |<-- FileContents {results} --  |
    ...

SSH フロー:
    |-- cat batch (ARG_MAX制限) -->  |
    |<-- delimiter-separated output |
    ...
```

### 改善後のフロー

#### 案 B: HashFiles コマンド (status 特化) — 最優先

```
  Client                          Agent
    |-- HashFiles {2000 paths} --> |
    |<-- FileHashes {              |
    |     path→sha256 map,         |  ← 32 bytes/file vs 数KB〜MB
    |     is_last: true}        -- |
```

- 新しい `AgentRequest::HashFiles` / `AgentResponse::FileHashes` を追加
- Agent がファイルの SHA-256 ハッシュのみを計算して返す
- ローカル側もファイルの SHA-256 を計算し、ハッシュ同士を比較して equal/modified を判定
- ストリーミング対応（大量ファイル時は `is_last: false` で分割）
- シンボリックリンクはハッシュではなくリンクターゲットパスを返す（プロジェクト仕様: シンボリックリンクはターゲットパスで比較）

#### 案 A: ストリーミング ReadFiles

```
  Client                          Agent
    |-- ReadFiles {2000 paths} --> |
    |<-- FileContents {chunk1,     |
    |     is_last: false}       -- |  ← Agent がフレームサイズ内で
    |<-- FileContents {chunk2,     |    自律的に分割
    |     is_last: true}        -- |
```

- `list_tree` と同じストリーミングパターンを `read_files` に適用
- Agent 側がレスポンスサイズを `MAX_FRAME_SIZE` 内に収めてチャンク分割
- クライアントは `is_last: true` まで読み続ける
- `AGENT_READ_BATCH_SIZE` 制限が不要になる

### Files to Change

```
src/
  agent/
    protocol.rs        - AgentResponse::FileContents に is_last 追加 (#[serde(default)])
                       - AgentRequest::HashFiles / AgentResponse::FileHashes 新設
                       - PROTOCOL_VERSION は HashFiles 追加時に bump (is_last は後方互換)
    client.rs          - read_files() をストリーミングループに変更
                       - hash_files() メソッド新設
                       - check_protocol_version() を >= 比較に変更し、negotiated version を保持
    dispatch.rs        - handle_read_files() の戻り値を Vec<AgentResponse> に変更しストリーミング送信
                       - handle_hash_files() 新設
    framing.rs         - 変更なし（既存の write_frame/read_frame で対応可能）
  runtime/
    side_io.rs         - AGENT_READ_BATCH_SIZE 削除 or 大幅増加
                       - try_agent_hash_files() 新設（ローカル側ハッシュ計算含む）
                       - status 時に hash_files 優先使用
  service/
    status.rs          - hash ベース比較ロジック追加（純粋関数）
  ssh/
    batch_read.rs      - AGENT_BATCH_MAX_PATHS 定数の見直し
```

### Key Points

- **後方互換性戦略**:
  - `is_last` フィールド追加は `#[serde(default)]` で後方互換 → プロトコル bump 不要
  - `HashFiles` は新コマンドなので PROTOCOL_VERSION bump が必要
  - `check_protocol_version()` を `==` から `>=` に変更し、negotiated version を `AgentClient` に保持
  - negotiated version < 3 の場合は `hash_files` 使用不可 → `read_files` にフォールバック
- **list_tree パターンの再利用**: ストリーミングは既に list_tree で実装済み。同じパターンを read_files/hash_files に適用するだけ
- **実装順序**: 案 B（ハッシュ）→ 案 A（ストリーミング）。status の転送量削減が最大の効果を持つため、案 B を先に実装する
- **ローカルハッシュ計算**: `runtime/side_io.rs` でローカルファイルの SHA-256 を計算し、Agent から返ったリモートハッシュと比較
- **シンボリックリンク**: ハッシュ計算せず、リンクターゲットパス文字列を返す（`FileHashResult::Symlink { target }` バリアント）

## 📋 Implementation Steps

### Step 1: プロトコル拡張 + バージョンネゴシエーション
- `AgentResponse::FileContents` に `is_last: bool` フィールド追加（`#[serde(default)]` で後方互換）
- `AgentRequest::HashFiles` / `AgentResponse::FileHashes` 型を新設
- `FileHashResult` enum: `Ok { path, hash }` / `Symlink { path, target }` / `Error { path, reason }`
- `PROTOCOL_VERSION` を 3 に bump
- `check_protocol_version()` を `>=` 比較に変更し、negotiated version を返す
- **Files:** `agent/protocol.rs`, `agent/client.rs`

### Step 2: Agent ハンドラ — HashFiles
- `handle_hash_files()` を新設: 各ファイルの SHA-256 を計算してストリーミング返却
- シンボリックリンクは `Symlink { target }` バリアントで返す
- `dispatch()` メソッドで `HashFiles` リクエストをルーティング
- 戻り値は `Vec<AgentResponse>`（ストリーミング対応）
- **Files:** `agent/dispatch.rs`

### Step 3: クライアント — hash_files
- `hash_files()` メソッドを新設
- ストリーミング対応（`is_last: true` まで読み続ける）
- negotiated version < 3 の場合は `Err` を返す（呼び出し元でフォールバック）
- **Files:** `agent/client.rs`

### Step 4: ランタイム統合 — hash 優先 status
- `try_agent_hash_files()` を新設（ローカル側ハッシュ計算 + リモートハッシュ比較）
- status 時のフォールバック: hash_files → read_files → SSH
- ローカルファイルの SHA-256 計算は `sha2` crate を使用
- **Files:** `runtime/side_io.rs`

### Step 5: status サービス — hash 比較ロジック
- ハッシュベースの equal/modified 判定を純粋関数で実装
- 入力: `(local_hash: &str, remote_hash: &str)` → `CompareStatus`
- シンボリックリンク: ターゲットパス文字列の一致で判定
- **Files:** `service/status.rs`

### Step 6: Agent ハンドラ — ストリーミング ReadFiles
- `handle_read_files()` の戻り値を `Vec<AgentResponse>` に変更
- 結果を `MAX_FRAME_SIZE` 内に収まるようチャンク分割して送信
- 各チャンクに `is_last` フラグを設定
- `dispatch()` の呼び出し元も対応（既に `Vec` を `flatten` するパターンは `list_tree` で実装済み）
- **Files:** `agent/dispatch.rs`

### Step 7: クライアント — ストリーミング read_files
- `read_files()` を `list_tree()` と同様のストリーミングループに変更
- `is_last: true` まで読み続ける
- **Files:** `agent/client.rs`

### Step 8: ランタイム統合 — バッチサイズ撤廃
- `AGENT_READ_BATCH_SIZE` を `AGENT_BATCH_MAX_PATHS` (2000) に統一
- ストリーミング対応により 1 リクエストで大量ファイルを処理可能に
- **Files:** `runtime/side_io.rs`

## ✅ Tests

### Protocol Layer
- [ ] `FileContents` の `is_last` フィールドのシリアライズ/デシリアライズ
- [ ] `is_last` 未設定時に `#[serde(default)]` で `false` になること（v2 後方互換）
- [ ] `HashFiles` リクエスト/レスポンスのシリアライズ/デシリアライズ
- [ ] `FileHashResult::Symlink` バリアントのシリアライズ/デシリアライズ
- [ ] `check_protocol_version()` が `>=` 比較で v2 Agent を受け入れること

### Agent Handler (dispatch.rs)
- [ ] `handle_read_files()` がフレームサイズ内でチャンク分割すること
- [ ] `handle_read_files()` の最後のチャンクが `is_last: true` であること
- [ ] `handle_hash_files()` が正しい SHA-256 を返すこと
- [ ] `handle_hash_files()` のストリーミング分割
- [ ] `handle_hash_files()` がシンボリックリンクに `Symlink` バリアントを返すこと

### Client
- [ ] `read_files()` ストリーミングで全結果を収集できること
- [ ] `hash_files()` が正しいハッシュマップを返すこと
- [ ] negotiated version < 3 で `hash_files()` がエラーを返すこと

### Runtime
- [ ] `try_agent_hash_files()` がローカル + リモートハッシュで status を判定すること
- [ ] hash_files 未対応 Agent (v2) で read_files にフォールバックすること
- [ ] バッチサイズ拡大後もフレームサイズ超過しないこと
- [ ] ローカルファイルの SHA-256 計算が正しいこと

### Service
- [ ] ハッシュベース equal/modified 判定の純粋関数テスト
- [ ] シンボリックリンクのターゲットパス比較テスト

## 🔒 Security

- [ ] SHA-256 ハッシュ計算に標準ライブラリ (sha2 crate) を使用
- [ ] Agent プロトコルバージョン不一致時の安全なフォールバック
- [ ] 大量パス送信時のメモリ使用量上限

## 📊 Progress

| Step | Description | Status |
|------|-------------|--------|
| 1 | プロトコル拡張 + バージョンネゴシエーション | 🟢 |
| 2 | Agent ハンドラ — HashFiles | 🟢 |
| 3 | クライアント — hash_files | 🟢 |
| 4 | ランタイム統合 — hash 優先 status | ⚪ |
| 5 | status サービス — hash 比較ロジック | ⚪ |
| 6 | Agent ハンドラ — ストリーミング ReadFiles | ⚪ |
| 7 | クライアント — ストリーミング read_files | ⚪ |
| 8 | ランタイム統合 — バッチサイズ撤廃 | ⚪ |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**Next:** Write tests → Implement → Commit with `commit` 🚀
