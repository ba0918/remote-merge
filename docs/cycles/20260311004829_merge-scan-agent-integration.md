# Agent 高速化統合（TUI merge scan + CLI バッチ化 + 追加最適化）

**Cycle ID:** `20260311004829`
**Started:** 2026-03-11 00:48:29
**Status:** 🟢 Done

---

## 📝 What & Why

Agent 接続済みでも高速化が効いていない箇所が **TUI と CLI の両方** に存在する。

### 問題 1: TUI merge scan（SSH exec ~1200回）

merge scan は `std::thread::spawn` 内で **新規 SSH 接続を確立** して独立に動作するため、`CoreRuntime.agent_clients` にアクセスできない。

```
現状の merge scan フロー:
  std::thread::spawn()
    ├── tokio::runtime::Runtime::new()      ← 独自ランタイム
    ├── SshClient::connect()                ← 新規SSH接続
    ├── expand_subtree_recursive()          ← list_dir × ディレクトリ数（~100回のSSH exec）
    ├── read_all_contents()                 ← read_file × ファイル数（~1000回のSSH exec）
    └── read_ref_contents()                 ← read_file × refファイル数（~100回のSSH exec）
    合計: ~1200回の SSH exec → 5.91秒
```

### 問題 2: CLI のファイル読み込みが逐次（バッチ未使用）

CLI status/diff/merge で `fetch_contents_tolerant()` や `read_file_bytes()` を **1ファイルずつ** 呼んでおり、Agent の `read_files` バッチ API が活用されていない。Agent 接続済みでも N 回の Agent RPC（各1ファイル）が走る。

```
現状の CLI status フロー:
  fetch_contents_tolerant()
    └── for path in paths:
          core.read_file_bytes(side, path)  ← 1ファイルずつ Agent RPC or SSH exec
    ファイル100件 = 100回の RPC
```

### 問題 3: CLI merge のコンテンツ比較も逐次

`cli/merge.rs` の `needs_content_compare` ループで `read_file_bytes` を左右それぞれ個別に呼んでいる。

## 🎯 Goals

- **TUI**: merge scan で Agent バッチ API を優先使用（SSH exec ~1200回 → 3回）
- **CLI**: `fetch_contents_tolerant` をバッチ化（N 回 → 1回の Agent RPC）
- **CLI**: merge/diff の `needs_content_compare` ループをバッチ化
- Agent なしの場合は **既存の SSH exec fallback をそのまま維持**
- merge scan の所要時間を **5.91秒 → 1秒以下** に短縮（推定）
- CLI status/diff/merge の所要時間を **30-50% 短縮**（推定）
- 既存テストを壊さない

## 📐 Design

### Part A: TUI merge scan への Agent 統合

#### 方針: AgentClient を clone/take ではなく Arc<Mutex<>> で共有

~~take/return 方式~~（旧計画）は以下の問題がある:
- merge scan 中に TUI の他の操作（diff 表示等）が Agent を使えなくなる
- TransportGuard のライフサイクルと Agent の所有権が分離する

**新方針: `Arc<Mutex<BoxedAgentClient>>`**

```rust
// runtime/core.rs: 型変更
pub(crate) agent_clients: HashMap<String, Arc<Mutex<BoxedAgentClient>>>,
```

これにより:
- merge scan スレッドが `Arc::clone()` で Agent を共有できる
- メインスレッドも同時に Agent を使用可能（mutex で排他）
- TransportGuard の所有権は CoreRuntime に残り、ライフサイクル問題なし
- Agent 無効化（invalidation）も `agent_clients.remove()` で統一的に処理

**mutex contention**: merge scan のバッチ呼び出し（list_tree, read_files）は1回あたり数百ms〜1秒。この間メインスレッドの Agent アクセスはブロックされるが、TUI のイベントループは非同期なので体感影響は軽微。バッチ単位でロック→解放すれば contention は最小限。

#### task.rs の変更

