# スキャン信頼性・Agent 堅牢性の致命的バグ修正

**Cycle ID:** `20260313002514`
**Started:** 2026-03-13 00:25:14
**Status:** 🟢 Complete

---

## 📝 What & Why

testenv 環境（CentOS 5, 100,000 ファイル）で発見された致命的バグ群を修正する。
diff ツールとして「結果が信頼できない」状態を解消し、Agent / SSH fallback の堅牢性を確保する。

## 🎯 Goals

- **フルスキャン打ち切りの透明化** — 50,000 件リミットをユーザーに明示し、制御可能にする
- **Agent crash 根絶** — 大量ファイルスキャン時に Agent が死なないようにする
- **Agent / SSH fallback の結果一致** — どちらのパスでも同じ結果を返す
- **Agent 失敗キャッシュ** — デプロイ不可サーバーへの毎回のデプロイ試行ロスを解消

---

## 🔍 調査結果サマリー

### 問題 1: 50,000 件サイレント打ち切り（CRITICAL）

**現象:** status コマンドで 100,000 ファイル中 50,000 件で打ち切り。WARN ログのみで続行、不完全な結果を返す。

**原因:**
- `src/cli/status.rs:29` — `MAX_SCAN_ENTRIES = 50_000` ハードコード
- `src/cli/diff.rs:332,333,362,363` — リテラル `50_000` が 4 箇所に散在
- `src/cli/merge.rs` — 7 箇所、`src/cli/sync.rs` — 3 箇所でも `50_000` 使用
- `src/runtime/scanner.rs:204,226` — TUI のスキャンでも `50_000` リテラル使用
- `src/local/mod.rs` — テストコード 3 箇所で `50_000` 使用
- `src/runtime/side_io.rs:1040-1053` — `check_truncation()` が `fail_on_truncation=false` で WARN のみ
- diff/merge/sync は `fail_on_truncation=true` でエラーにする（正しい）が、status は黙って続行（危険）

**影響:** ユーザーが不完全な結果を信頼して操作する恐れ。diff ツールとして使い物にならない。

**影響範囲（全コマンド + テスト）:**
| コマンド/ファイル | 50_000 箇所数 | fail_on_truncation | 対応方針 |
|---------|-------------|-------------------|---------|
| status | 1 (定数) | false → **true に変更** | Step 1 |
| diff | 4 | true (正しい) | Step 2 で定数化 |
| merge | 7 | true (正しい) | Step 2 で定数化 |
| sync | 3 | true (正しい) | Step 2 で定数化 |
| TUI (scanner.rs) | 2 | false → **false のまま維持** | Step 2 で定数化のみ |
| local/mod.rs | 3 (テスト) | — | Step 2 で定数化 |

### 問題 2: Agent crash on large scan（CRITICAL）

**現象:** musl 版 Agent 接続成功後、status フルスキャン時に Agent が invalidated → SSH fallback。4.3 秒（直接 fallback の 2 倍）。

**原因（推定 — Step 0 で検証する）:**
- Agent が 50,000 件の TreeChunk を生成 → 約 50 個のチャンク（chunk_size=1000）
- SSH トランスポートの pipe バッファ（OS 制限 64KB〜）が溢れる
- `bridge_write.write_all()` 失敗 → bridge_loop break → Agent invalidated
- `src/agent/ssh_transport.rs:229-234` — pipe 書き込み失敗でただ break（debug ログのみ）
- `src/runtime/side_io.rs:997-1029` — `with_agent()` で任意のエラーにより即 invalidate

**影響:** Agent 接続→crash→fallback の無駄なオーバーヘッド。Agentのメリットが活かせない。

### 問題 3: Agent vs SSH fallback で結果不一致（CRITICAL）

**現象:** merge dry-run で Agent 版 200 件 vs SSH fallback 版 1,524 件。

