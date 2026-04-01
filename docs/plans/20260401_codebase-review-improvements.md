# コードベースレビュー改善計画

**Created:** 2026-04-01
**Status:** Planning
**Trigger:** codebase-review スコア 78/100 (B ランク) の指摘事項を系統的に解消する

---

## 背景

4専門エージェントによるコードベースレビュー (2026-04-01) で以下が検出された:
- Critical: 0 / High: 11 / Medium: 30 / Low: 26 / Info: 6
- 主要課題: セキュリティ1件、パフォーマンス4件、アーキテクチャ3件、衛生3件

目標スコア: **85+ (A ランク)**

---

## Step 1: セキュリティ即時修正 [5分]

**S-H1: `stat_remote_files` のコマンドインジェクション防止**

- `src/runtime/remote_io.rs:308`
- `format!("'{}'", p)` → `shell_escape(p)` に変更
- ファイル名にシングルクォートを含むリモートファイルで壊れるリスク

```rust
// Before
let quoted: Vec<String> = full_paths.iter().map(|p| format!("'{}'", p)).collect();
// After
let quoted: Vec<String> = full_paths.iter().map(|p| shell_escape(p)).collect();
```

テスト: シングルクォート含むパスの stat テストを追加

---

## Step 2: Flaky テスト修正 [15分]

**H-H1: `set_current_dir()` テストに `#[serial]` 追加**

- `src/local/mod.rs:679,964,1007,1054`
- 4テストが `std::env::set_current_dir()` を使用するが `#[serial]` なし
- 並行テスト実行でレースコンディション → 非決定的失敗

対応: 4テストに `#[serial_test::serial]` 属性を追加

---

## Step 3: パフォーマンス・ホットパス最適化 [2-3時間]

### Step 3-1: Badge scan N+1 SSH パターン解消

**P-H1: `read_side_content` のバッチ化** (最高インパクト)
- `src/runtime/badge_scan/task.rs:86-105`
- 現状: ファイルごとに `read_side_content` → 内部で `read_files_batch_bytes(std::slice::from_ref(&abs_path))` を1ファイルずつ呼び出し
- 修正方針: ファイルパスリストを事前に収集し、`read_files_batch_bytes` で一括取得後、結果を分配
- 具体的変更:
  1. `run_badge_scan_inner` でリモートパスを一括収集
  2. `read_files_batch_bytes` でバッチ取得（内部で ARG_MAX チャンク分割済み）
  3. 取得結果を `HashMap` で保持し、ループ内で参照
- 効果: 50ファイル × 100ms/SSH = 5秒 → 1回のバッチ呼び出し ~100ms

テスト: 既存の badge scan テスト + バッチ動作の単体テスト追加

### Step 3-2: `rebuild_flat_nodes` 検索最適化

**P-H3: `query.to_lowercase()` の重複計算排除**
- `src/app/tree_ops.rs:124`
- `rebuild_flat_nodes` で1回だけ計算し、`flatten_node` にパラメータ `query_lower: Option<&str>` として渡す
- 100kノードで100k回の不要な String アロケーション排除

**P-H4: `dir_has_search_matches` の重複走査排除**
- `src/app/tree_ops.rs:129`
- 方針: `HashMap<String, bool>` でディレクトリパスごとの結果をキャッシュ
  - ボトムアップ1パスは `flatten_node` の再帰構造と合わないため、メモ化が現実的
  - `rebuild_flat_nodes` 冒頭でキャッシュ生成、`flatten_node` に `&HashMap` として渡す
- O(n*d) → O(n) に改善

テスト: 既存の検索テストが通ることを確認 + 大量ノードでのベンチマーク

### Step 3-3: `stat_remote_files` の HashMap 化

**P-H2: O(n*m) → O(n) パスマッチング**
- `src/runtime/remote_io.rs:322-338`
- `full_paths` の線形走査を `HashMap<&str, usize>` に変更

テスト: 既存の stat テストが通ることを確認

### Step 3-4: `try_agent_read_files_batch` 二重クローン除去

**P-M7: `rel_paths.to_vec()` の重複排除**
- `src/runtime/side_io.rs:647-648`
- `let paths = rel_paths.to_vec()` と `let owned_rel_paths = rel_paths.to_vec()` で同一データを2回クローン
- 1回だけクローンして共有する