```rust
fn run_merge_scan(
    tx: &mpsc::Sender<MergeScanMsg>,
    agent: Option<Arc<Mutex<BoxedAgentClient>>>,    // NEW: Arc<Mutex<>> で共有
    ref_agent: Option<Arc<Mutex<BoxedAgentClient>>>, // NEW
    local_root: &Path,
    // ... 既存引数
) -> Result<MergeScanResult, String> {
    // Agent 版: list_tree でサブツリーを一括取得
    let use_agent = agent.is_some();
    if let Some(ref ag) = agent {
        let mut guard = ag.lock().unwrap();
        match guard.list_tree(scan_root, &exclude, MAX_FILES) {
            Ok(entries) => {
                // convert_agent_entries_to_nodes() で FileNode に変換
                // サブツリーのみフィルタ（dir_path 配下）
                drop(guard); // ロック早期解放
                // ... tree 構築
            }
            Err(e) => {
                tracing::warn!("Agent list_tree failed, falling back to SSH: {}", e);
                drop(guard);
                // SSH fallback（新規接続）
            }
        }
    }
    // ... SSH fallback（Agent なし時）
}
```

#### Agent list_tree のサブツリー取得

Agent の `list_tree(root, exclude, max)` は root パスを受け付ける。merge scan 対象ディレクトリを root として渡せば、サブツリーのみを取得可能。ただし **root_dir + dir_path** を連結して Agent に渡す必要がある。

```rust
// scan_root = format!("{}/{}", remote_root.trim_end_matches('/'), dir_path)
// agent.list_tree(&scan_root, &exclude, MAX_FILES)
```

#### Agent read_files のバッチサイズ制御

Agent `read_files` は内部で chunk 分割しない（1リクエストで全ファイル）。大量ファイル時のメモリ圧を考慮し、**256ファイルずつ** バッチ分割する。

```rust
const AGENT_READ_BATCH_SIZE: usize = 256;

fn agent_read_all_contents(
    agent: &Arc<Mutex<BoxedAgentClient>>,
    file_paths: &[String],
    // ...
) -> Result<(), String> {
    for chunk in file_paths.chunks(AGENT_READ_BATCH_SIZE) {
        let mut guard = agent.lock().unwrap();
        let results = guard.read_files(&full_paths, 0)?;
        drop(guard); // ロック早期解放
        // ... 結果を cache に格納
    }
}
```

#### Agent エラー時の invalidation

task.rs はスレッド内で実行されるため `self.invalidate_agent()` を呼べない。代わりに:
- Agent プロトコルエラー時は `agent` を `None` に設定してスレッド内で SSH fallback
- スレッド完了後、MergeScanMsg で `agent_failed: bool` を返し、メインスレッドで `invalidate_agent()` を呼ぶ

```rust
pub enum MergeScanMsg {
    Progress { files_found: usize, current_path: Option<String> },
    ContentPhase { total: usize },
    Done(Box<MergeScanResult>),
    Error(String),
    AgentFailed { server_name: String }, // NEW: Agent 無効化通知
}
```

### Part B: CLI バッチ化

#### fetch_contents_tolerant のバッチ化

`cli/tolerant_io.rs` の `fetch_contents_tolerant()` を **バッチ API** を使うように変更。

```rust
pub fn fetch_contents_tolerant(
    side: &Side,
    paths: &[String],
    core: &mut CoreRuntime,
) -> HashMap<String, Vec<u8>> {
    // バッチバイト列読み込みを試行
    if let Ok(batch) = core.read_files_bytes_batch(side, paths) {
        return batch;
    }
    // フォールバック: 1ファイルずつ（既存ロジック）
    let mut contents = HashMap::new();
    for path in paths {
        match core.read_file_bytes(side, path, false) {
            Ok(content) => { contents.insert(path.clone(), content); }
            Err(e) => { tracing::debug!("Failed to read {}: {}", path, e); }
        }
    }
    contents
}
```

#### CoreRuntime に read_files_bytes_batch を追加

`side_io.rs` に新メソッド追加。Agent の `read_files` はバイト列を返すため、バッチバイト読み込みに最適。