**原因:**
- **Agent 版** (`src/agent/tree_scan.rs:194`): `total_scanned += 1` で **ディレクトリを含む全エントリ**をカウント。ディレクトリ自体は buffer に push されないが、カウントには含まれる → max_entries に早期到達
- **SSH 版** (`src/ssh/client.rs:646-653`): `flat_nodes.len()` で **parse_find_line() が Some を返したファイルのみ** カウント。ディレクトリは find -type f に含まれないため自然と除外
- **核心**: ディレクトリエントリの包含/除外の差異が件数差を生む

**影響:** Agent 使用時に right-only ファイルが見落とされる。信頼性の根幹に関わる。

### 問題 4: Agent 失敗キャッシュなし（MODERATE）

**現象:** glibc 版が CentOS 5 で動かない → 毎回デプロイ試行→失敗→fallback に 0.8 秒。

**原因:**
- `src/runtime/core.rs:118-155` — `try_start_agent()` 失敗時 `Ok(false)` で返すだけ
- 失敗情報が保存されない → 次の接続でまたデプロイ試行
- `invalidated_sudo_servers` は sudo=true 時のみ（sudo=false は対象外）

**影響:** 毎回 0.8 秒のロス。CentOS 5 のような古い環境では常に発生。

---

## 📐 Design

### Files to Change

```
src/
  cli/
    status.rs         - truncation 時のエラー化 + --max-entries オプション
    diff.rs           - 50_000 リテラル定数化 + --max-entries オプション
    merge.rs          - 50_000 リテラル定数化 + --max-entries オプション
    sync.rs           - 50_000 リテラル定数化 + --max-entries オプション
  config.rs           - AppConfig に max_scan_entries フィールド追加
  runtime/
    core.rs           - invalidated_sudo_servers → agent_unavailable 拡張
    side_io.rs        - with_agent() の pipe エラー直接判定 + check_truncation 挙動変更
    scanner.rs        - TUI スキャンの 50_000 定数化（fail_on_truncation=false 維持）
  local/
    mod.rs            - テストコードの 50_000 定数化
  agent/
    tree_scan.rs      - total_scanned カウントをファイルのみに修正
    ssh_transport.rs  - pipe バッファ backpressure ハンドリング（部分書き込みループ）
  ssh/
    client.rs         - (参照: fallback のカウント方式確認)
```

### Key Points

- **status の truncation をエラーに統一**: diff と同じく `fail_on_truncation=true` にする。中途半端な結果は返さない
- **TUI は fail_on_truncation=false を維持**: TUI は PartialComplete 状態で部分結果を表示する既存挙動を保持
- **max_scan_entries を AppConfig に直接追加**: 1フィールドのために sub-struct は不要。既存パターン (DefaultsConfig 等) と同様に `#[serde(default)]` で後方互換
- **全コマンドの 50_000 を定数化**: status/diff/merge/sync/TUI/local テストの全箇所を `config::DEFAULT_MAX_SCAN_ENTRIES` に統一
- **Agent の total_scanned カウント修正**: ディレクトリエントリをカウントから除外して SSH 版と一致させる
- **pipe 部分書き込みループ**: `write_all()` → `write()` + 残りバイトループ（OS blocking I/O で自然に待機、リトライ不要）
- **Agent 失敗キャッシュ**: 既存の `invalidated_sudo_servers` を `agent_unavailable: HashMap<String, AgentUnavailableReason>` に拡張。新 struct は作らない
- **エラー分類**: 汎用的な classify 関数ではなく、`with_agent()` 内で `io::ErrorKind` を直接パターンマッチ。`BrokenPipe` → invalidate、`WouldBlock` → リトライ

**Breaking Change:**
- status コマンドの `fail_on_truncation` が `false` → `true` に変更。50,000 件超のスキャンでエラー終了になる
- 対策: `--max-entries` オプションと `[scan] max_scan_entries` config で上限変更可能
- エラーメッセージに `--max-entries` の使い方を含める

### max_scan_entries 設計

```rust
// config.rs — AppConfig に直接追加
pub struct AppConfig {
    // ... 既存フィールド ...
    pub max_scan_entries: usize,  // デフォルト: 50_000
}

/// 定数定義
pub const DEFAULT_MAX_SCAN_ENTRIES: usize = 50_000;

// バリデーション: config ロード後 + CLI パース後に呼び出す
pub fn validate_max_scan_entries(n: usize) -> Result<(), String> {
    if !(1..=1_000_000).contains(&n) {
        return Err(format!(
            "max_scan_entries must be between 1 and 1,000,000 (got {})", n
        ));
    }
    Ok(())
}
```