テスト: コンパイル確認のみ (trivial fix)

---

## Step 4: レイヤー違反の解消 [2-3時間]

### Step 4-1: DialogState 型のドメイン層移動

**Q-H1: app/ → ui/dialog 逆依存の解消**
- `src/app/mod.rs:35`, `src/app/dialog_ops.rs:6-8`

現状の問題: `DialogState` は `HelpOverlay`, `ServerMenu`, `FilterPanel`, `ConfirmDialog` 等の
UI ウィジェット型を直接保持する enum。これらは描画ロジックを含むため、単純に app/ に移動しても
UI 依存が付いてくる。

**採用方針: ドメイン「意図」型と UI「実装」型の分離**

1. `app/dialog_types.rs` に **ドメイン層のダイアログ意図型** を新設:
   ```rust
   /// ドメイン層: 「何のダイアログを出すか」を表す (描画方法は知らない)
   pub enum DialogIntent {
       None,
       Help,
       ServerMenu,
       PairServerMenu,
       Confirm(ConfirmRequest),     // 確認要求データ (タイトル, メッセージ, 方向)
       BatchConfirm(BatchRequest),  // バッチ確認データ
       Filter(FilterRequest),      // フィルター状態データ
       Progress(ProgressState),    // 進捗データ
       MtimeWarning(MtimeData),    // mtime 警告データ
       HunkPreview(HunkData),      // ハンクプレビューデータ
   }
   ```
2. `AppState.dialog` の型を `DialogIntent` に変更
3. `ui/dialog/` は `DialogIntent` を受け取って対応する Widget を描画する
4. `app/dialog_ops.rs` は `DialogIntent` の操作のみ行い、`ui::dialog` を import しない

**不採用理由 (代替案):**
- 単純な型移動 (A/B): UI ウィジェット型がドメイン層に来てしまい、依存方向が改善しない
- `Deref` パターン: ダイアログ型には不適切

### Step 4-2: runtime/ → ui/dialog 依存の解消

**Q-H2: ランタイム層がダイアログ状態を直接操作**
- `src/runtime/merge_scan/mod.rs:19` — `use crate::ui::dialog::{DialogState, ProgressDialog, ProgressPhase}`

**修正方針:**
1. `ProgressState` を `app/dialog_types.rs` の `DialogIntent::Progress` に含める
2. runtime 層は `MergeScanMsg` (既存のドメインメッセージ) に進捗情報を含めて発行
3. handler 層の `poll_merge_scan_result` が `MergeScanMsg` を受け取り、`DialogIntent::Progress` に変換
4. runtime/ から `crate::ui::dialog` の import を完全除去

テスト: 既存の全テスト + コンパイル確認

---

## Step 5: God Module 分割 [半日]

### Step 5-1: `side_io.rs` の分割 (3,799行)

**Q-H3 + H-H3: CoreRuntime I/O + TuiRuntime 委譲の分離**
- 分割案:
  - `runtime/side_io/read.rs` — 読み取り操作 (text + binary)
  - `runtime/side_io/write.rs` — 書き込み操作
  - `runtime/side_io/tree.rs` — ツリー取得操作
  - `runtime/side_io/backup.rs` — バックアップ/リストア
  - `runtime/side_io/agent_helpers.rs` — Agent フォールバック + 結果変換
  - `runtime/side_io/tui_delegate.rs` — TuiRuntime 委譲 (Deref またはマクロで削減)
  - `runtime/side_io/mod.rs` — re-export

**H-H3: TuiRuntime 委譲ボイラープレート削減**
- ~120行 (line 1597-1718) の 1:1 フォワードを削減
- **`Deref` は不採用**: 委譲メソッドは全て `&mut self` を取るため `DerefMut` が必要だが、
  `DerefMut` をラッパー型に実装するのは Rust アンチパターン (意図しないメソッド解決を引き起こす)
- **採用方針: `macro_rules!` による委譲マクロ**:
  ```rust
  macro_rules! delegate_to_core {
      ($( fn $name:ident(&mut self $(, $arg:ident: $ty:ty)*) -> $ret:ty; )*) => {
          $(pub fn $name(&mut self $(, $arg: $ty)*) -> $ret {
              self.core.$name($($arg),*)
          })*
      };
  }
  ```