```rust
/// Side に基づいて複数ファイルのバイト列をバッチ読み込みする（エラートレラント）
pub fn read_files_bytes_batch(
    &mut self,
    side: &Side,
    rel_paths: &[String],
) -> anyhow::Result<HashMap<String, Vec<u8>>> {
    match side {
        Side::Local => {
            let mut result = HashMap::with_capacity(rel_paths.len());
            for rel_path in rel_paths {
                match executor::read_local_file_bytes(&self.config.local.root_dir, rel_path, false) {
                    Ok(bytes) => { result.insert(rel_path.clone(), bytes); }
                    Err(_) => {} // トレラント: スキップ
                }
            }
            Ok(result)
        }
        Side::Remote(name) => {
            if let Some(batch) = self.try_agent_read_files_bytes_batch(name, rel_paths) {
                return batch;
            }
            // SSH fallback: 1ファイルずつ
            let mut result = HashMap::with_capacity(rel_paths.len());
            for rel_path in rel_paths {
                if let Ok(bytes) = self.read_remote_file_bytes(name, rel_path, false) {
                    result.insert(rel_path.clone(), bytes);
                }
            }
            Ok(result)
        }
    }
}
```

#### CLI merge/diff の needs_content_compare バッチ化

`cli/merge.rs` と `cli/diff.rs` の `needs_content_compare` ループを、`read_files_bytes_batch` で一括取得に変更。

```rust
// Before (merge.rs L99-127):
for path in &paths_to_compare {
    let left_bytes = core.read_file_bytes(&pair.left, path, false)...;
    let right_bytes = core.read_file_bytes(&pair.right, path, false)...;
    compare_pairs.insert(path.clone(), (left_bytes, right_bytes));
}

// After:
let left_batch = core.read_files_bytes_batch(&pair.left, &paths_to_compare)?;
let right_batch = core.read_files_bytes_batch(&pair.right, &paths_to_compare)?;
for path in &paths_to_compare {
    let left_bytes = left_batch.get(path).cloned().unwrap_or_default();
    let right_bytes = right_batch.get(path).cloned().unwrap_or_default();
    compare_pairs.insert(path.clone(), (left_bytes, right_bytes));
}
```

### Part C: 追加の高速化

#### C-1: Agent stat_files で mtime 一括取得（quick check 高速化）

CLI status の `needs_content_compare()` はメタデータ（size, mtime）で事前フィルタする。現在はツリー取得時のメタデータを使うが、Agent の `stat_files()` バッチ API で **マージ前の最新 mtime を一括取得** すれば、コンテンツ比較対象をさらに絞り込める（false positive 削減）。

→ ただし現状 `fetch_tree_recursive` 時のメタデータで十分な精度があるため、**Phase 2 で検討**（今回はスコープ外）。

#### C-2: SSH 接続確立の省略（Agent あり時）

merge scan で Agent がある場合、SSH 接続確立（`SshClient::connect()`）を完全にスキップ。Agent fallback 時のみ SSH 接続を行う。

### Files to Change

```
Part A: TUI merge scan
  src/runtime/core.rs
    - agent_clients の型を HashMap<String, Arc<Mutex<BoxedAgentClient>>> に変更
    - try_start_agent() で Arc::new(Mutex::new()) でラップ
    - has_agent(), invalidate_agent() を Arc<Mutex<>> 対応に
  src/runtime/side_io.rs
    - try_agent_* メソッド群を Arc<Mutex<>> 対応に（.lock().unwrap()）
  src/runtime/merge_scan/mod.rs
    - start_merge_scan(): Arc::clone() で Agent をスレッドに渡す
    - MergeScanMsg に AgentFailed variant 追加
    - poll_merge_scan(): AgentFailed 受信時に invalidate_agent() 呼出
  src/runtime/merge_scan/task.rs
    - run_merge_scan(): agent/ref_agent 引数追加（Arc<Mutex<>>）
    - agent_list_tree(): Agent 版サブツリー取得（新設）
    - agent_read_all_contents(): Agent 版バッチ読み込み（新設、256ファイルずつ）
    - agent_read_ref_contents(): Agent 版 ref バッチ読み込み（新設）
    - SSH 接続は Agent なし or Agent fallback 時のみ確立
  src/app/types.rs
    - MergeScanMsg enum に AgentFailed variant 追加
  src/agent/client.rs
    - BoxedAgentClient の Send 確認（static assert）

Part B: CLI バッチ化
  src/runtime/side_io.rs
    - read_files_bytes_batch() 新設
    - try_agent_read_files_bytes_batch() 新設
  src/cli/tolerant_io.rs
    - fetch_contents_tolerant() を read_files_bytes_batch() ベースに変更
  src/cli/merge.rs
    - needs_content_compare ループをバッチ化
  src/cli/diff.rs
    - needs_content_compare ループをバッチ化
```