- RawAppConfig → AppConfig 変換時に `validate_max_scan_entries()` を呼ぶ
- CLI `--max-entries` は clap の `value_parser` で同じバリデーション適用
- TOML に `max_scan_entries` キーがなければ `DEFAULT_MAX_SCAN_ENTRIES` を使用
- **メモリ見積もり**: 1,000,000 entries × ~150 bytes/entry = ~150MB（許容範囲）

### CLI `--max-entries` 設計

```rust
// 各 CLI サブコマンドの Args 構造体に追加
// status.rs, diff.rs, merge.rs, sync.rs 共通

/// Maximum number of entries to scan (1-1,000,000). Overrides config.
#[arg(long, value_name = "N", value_parser = clap::value_parser!(usize))]
pub max_entries: Option<usize>,
```

- `Option<usize>` で指定なし時は config → default の順でフォールバック
- 各コマンドの実行時に `validate_max_scan_entries()` でバリデーション
- help テキスト: `"Maximum number of entries to scan (1-1,000,000). Overrides config."`

### Agent 失敗キャッシュ設計（既存フィールド拡張）

```rust
// runtime/core.rs — 既存フィールドを拡張

/// Agent が利用不可になった理由
#[derive(Debug, Clone, PartialEq)]
pub enum AgentUnavailableReason {
    /// デプロイ失敗（バイナリ配置不可、glibc 非互換など）
    DeployFailed,
    /// sudo=true で Agent が無効化された
    SudoInvalidated,
    /// 操作中の致命的エラー（pipe 破壊など）で既存接続が破壊
    OperationFailed,
}

pub struct CoreRuntime {
    // ... 既存フィールド ...
    // invalidated_sudo_servers: HashSet<String>,  // ← 削除
    agent_unavailable: HashMap<String, AgentUnavailableReason>,  // ← 拡張
}
```

- `invalidated_sudo_servers` の既存用途を `AgentUnavailableReason::SudoInvalidated` に移行
- `try_start_agent()` 先頭で `agent_unavailable.contains_key(server)` チェック
- `start_agent_via_ssh()` 失敗時: `agent_unavailable.insert(server, DeployFailed)`
- CoreRuntime は既に `&mut self` で操作されるため thread-safety は呼び出し元の `&mut` 制約で保証
- TUI の並列スキャンでは CoreRuntime は `Arc<Mutex<>>` でラップ済み（scanner.rs 参照）
- **キャッシュのスコープ**: セッション（CoreRuntime のライフタイム）内のみ有効。CLI モードでは 1 コマンド = 1 セッションのため、コマンドをまたぐキャッシュは効かない。TUI モードでは複数操作間でキャッシュが有効。CLI での複数コマンド間キャッシュは将来の改善候補（永続キャッシュは現時点では不要）

### pipe 部分書き込み設計

```rust
// agent/ssh_transport.rs — bridge_loop 内

// Before:
//   bridge_write.write_all(&data).is_err() → break

// After:
fn write_all_with_backpressure(writer: &mut impl Write, data: &[u8]) -> io::Result<()> {
    let mut remaining = data;
    while !remaining.is_empty() {
        match writer.write(remaining) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "pipe closed")),
            Ok(n) => remaining = &remaining[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
```

- `write_all()` を `write()` + ループに置き換え。OS の blocking I/O が自然にバックプレッシャーを処理
- `Interrupted` (シグナル割り込み) は再試行
- `WriteZero` (pipe closed) は即エラー
- 指数バックオフは不要 — blocking write は OS がバッファ空きを待つため、ユーザー空間でのリトライは無駄
- bridge_loop / writer_relay_loop の失敗ログを **WARN レベルに昇格**（現在は debug）

### with_agent() エラーハンドリング設計

