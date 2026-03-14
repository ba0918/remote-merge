# ディレクトリ展開時の非同期バッジスキャン

**Cycle ID:** `20260314171406`
**Started:** 2026-03-14 17:14:06
**Status:** 🟢 Done

---

## 📝 What & Why

ディレクトリ展開時にその階層のファイルだけ非同期でdiffを取り、バッジを `[?]` → `[M]`/`[=]` 等に更新する。
現状はファイル選択時にしかバッジが確定しないため、ユーザーはどのファイルに差分があるか事前に把握できない。

## 🎯 Goals

- ディレクトリ展開時に、その直下の**ファイルのみ**（再帰しない）をバックグラウンドで diff スキャン
- スキャン結果が返ってきたら `flat_nodes` のバッジを順次更新（UIブロックなし）
- ディレクトリを折りたたんだら進行中のスキャンをキャンセル
- 結果はキャッシュに保存し、再展開時はキャッシュヒットで即表示
- 1ディレクトリ内のファイル数が多い場合でも並行度制限で SSH を圧迫しない

## 📐 Design

### 既存パターンの活用

`merge_scan`（`std::thread::spawn` + `mpsc::channel` + `poll`）パターンをそのまま踏襲。ただし merge_scan と異なり:

1. **スコープが1階層のみ**（再帰しない）
2. **複数ディレクトリの同時スキャンが可能**（merge_scan は1つだけ）
3. **ツリー展開は不要**（展開済みの直下ファイルだけ対象）
4. **マージダイアログは不要**（バッジ更新のみ）

### 状態管理

```rust
// app/types.rs に追加
/// バッジスキャンの進捗メッセージ（ワーカースレッド → メインスレッド）
pub enum BadgeScanMsg {
    /// 1ファイルのスキャン結果（コンテンツ + バイナリ情報）
    FileResult {
        path: String,
        left_content: Option<String>,
        right_content: Option<String>,
        left_binary: Option<BinaryInfo>,
        right_binary: Option<BinaryInfo>,
    },
    /// スキャン完了
    Done { dir_path: String },
    /// エラー（致命的でない、ログのみ）
    Error { path: String, message: String },
}
```

**設計判断: メッセージにコンテンツを含める理由**

代替案として「バッジ結果（`Badge` enum 値）だけをメッセージで送る」方式も検討したが、以下の理由から
コンテンツを含める現方式を採用する:

1. キャッシュ（`BoundedCache`）は `AppState` が所有しており、ワーカースレッドからは書き込めない
   （`Arc<Mutex<BoundedCache>>` にすると `AppState` 全体の設計に波及する）
2. コンテンツをキャッシュに保存することで、後続のファイル選択時に再取得が不要になる
3. チャネルバッファの肥大化リスクへの対策として、ワーカースレッドでは `read_files_batch` の
   チャンク分割（既存インフラ）を使い、送信ペースを制限する。poll 側は各 tick で全メッセージを
   drain するため、通常の操作では溜まりにくい

### キャンセル機構

```rust
// std::sync::atomic::AtomicBool でキャンセルフラグ
// ディレクトリ折りたたみ時に cancelled.store(true, Relaxed)
// ワーカースレッド側は各ファイル処理前に cancelled.load(Relaxed) をチェック
```

### Files to Change

```
src/
  app/types.rs            - BadgeScanMsg 型追加
  runtime/badge_scan/
    mod.rs                - start_badge_scan(), cancel_badge_scan() エントリーポイント
    task.rs               - ワーカースレッド処理（ファイル読み込み + 結果送信）
    poll.rs               - poll_badge_scan_results() イベントループからのポーリング
  runtime/mod.rs          - badge_scan モジュール宣言、TuiRuntime にフィールド追加
  handler/tree_keys.rs    - expand_directory() からスキャン起動、折りたたみ時キャンセル
  app/badge.rs            - compute_badge() は既存のまま（キャッシュから判定するので変更不要）
```

### Key Points

- **非再帰スキャン**: 展開したディレクトリの直下ファイルのみ対象。サブディレクトリは対象外（サブディレクトリ展開時にそのディレクトリ分がスキャンされる）
- **複数同時スキャン**: `HashMap<String, BadgeScanEntry>` で管理。別ディレクトリを展開した場合は同時に走る

#### TuiRuntime に追加するフィールド