### Key Design Decisions

- **Arc<Mutex<>> 方式**: take/return よりも安全。merge scan 中も TUI の他の操作が Agent を使える。TransportGuard のライフサイクル問題を回避。contention は軽微（バッチ単位でロック→解放）
- **Agent 失敗時は SSH fallback**: Agent の list_tree/read_files が失敗したら、そのフェーズ全体を SSH fallback に切り替え。部分的な Agent 使用はしない（複雑さ回避）
- **Agent invalidation は MergeScanMsg 経由**: スレッド内で CoreRuntime にアクセスできないため、メインスレッドに通知して invalidation を行う
- **256ファイルずつバッチ分割**: Agent read_files の1リクエストで 256 ファイルまで。メモリ圧と応答時間のバランス
- **CLI バッチ化は tolerant**: バッチ全体が失敗したら1ファイルずつフォールバック。ファイル単位のエラーはスキップ
- **SSH 接続は Agent なし時のみ確立**: Agent がある場合は SshClient::connect() をスキップ（不要な接続を避ける）
- **Agent list_tree のサブツリー取得**: scan_root を `remote_root/dir_path` に設定して対象ディレクトリ配下のみ取得
- **side_io.rs の convert_agent_entries_to_nodes() を再利用**: Agent レスポンスの変換ロジックは既存を流用
- **`&mut self` を維持する理由**: side_io.rs の try_agent_* メソッドは `&mut self` を取る。Arc<Mutex<>> 導入後も `get()` + `.lock()` で読み込みアクセスは `&self` で可能だが、`invalidate_agent()` が `agent_clients.remove()` を呼ぶため `&mut self` が必要。try_agent_* 内でエラー時に invalidation するフローのため、`&mut self` を維持する
- **SSH fallback のバッチ化はスコープ外**: Agent 未接続環境での SSH バッチ化（tar -c | base64 等）は効果が限定的（Agent 接続時は不要）。Phase 2 で必要に応じて検討する

### side_io.rs の Arc<Mutex<>> 変更方針

全 `try_agent_*` メソッド（約10箇所）で以下のパターンに変更:

```rust
// Before:
let agent = self.agent_clients.get_mut(server_name)?;
agent.read_files(...)

// After:
let agent_arc = self.agent_clients.get(server_name)?;
let mut agent = agent_arc.lock().unwrap();
agent.read_files(...)
```

変更は機械的であり、ロジックの変更は不要。`.lock().unwrap()` による panic リスクは以下の理由で極めて低い:
1. Agent 操作は Result ベースで panic しない設計
2. merge scan スレッドが panic しても `catch_unwind` しないため、poison は発生しうるが、poison 時は Agent を invalidate して SSH fallback すれば安全
3. poison 対策として `.lock().unwrap_or_else(|e| e.into_inner())` を使う選択肢もあるが、poison 状態の Agent は不整合の可能性があるため、panic（= invalidation 相当）の方が安全

### レイヤー分析

| レイヤー | ファイル | 責務 |
|---------|---------|------|
| ドメイン | `merge_scan/task.rs` | Agent/SSH の切り替え判定、ツリー変換、バッチ読み込み |
| ドメイン | `cli/tolerant_io.rs` | エラートレラントなバッチ読み込み |
| サービス | `merge_scan/mod.rs` | Agent 共有、スレッド管理、メッセージ定義 |
| サービス | `side_io.rs` | バッチバイト読み込み API |
| インフラ | `runtime/core.rs` | Agent 型変更（Arc<Mutex<>>） |
| CLI | `cli/merge.rs`, `cli/diff.rs` | バッチ化呼び出し |

## ✅ Tests

### Part A: TUI merge scan