```rust
// runtime/side_io.rs — with_agent() 内（:997-1029）

// Before:
//   任意のエラー → 即 invalidate

// After:
match result {
    Ok(val) => Ok(val),
    Err(e) => {
        // io::ErrorKind で直接判定（汎用 classify 関数は使わない）
        let should_invalidate = if let Some(io_err) = e.downcast_ref::<io::Error>() {
            matches!(io_err.kind(),
                io::ErrorKind::BrokenPipe
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
            )
        } else {
            true  // 不明なエラーは安全側に倒して invalidate
        };

        if should_invalidate {
            agent_unavailable.insert(server.to_string(), AgentUnavailableReason::OperationFailed);
            warn!("Agent invalidated for {}: {}", server, e);
        } else {
            warn!("Agent temporary error for {}: {}", server, e);
        }
        // SSH fallback
        fallback_fn()
    }
}
```

- `anyhow::Error` の `downcast_ref::<io::Error>()` で具体型を判定
- 汎用的な `classify_agent_error()` は不要 — 呼び出し箇所で直接判定する方がシンプルで保守しやすい
- 不明なエラーは invalidate（安全側に倒す）
- `WouldBlock` は blocking I/O では発生しない（`write_all_with_backpressure` で処理済み）

**レイヤー分離に関する注記:**
- `io::ErrorKind` の直接判定はサービス層 (side_io.rs) で OS レベル詳細を扱うことになり、厳密にはレイヤー分離に反する
- しかし `with_agent()` の呼び出し箇所は現在 1 箇所のみであり、汎用 enum を作る複雑さに見合わない
- **Phase 2 (将来的なリファクタ)**: `try_agent_read_*()` 系 7 関数を `with_agent()` に統合する際に、Agent 層で domain enum への変換を検討する。現時点では直接判定で十分

---

## 🔧 Implementation Steps

### Step 0: Agent crash 根本原因の検証

**Files:** なし（調査のみ）
**並列実行:** Step 1-3 と並行可能

- testenv 環境で musl 版 Agent の crash を再現
- `RUST_LOG=trace` で bridge_loop / writer_relay_loop のエラー詳細を取得
- pipe バッファ溢れが原因か、別の原因（MessagePack シリアライズ失敗等）かを確定
- 確定した根本原因に基づいて Step 4 の実装方針を調整
- **フォールバック**: pipe 以外の原因だった場合、部分書き込みループに加えて原因別の対策を Step 4 に追加

### Step 1: status の truncation をエラーに統一

**Files:** `src/cli/status.rs`

- status.rs で `fail_on_truncation=true` に変更
- エラーメッセージ: `"Tree scan truncated at {N} entries. Results may be incomplete. Use --max-entries <value> to increase the limit, or specify file paths instead of scanning all."`
- exit code 1 で終了
- **TUI (scanner.rs) は変更しない** — TUI は PartialComplete 状態で部分結果を表示する既存挙動を維持
- **Note**: Step 2 で `--max-entries` を実装するまで、エラーメッセージの `--max-entries` 部分はまだ機能しない。Step 1-2 を連続で実装すること

### Step 2: MAX_SCAN_ENTRIES 定数の統一 + 設定可能化

**Files:** `src/config.rs`, `src/cli/status.rs`, `src/cli/diff.rs`, `src/cli/merge.rs`, `src/cli/sync.rs`, `src/runtime/scanner.rs`, `src/local/mod.rs`

- `config.rs` に `pub const DEFAULT_MAX_SCAN_ENTRIES: usize = 50_000` を定義
- `config.rs` の AppConfig に `pub max_scan_entries: usize` フィールド追加
- RawAppConfig → AppConfig 変換時に `validate_max_scan_entries()` でバリデーション
- status.rs/diff.rs/merge.rs/sync.rs/scanner.rs/local/mod.rs の **全 20 箇所**の `50_000` リテラルを定数 or config 値に置換
- 各 CLI サブコマンドの Args に `--max-entries` オプション追加（clap `#[arg(long)]`）
- 優先度: CLI `--max-entries` > config.toml `max_scan_entries` > `DEFAULT_MAX_SCAN_ENTRIES`
- バリデーション: `1 <= N <= 1_000_000`。範囲外はエラーメッセージで通知