```rust
// runtime/mod.rs — TuiRuntime に追加
/// バッジスキャンの進行中エントリ（ディレクトリパス → エントリ）
pub badge_scans: HashMap<String, BadgeScanEntry>,

// runtime/badge_scan/mod.rs
pub struct BadgeScanEntry {
    pub receiver: mpsc::Receiver<BadgeScanMsg>,
    pub cancel_flag: Arc<AtomicBool>,
}
```
- **キャッシュ活用**: スキャン結果は `left_cache`/`right_cache` に保存。既にキャッシュにあるファイルはスキップ
- **バッジ更新タイミング**: `poll_badge_scan_results()` で `FileResult` を受信したら即 `compute_badge()` → `flat_nodes` 更新
- **ディレクトリバッジ伝播**: ファイルバッジ更新後、親ディレクトリのバッジも再計算（`compute_dir_badge()`）
- **キャンセル**: ディレクトリ折りたたみ時 (`h`/`Left`) に `AtomicBool` でキャンセル。receiver も drop
- **並行度制限**: ワーカースレッド内でバッチ読み込み（`read_files_batch`）を使用。一度に全ファイル送るのではなくチャンクに分割
- **既存スキャンとの共存**: `merge_scan` や `scanner`（Shift+F）とは独立。同時に走っても干渉しない

### LeftOnly / RightOnly ファイルの処理

片方のツリーにしか存在しないファイルはコンテンツ読み込み不要。ワーカースレッドでは:

1. `collect_direct_children_files()` がマージ済みツリーの直下ファイルを列挙
2. 各ファイルについてツリーの存在有無（`left_tree` / `right_tree`）を確認
3. **片方のみ存在**: コンテンツを読まず、`FileResult` に存在する側のコンテンツだけ設定（もう一方は `None`）。poll 側で `compute_badge()` が `LeftOnly` / `RightOnly` を判定
4. **両方存在**: 両方のコンテンツを読み込んで `FileResult` に設定

### ローカルファイルの読み込み方式

`left_source` がローカルの場合（通常ケース）と、リモートの場合（remote-to-remote 比較）で処理が分岐する:

- **ローカル (`Side::Local`)**: ワーカースレッド内で `std::fs::read_to_string()` で直接読み込む（SSH 不要）。`merge_scan/task.rs` と同じパターン
- **リモート (`Side::Remote`)**: SSH 経由の `read_files_batch()` で読み込む（right 側と同様）

### 多重スキャン防止

以下のケースで多重・無駄なスキャンが発生しないように設計する：

| ケース | 対策 |
|--------|------|
| **開く→閉じる→開く**（高速操作） | `start_badge_scan()` で dir_path をキーに重複チェック。同じディレクトリのスキャンが進行中なら起動しない。2回目の展開時はキャッシュ済みファイルをスキップするので実質的に差分のみスキャン |
| **開く→ディレクトリマージ** | `merge_scan` 開始時に該当ディレクトリのバッジスキャンをキャンセル（`cancel_badge_scan(dir_path)`）。merge_scan が完了すればキャッシュが埋まるのでバッジスキャンは不要 |
| **開く→ファイル選択** | ファイル選択時の `load_file_content` はキャッシュを invalidate してから再取得する。バッジスキャンの結果受信時に `left_cache.contains_key()` をチェックし、既にキャッシュ済みなら上書きしない |
| **スキャン中に再接続** | 再接続時に全バッジスキャンをキャンセル（`cancel_all_badge_scans()`）。古い SSH 接続のスレッドが残らないようにする |

### Loading バッジ表示

スキャン中のファイルが視覚的にわかるように、既存の `Badge::Loading` (`[..]`) を活用する：

```
スキャン開始時:
  対象ファイルのバッジを [?] → [..] に一括変更
  → ユーザーは「どのファイルがスキャン対象か」一目でわかる

結果到着時（逐次）:
  [..] → [M] / [=] / [+] / [-] に更新
  → ファイルごとに順次確定していく

キャンセル時:
  残っている [..] を [?] に戻す
  → 未完了のファイルは「未チェック」に戻る
```

### シーケンス図

```
User: ディレクトリ展開 (Enter/l/Right)
  → expand_directory()  [ツリーの子ノードロード、既存処理]
  → toggle_expand()     [flat_nodes 再構築]
  → start_badge_scan()
       1. 直下ファイル一覧取得（キャッシュ済みを除外）
       2. 重複チェック（同じ dir_path のスキャンが進行中なら skip）
       3. 対象ファイルのバッジを [?] → [..] に変更
       4. スレッド起動
       ↓ (別スレッド)
       read_files_batch(left_paths)
       read_files_batch(right_paths)  ← or read_file(local)
       ↓ 各ファイルごとに FileResult 送信

Event loop:
  poll_badge_scan_results()
    ← FileResult 受信
    → キャッシュ済みでなければ left_cache/right_cache に保存
    → compute_badge() でバッジ再計算 [..] → [M]/[=] 等
    → flat_nodes[i].badge 更新
    → 親ディレクトリバッジも再計算

User: ディレクトリ折りたたみ (h/Left)
  → cancel_badge_scan(dir_path)
       1. AtomicBool でキャンセル通知
       2. receiver drop
       3. 残っている [..] バッジを [?] に戻す

User: ディレクトリマージ (L/R)
  → merge_scan 開始前に cancel_badge_scan(dir_path)
```

### 不具合リスクと対策