#### ドメイン層（task.rs）
- [ ] `agent_list_tree_converts_entries`: Agent の list_tree レスポンスがファイルツリーに正しく変換される
- [ ] `agent_list_tree_subtree_only`: scan_root 配下のエントリのみが返される
- [ ] `agent_read_all_contents_batch`: Agent の read_files でファイル内容がバッチ取得される
- [ ] `agent_read_batch_chunking`: 256ファイル超で複数バッチに分割される
- [ ] `agent_fallback_on_list_tree_error`: list_tree 失敗時に SSH fallback に切り替わる
- [ ] `agent_fallback_on_read_files_error`: read_files 失敗時に SSH fallback に切り替わる

#### サービス層（mod.rs）
- [ ] `agent_shared_via_arc_mutex`: Agent が Arc::clone() でスレッドに渡される
- [ ] `agent_failed_triggers_invalidation`: AgentFailed メッセージで invalidate_agent() が呼ばれる

#### インフラ層（core.rs）
- [ ] `boxed_agent_client_is_send`: BoxedAgentClient が Send を満たすことの static assert
- [ ] `arc_mutex_agent_accessible`: Arc<Mutex<BoxedAgentClient>> が複数箇所からアクセス可能

### Part B: CLI バッチ化

#### side_io.rs
- [ ] `read_files_bytes_batch_local`: ローカルファイルのバッチバイト読み込み
- [ ] `read_files_bytes_batch_tolerant`: 一部ファイルの読み込み失敗がスキップされる
- [ ] `read_files_bytes_batch_empty`: 空パスリストで空結果

#### tolerant_io.rs
- [ ] `fetch_contents_tolerant_uses_batch`: バッチ API 経由で取得される
- [ ] `fetch_contents_tolerant_fallback`: バッチ失敗時に1ファイルずつフォールバック

### 既存テスト
- [ ] 全既存テスト通過（Agent なし環境で動作変更なし）

## 🔒 Security

- [ ] Arc<Mutex<>> で deadlock の可能性がないか（lock 順序が一方向であること）
- [ ] Agent 失敗時の SSH fallback で既存のセキュリティチェックが維持されるか
- [ ] バッチ読み込みで sensitive ファイルが意図せず読まれないか（force フラグの伝播）

## 📊 Implementation Steps

| Step | 内容 | 影響ファイル | Status |
|------|------|-------------|--------|
| 0 | BoxedAgentClient の Send 確認（compile test） | core.rs | 🟢 |
| 1 | agent_clients の型を Arc<Mutex<>> に変更 + 全 try_agent_* を対応 | core.rs, side_io.rs | 🟢 |
| 2 | read_files_bytes_batch + try_agent_read_files_bytes_batch 新設 | side_io.rs | 🟢 |
| 3 | fetch_contents_tolerant をバッチ化 | tolerant_io.rs | 🟢 |
| 4 | CLI merge/diff の needs_content_compare ループをバッチ化 | merge.rs, diff.rs | 🟢 |
| 5 | MergeScanMsg に AgentFailed 追加 + poll で invalidation | types.rs, mod.rs, poll.rs | 🟢 |
| 6 | start_merge_scan で Arc::clone() で Agent をスレッドに渡す | mod.rs | 🟢 |
| 7 | task.rs に agent_list_tree（サブツリー取得、scan_root 指定） | task.rs | 🟢 |
| 8 | task.rs に agent_read_files_batch（共通ヘルパー、256ファイルずつ） | task.rs | 🟢 |
| 9 | task.rs: ref コンテンツも共通ヘルパーで読み込み | task.rs | 🟢 |
| 10 | Agent なし時のみ SSH 接続確立（最適化） | task.rs | 🟢 |
| 11 | テスト + clippy + fmt 確認 | - | 🟢 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

**実装順序の理由:**
- Step 0-1: 基盤変更（Arc<Mutex<>>）を先にやることで、後続の Part A/B が自然に書ける
- Step 2-4: CLI バッチ化は基盤変更後すぐに着手可能（merge scan より影響範囲が小さい）
- Step 5-10: TUI merge scan は最後（影響範囲が大きいため、CLI が安定してから）

---

**Next:** テスト書いて → 実装 → コミット