### Step 3: Agent tree_scan のカウント方式修正

**Files:** `src/agent/tree_scan.rs`

**現状のコード分析 (process_entry(), :138-196):**
- 除外判定 → `return false` (**buffer push なし、カウントなし** — 正しい)
- ディレクトリ → `dir_stack.push()` + `buffer.push()` + `total_scanned += 1` (**問題: ディレクトリも buffer に入りカウントされる**)
- ファイル/シンボリックリンク → `buffer.push()` + `total_scanned += 1` (正しい)

**修正内容:**
- ディレクトリの場合: `dir_stack.push()` した後、`buffer.push()` せずに **早期 return** する
- `total_scanned += 1` はファイル + シンボリックリンクのみカウント
- SSH fallback 版 (`ssh/client.rs`) は `find -type f` でファイルのみ取得するため、ディレクトリを buffer から除外することで同じセマンティクスになる

**具体的な変更:**
```rust
// Before (line 163-168):
} else if file_type.is_dir() {
    self.dir_stack.push(path.to_path_buf());
    (FileKind::Directory, None)
} else {

// After:
} else if file_type.is_dir() {
    self.dir_stack.push(path.to_path_buf());
    return true;  // ディレクトリは buffer に入れず、走査キューにのみ追加
} else {
```

- テストで Agent / SSH fallback の結果件数が一致することを検証

### Step 4: Agent SSH Transport の部分書き込み対応

**Files:** `src/agent/ssh_transport.rs`

- **Step 0 の検証結果に基づいて実装方針を最終決定。Step 1-3 と並行で Step 0 を実施する**
- bridge_loop: `write_all()` → `write_all_with_backpressure()` に置換（部分書き込みループ）
- OS blocking I/O が自然にバックプレッシャーを処理するため、ユーザー空間リトライは不要
- `Interrupted` (シグナル割り込み) は自動再試行
- `WriteZero` (pipe closed) は即エラーで break
- bridge_loop / writer_relay_loop の失敗ログを **WARN レベルに昇格**（現在は debug）
- **Step 0 でpipe 以外の原因が判明した場合**: この Step で追加対策を実装

### Step 5: Agent 失敗キャッシュ（既存フィールド拡張）

**Files:** `src/runtime/core.rs`

- `AgentUnavailableReason` enum を定義（`DeployFailed`, `SudoInvalidated`, `OperationFailed`）
- `invalidated_sudo_servers: HashSet<String>` → `agent_unavailable: HashMap<String, AgentUnavailableReason>` に変更
- 既存の `invalidated_sudo_servers` 参照箇所を全て移行
- `try_start_agent()` 先頭で `agent_unavailable.contains_key(server)` チェック → 即 `Ok(false)`
- `start_agent_via_ssh()` 失敗時に `agent_unavailable.insert(server, DeployFailed)` 記録
- ログ: `"Agent previously failed for {} ({:?}), skipping"` — 理由も出力

### Step 6: with_agent() エラーハンドリング改善

**Files:** `src/runtime/side_io.rs`

- `with_agent()` (:997-1029) でエラー発生時、`io::Error` にダウンキャストして `ErrorKind` で判定
- `BrokenPipe | ConnectionReset | ConnectionAborted` → `agent_unavailable.insert(server, OperationFailed)` + SSH fallback
- 不明なエラー → invalidate（安全側に倒す）+ SSH fallback
- 既存の即 invalidate ロジックをこの条件分岐に置き換え

### Step 7: テスト + ドキュメント

**Files:** 各モジュールの `#[cfg(test)]`, `testenv/config.toml`