- 120行 → ~30行に削減。型シグネチャの同期も宣言的に管理可能

テスト: 既存テストの移動 + コンパイル確認

---

## Step 6: 定数重複・衛生改善 [1時間]

### Step 6-1: 定数の一元化

**H-H2: 重複定数の統合**
- `AGENT_READ_BATCH_SIZE` (2箇所) → 1箇所に統合
- `MAX_DIR_ENTRIES` (2箇所) → 共通定数モジュールへ
- 4MB chunk size (3箇所) → 単一の正規定数 + エイリアス

### Step 6-2: マジックナンバーの定数化

- `src/main.rs:593` — `10 * 1024 * 1024` → `MAX_DEBUG_LOG_BYTES`
- `src/main.rs:590` — `10_000` → `MAX_EVENT_LOG_LINES`
- `src/main.rs:664,666` — `100`, `60` → タイムアウト定数

### Step 6-3: `unsafe` ブロックに SAFETY コメント追加

- `src/agent/file_io.rs:112,157,172`
- `src/app/clipboard_write.rs:105,135`

テスト: コンパイル確認のみ (リファクタリング)

---

## Step 7: その他 Medium 指摘の対応 [1-2時間]

### Step 7-1: エラーハンドリング改善

- `src/handler/merge_exec.rs:61` — symlink_merge の結果破棄を修正
- `src/runtime/merge_scan/mod.rs:76` — `.expect()` → `anyhow::bail!()`
- `src/backup/mod.rs:190` — `session_id` のトラバーサル検証追加

### Step 7-2: `draw_ui` の状態変更排除

- `src/ui/render.rs:18` — `&mut AppState` → viewport サイズをハンドラ層で設定
- **具体的な移動先**: `src/main.rs` のイベントループ内、`terminal.draw()` 呼び出し前に
  `terminal.size()?` で `Rect` を取得し、レイアウト計算して
  `state.tree_visible_height` / `state.diff_visible_height` を設定
  - 注: `frame.area()` はクロージャ内でしか使えないため、`terminal.size()` を使う
  - `terminal.size()` と `frame.area()` は同じ値を返す（ratatui の実装を確認済み）
- `draw_ui` のシグネチャを `fn draw_ui(frame: &mut Frame, state: &AppState)` に変更

### Step 7-3: `too_many_arguments` 対応 (部分的)

- 最も影響の大きい2-3関数にパラメータ構造体を導入
- 全12箇所の解消は後続サイクルへ

---

## 実行順序とスケジュール

| Step | 内容 | 工数 | 優先度 | 依存 |
|------|------|------|--------|------|
| 1 | セキュリティ修正 | 5分 | **最優先** | なし |
| 2 | Flaky テスト修正 | 15分 | **最優先** | なし |
| 3 | パフォーマンス最適化 (badge N+1, 検索, stat, clone) | 2-3時間 | 高 | なし |
| 4 | レイヤー違反解消 (DialogIntent 分離) | 2-3時間 | 高 | なし |
| 5 | God Module 分割 (side_io.rs + 委譲マクロ) | 半日 | 中 | Step 4 完了後推奨 |
| 6 | 定数・衛生改善 | 1時間 | 中 | なし |
| 7 | Medium 指摘対応 | 1-2時間 | 中 | なし |

**Step 1-2 は即時実行可能（並行も可）**
**Step 3-4 は独立して並行実行可能**
**Step 5 は Step 4 の DialogIntent 型定義後に実施が効率的**

---

## スコープ外（今回対応しない）

- Low/Info レベルの指摘 (26件) — 個別に issue 化して段階的に対応
- `config.rs` (2,447行) / `output.rs` (2,411行) の分割 — 独立した計画で対応
- `badge.rs` (2,192行) の分割 — テストが大部分を占めるため優先度低
- handler 層のテスト追加 (22ファイル) — `*_logic.rs` パターンで対応済みの部分が多い
- `too_many_arguments` の全12箇所対応 — 段階的に対応

---

## 完了条件

- [ ] 全 High 指摘 (11件) が解消されている
- [ ] `cargo test` が全テスト pass
- [ ] `cargo clippy` が警告ゼロ
- [ ] `cargo fmt --check` が差分ゼロ
- [ ] 再レビューで 85+ (A ランク) を達成