1. **レースコンディション**: スキャン結果到着時に既にディレクトリが折りたたまれている
   - 対策: `flat_nodes` にパスが存在しなければバッジ更新をスキップ（キャッシュには保存する）

2. **キャッシュ不整合**: ユーザーがファイル選択で最新を読み込んだ直後にスキャン結果の古いデータが上書き
   - 対策: poll 側で `left_cache.contains_key()` チェック。キャッシュ済みなら上書きしない（ファイル選択の方が新しいデータを持っている）

3. **大量ファイルでの SSH 負荷**: 1ディレクトリに数百ファイル
   - 対策: `read_files_batch` のチャンク分割（既存インフラ）。さらにファイル数が閾値 **100** を超えた場合はスキャンをスキップしてステータスメッセージで通知（`"Too many files (N) — badge scan skipped"`）。閾値は `const BADGE_SCAN_MAX_FILES: usize = 100` として定義

4. **メモリ逼迫**: 大量ファイルの内容がキャッシュに載る
   - 対策: `BoundedCache`（既存、max 500）の LRU eviction がそのまま効く

5. **接続断**: スキャン中に SSH 接続が切れる
   - 対策: エラーを `BadgeScanMsg::Error` で通知。`is_connection_error` なら `is_connected = false` + disconnect

6. **多重スキャン**: 開閉の繰り返しやマージ操作との競合
   - 対策: 上記「多重スキャン防止」テーブル参照。dir_path キーの重複チェック + 操作連動キャンセル

## ✅ Tests

### Domain (純粋関数)
- [ ] `collect_direct_children_files()`: 直下ファイルのみ返す（サブディレクトリのファイルは含まない）
- [ ] `collect_direct_children_files()`: 空ディレクトリ → 空 Vec
- [ ] `collect_direct_children_files()`: キャッシュ済みファイルを除外するフィルタリング
- [ ] `filter_uncached_paths()`: キャッシュ済みパスを除外
- [ ] キャンセルフラグが立ったらスキャン中断の判定ロジック
- [ ] `revert_loading_badges()`: `[..]` のバッジを `[?]` に戻す（キャンセル時用）
- [ ] `set_loading_badges()`: 対象ファイルのバッジを `[?]` → `[..]` に変更
- [ ] `collect_direct_children_files()`: LeftOnly ファイル（片方のツリーにのみ存在）も対象に含む
- [ ] `collect_direct_children_files()`: RightOnly ファイル（片方のツリーにのみ存在）も対象に含む

### Runtime (badge_scan)
- [ ] `BadgeScanMsg` 型のバリアント構築テスト
- [ ] `start_badge_scan()`: 同じディレクトリの二重起動を防止（2回目は起動しない）
- [ ] `start_badge_scan()`: 異なるディレクトリは同時に走れる
- [ ] `cancel_badge_scan()`: 存在しないディレクトリのキャンセルは no-op
- [ ] `cancel_all_badge_scans()`: 全スキャンがキャンセルされる
- [ ] poll で FileResult 受信 → キャッシュ未保存なら挿入、保存済みなら上書きしない
- [ ] poll で FileResult 受信 → flat_nodes にパスが存在しなければバッジ更新スキップ（キャッシュには保存）
- [ ] poll で Done 受信 → 該当ディレクトリのスキャンエントリ削除
- [ ] poll で FileResult 受信 → LeftOnly/RightOnly ファイル（片方 None）でも正しくキャッシュ・バッジ更新
- [ ] `BADGE_SCAN_MAX_FILES` 超過時にスキャンがスキップされステータスメッセージが表示される

### Handler (tree_keys)
- [ ] 展開時にスキャンが起動されること（スキャン対象ファイル一覧の正しさ）
- [ ] 展開時に対象ファイルのバッジが `[..]` になること
- [ ] 折りたたみ時にキャンセルされ、`[..]` バッジが `[?]` に戻ること
- [ ] マージスキャン開始時に該当ディレクトリのバッジスキャンがキャンセルされること

## 🔒 Security

- [ ] スキャン対象パスはツリーノードから取得（ユーザー入力ではない）
- [ ] sensitive ファイルのコンテンツもキャッシュには入るが、表示時に既存の sensitive チェックが効く

## 📊 Progress

| Step | Description | Status |
|------|-------------|--------|
| 1 | `BadgeScanMsg` 型定義 + `collect_direct_children_files()` 純粋関数 | 🟢 |
| 2 | `runtime/badge_scan/task.rs` — ワーカースレッド処理 | 🟢 |
| 3 | `runtime/badge_scan/mod.rs` — start/cancel エントリーポイント | 🟢 |
| 4 | `runtime/badge_scan/poll.rs` — ポーリング + バッジ更新 | 🟢 |
| 5 | `TuiRuntime` フィールド追加 + イベントループ統合 | 🟢 |
| 6 | `handler/tree_keys.rs` — 展開/折りたたみからの起動/キャンセル | 🟢 |
| 7 | テスト + testenv 検証 | 🟢 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done