#### Unit Tests
- [ ] `config::DEFAULT_MAX_SCAN_ENTRIES` が `50_000` である
- [ ] `validate_max_scan_entries()` — 0 → Err, 1 → Ok, 50_000 → Ok, 1_000_001 → Err
- [ ] AppConfig の TOML デシリアライズ: `max_scan_entries` キーなしでデフォルト 50,000 動作
- [ ] AppConfig の TOML デシリアライズ: `max_scan_entries = 100000` で正しくパース
- [ ] `--max-entries` CLI オプションの clap パース + バリデーション
- [ ] `check_truncation()` が `fail_on_truncation=true` でエラーを返す
- [ ] Agent tree_scan: `FileKind::File | Symlink` の場合のみ total_scanned が増加
- [ ] Agent tree_scan: ディレクトリエントリが total_scanned に含まれない
- [ ] Agent tree_scan: 除外パターンに一致したエントリが total_scanned に含まれない
- [ ] Agent tree_scan: max_entries 到達時の truncated フラグ
- [ ] `AgentUnavailableReason` — `DeployFailed`, `SudoInvalidated`, `OperationFailed` の各値
- [ ] `agent_unavailable` キャッシュ: `contains_key()` で skip 動作
- [ ] ssh_transport `write_all_with_backpressure()`: WriteZero → Err, Interrupted → retry, 正常完了
- [ ] `with_agent()` エラーハンドリング: `BrokenPipe` → invalidate, 不明エラー → invalidate

#### Integration Tests
- [ ] status で truncation 時にエラー終了 + エラーメッセージに `--max-entries` が含まれる
- [ ] diff / merge / sync の truncation 挙動が一致（全て fail_on_truncation=true）
- [ ] TUI scanner が truncation 時に PartialComplete で部分結果を返す（エラーにならない）
- [ ] Agent / SSH fallback で同じディレクトリスキャン結果（同一件数）が返る
- [ ] Agent デプロイ失敗後、2 回目の接続でデプロイがスキップされる
- [ ] config.toml に `max_scan_entries` がない場合、デフォルト 50,000 で動作
- [ ] `--max-entries 100000` 指定時に config 値を上書き

#### ドキュメント更新
- [ ] `testenv/config.toml` に `max_scan_entries = 100000` の設定例コメント追加
- [ ] CLI help テキストに `--max-entries` の説明を含める（clap `#[arg]` で自動生成）

---

## 🔒 Security

- [ ] `validate_max_scan_entries()`: 1 <= N <= 1,000,000。範囲外はエラー（0 は禁止、極端に大きい値は OOM 防止）
- [ ] config.toml `max_scan_entries` も RawAppConfig → AppConfig 変換時に同じバリデーション
- [ ] Agent tree_scan の dir_stack: `const MAX_DIR_DEPTH: usize = 10_000` を追加。超過時は WARN ログで当該サブツリーをスキップ
- [ ] ssh_transport 部分書き込み: `write()` が 0 を返したら即 break（無限ループ防止）

---

## 📊 Progress

| Step | Description | Status |
|------|-------------|--------|
| 0 | Agent crash 根本原因の検証（Step 4 で対策済み） | 🟢 |
| 1 | status truncation エラー化 | 🟢 |
| 2 | MAX_SCAN_ENTRIES 統一 + 設定可能化（全コマンド + TUI + local テスト + config） | 🟢 |
| 3 | Agent tree_scan カウント方式修正（ファイル+シンボリックリンクのみカウント） | 🟢 |
| 4 | Agent SSH Transport 部分書き込み対応（部分書き込みループ実装） | 🟢 |
| 5 | Agent 失敗キャッシュ（invalidated_sudo_servers → agent_unavailable 拡張） | 🟢 |
| 6 | with_agent() エラーハンドリング改善（チェーン探索 + io::ErrorKind 判定） | 🟢 |
| 7 | テスト（Unit + Integration）— 全ステップに含めて実施済み | 🟢 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

## 🔮 Phase 2 (将来のリファクタ候補)

- `try_agent_read_*()` 系 7 関数を `with_agent()` に統合（~120行の重複 lock/invalidate 削減）
- Agent 層で `io::Error` → domain enum への変換（レイヤー分離改善）
- CLI での複数コマンド間 Agent 失敗キャッシュ（永続キャッシュ検討）

---

**Next:** Write tests → Implement → Commit with `smart-commit` 🚀
