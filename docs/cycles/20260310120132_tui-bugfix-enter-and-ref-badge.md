# TUI バグ修正: Enter 連打で diff 消失 + 再接続後マージで [3-] バッジ

**Cycle ID:** `20260310120132`
**Started:** 2026-03-10 12:01:32
**Status:** 🟢 Completed

---

## 📝 What & Why

TUI の実運用テストで発見した2つのバグを修正する。

### Bug 1: Enter 連打で diff 表示が消失する

**再現手順:** ファイル選択 → コンフリクト表示 → 再度 Enter → diff 消える

**根本原因:** `load_file_content()` (merge_content.rs:209) で `rebuild_flat_nodes()` を呼ぶと
`flat_nodes` が再構築される。その後に `select_file()` (tree_keys.rs:98) が呼ばれるが、
`rebuild_flat_nodes()` で `flat_nodes` が変わったのに `tree_cursor` はインデックスベースのため
別のノードを指す可能性がある。特にバッジ更新によりフィルタリング結果が変わると、
カーソルが範囲外になったり別ファイルを指す。

**影響:** `select_file()` がディレクトリノードや想定外のファイルに当たり、
diff 計算がスキップされ表示が消える。

### Bug 2: 再接続後のディレクトリマージで [3-] バッジが付く

**再現手順:** r で再接続 → ディレクトリマージ実行 → [3-] バッジ

**根本原因:** 再接続時に `ref_tree` がルート直下のみの浅いスキャンで再取得される（reconnect.rs:47）。
ディレクトリ展開時に `load_ref_children()` で ref_tree の子ノードが遅延ロードされるが、
`execute_batch_merge()` 後の `sync_cache_after_merge()` (dialog_ops.rs:304) では
`left_cache` / `right_cache` のみ更新し、**`ref_cache` のコンフリクト情報は更新しない**。
さらに `rebuild_flat_nodes()` で `compute_ref_badge()` が呼ばれると、
ref_tree にノードが存在しない（遅延ロード未完了の深いパス）ため `MissingInRef` → `[3-]` になる。

## 🎯 Goals

- Enter 連打しても diff が消えない（カーソル位置とファイル選択が整合する）
- マージ後にバッジが正しく計算される（ref キャッシュも同期）

## 📐 Design

### Step 1: `rebuild_flat_nodes()` 後のカーソル復元

**問題:** `rebuild_flat_nodes()` がインデックスをクランプするだけで、
元のファイルパスへの復元を行わない。

**修正方針:** `rebuild_flat_nodes()` 内でカーソル復元ロジックを追加する。

```rust
pub fn rebuild_flat_nodes(&mut self) {
    // 再構築前のカーソル位置のパスを保持
    let cursor_path = self.flat_nodes.get(self.tree_cursor).map(|n| n.path.clone());

    let mut nodes = Vec::new();
    let merged = self.merge_tree_nodes();
    for node in &merged {
        self.flatten_node(node, "", 0, &mut nodes);
    }
    self.flat_nodes = nodes;

    // パスベースでカーソル位置を復元
    if let Some(path) = cursor_path {
        if let Some(idx) = self.flat_nodes.iter().position(|n| n.path == path) {
            self.tree_cursor = idx;
        } else if self.tree_cursor >= self.flat_nodes.len() && !self.flat_nodes.is_empty() {
            self.tree_cursor = self.flat_nodes.len() - 1;
        }
    } else if self.tree_cursor >= self.flat_nodes.len() && !self.flat_nodes.is_empty() {
        self.tree_cursor = self.flat_nodes.len() - 1;
    }
}
```

**Key Point:** `selected_path` とは別に `tree_cursor` のパスを保持する理由は、
`selected_path` は diff 表示中のファイルパスであり、`tree_cursor` はツリー上の
フォーカス位置であるため、これらは必ずしも一致しない（ディレクトリにフォーカスしている場合等）。

#### 安全性分析: 既存呼び出し元への影響

`rebuild_flat_nodes()` は33箇所以上から呼ばれる共通関数。カーソル復元ロジックを内部に
組み込むことで全呼び出し元が恩恵を受ける。既存コードとの整合性を確認済み：

- **`server_switch.rs` L58-59**: `tree_cursor = 0` → `rebuild_flat_nodes()` の順。
  復元ロジックは「再構築**前**の `flat_nodes[0]` のパス」を保持するため、
  サーバ切り替え後のルートノードに正しく復元される。意図通り。
- **`reconnect.rs` L129,137**: `restore_cursor_position()` で `tree_cursor` を設定後に
  `rebuild_flat_nodes()` を呼ぶ。復元ロジックにより、さらに正確なパス復元が行われる。
- **`search_keys.rs`**: 検索クエリ変更時に呼ばれる。復元ロジックは検索フィルタ適用後の
  `flat_nodes` に対してパス検索するため、フィルタで消えたノードはクランプで処理される。
- **初期化時 (`mod.rs` L226)**: `flat_nodes` が空の状態 → `cursor_path = None` →
  フォールバックのクランプが動作。安全。
- **`toggle_expand()` (tree_ops.rs L30)**: ディレクトリ展開/折りたたみ後に呼ばれる。
  カーソルは展開したディレクトリ自身を指しており、再構築後もそのパスに復元される。

#### 呼び出し順序の明確化

`merge_content.rs` L209 では `rebuild_flat_nodes()` のみ呼ばれ、`select_file()` は
呼ばれない。`select_file()` は `tree_keys.rs` L98 で別途呼ばれる。
`rebuild_flat_nodes()` でカーソルが正しいパスに復元されるため、
後続の `select_file()` も `flat_nodes[tree_cursor]` で正しいファイルを参照する。

### Step 2: `sync_cache_after_merge` で conflict_cache をクリア

**問題:** マージ後に left_cache/right_cache が同期されるが、conflict_cache が
古い情報のまま残る。

**修正方針:** `sync_cache_after_merge()` 内で `conflict_cache` を更新する。
マージ後は left == right になるため、コンフリクトは解消されている。

```rust
pub fn sync_cache_after_merge(&mut self, path: &str, content: &str, direction: MergeDirection) {
    match direction {
        MergeDirection::LeftToRight => {
            self.right_cache.insert(path.to_string(), content.to_string());
        }
        MergeDirection::RightToLeft => {
            self.left_cache.insert(path.to_string(), content.to_string());
        }
    }
    // マージ後は left == right なのでコンフリクトは解消
    self.conflict_cache.remove(path);
}
```

#### 安全性根拠: 無条件 remove が正しい理由

`sync_cache_after_merge()` はフルファイルマージ専用関数。呼び出し元は：
1. `execute_batch_merge()` (merge_batch.rs L162,L221) — バッチ単位のフルファイルマージ
2. `update_badge_after_merge()` (dialog_ops.rs L293) — 単一ファイルのフルファイルマージ

ハンクマージ（部分マージ）は `hunk_ops.rs` L180-183 で left_cache/right_cache を直接更新し、
`sync_cache_after_merge()` は呼ばない。したがって `conflict_cache.remove()` の
無条件実行は安全。フルマージ後は必ず left == right でコンフリクト解消済み。

**ref_cache 自体は更新不要:** ref_cache はリファレンスサーバのコンテンツであり、
マージ操作では変わらない。`[3-]` バッジの原因は ref_cache ではなく ref_tree のノード不在。

### Step 3: バッチマージ後に ref_tree の深さを同期

**問題:** バッチマージ後の `rebuild_flat_nodes()` で `compute_ref_badge()` が
ref_tree の遅延ロード未完了パスに対して `MissingInRef` を返す。

**修正方針:** `execute_batch_merge()` 内の `rebuild_flat_nodes()` 直前で、
マージされたファイルのディレクトリについて `load_ref_children()` を呼び出し、
ref_tree の深さを同期する。

具体的には `merge_batch.rs` の `rebuild_flat_nodes()` の直前に追加：

```rust
// ref_tree の深さ同期（マージしたファイルのディレクトリについて ref 子ノードをロード）
if state.has_reference() {
    let dirs: std::collections::BTreeSet<String> = files.iter()
        .filter_map(|(path, _)| path.rsplit_once('/').map(|(dir, _)| dir.to_string()))
        .collect();
    for dir in &dirs {
        super::merge_tree_load::load_ref_children(state, runtime, dir);
    }
}
```

#### 冗長ロード防止の設計根拠

`load_ref_children()` は内部で `is_loaded()` チェックを行い、既に展開済みの
ディレクトリはスキップする。したがって正常フロー（`expand_subtree_for_merge()` が
先に ref_tree を展開済み）では追加 I/O は発生しない。

再接続後のフローでは：
1. `reconnect.rs` L44: `ref_tree = None` → L47: `execute_ref_connect()` で浅い ref_tree 再取得
2. ディレクトリ展開（ユーザー操作）で `load_ref_children()` が呼ばれ left/right は展開
3. バッチマージ前に `expand_subtree_for_merge()` が呼ばれ left/right/ref すべて展開
4. **しかし**、非同期パス（merge_scan 経由）では `expand_subtree_for_merge()` を経由しない可能性がある

Step 3 の追加により、非同期パスでも ref_tree の深さが担保される。

### Files to Change

```
src/
  app/
    tree_ops.rs — rebuild_flat_nodes() にカーソルパス復元ロジック追加
    dialog_ops.rs — sync_cache_after_merge() に conflict_cache.remove 追加
  handler/
    merge_batch.rs — バッチマージ後に ref_tree 深さ同期
```

### Key Points

- **tree_ops.rs**: `rebuild_flat_nodes()` はプロジェクト全体で33箇所以上から呼ばれる共通関数。
  カーソル復元を関数内部で行うことで、全呼び出し元が恩恵を受ける。
  安全性分析で全主要呼び出しパターンとの整合性を確認済み。
- **dialog_ops.rs**: `conflict_cache.remove()` は1行追加のみ。
  `sync_cache_after_merge()` はフルファイルマージ専用であり、
  ハンクマージでは呼ばれないため無条件 remove は安全。
- **merge_batch.rs**: `load_ref_children()` の `is_loaded()` 内部チェックにより
  冗長ロードなし。非同期パスのエッジケースもカバー。

## ✅ Tests

### tree_ops.rs（カーソル復元）
- [x] `rebuild_flat_nodes` 後にカーソルが同じパスのノードを指す
- [x] ノードがフィルタで消えた場合、カーソルが範囲内にクランプされる
- [x] 空の flat_nodes でもパニックしない
- [x] `tree_cursor` を 0 にセットしてから `rebuild_flat_nodes` を呼んだ場合、`flat_nodes[0]` のパスに復元される（server_switch パターン）
- [x] ディレクトリ展開後の `rebuild_flat_nodes` でカーソルがディレクトリ自身を指し続ける（toggle_expand パターン）
- [x] カーソルパス消失+範囲内の意図的挙動を文書化（test_rebuild_cursor_path_gone_but_index_in_range）

### dialog_ops.rs（conflict_cache クリア）
- [x] `sync_cache_after_merge` (LeftToRight) 後に conflict_cache からパスが除去される
- [x] `sync_cache_after_merge` (RightToLeft) 後に conflict_cache からパスが除去される
- [x] conflict_cache にパスが存在しない場合でもパニックしない

### merge_batch.rs（collect_merge_dirs 純粋関数）
- [x] ネストされたパスからディレクトリ抽出
- [x] ルートファイルが "" として収集される（BLOCK-1 修正）
- [x] ルートとネストの混在
- [x] 同一ディレクトリの重複排除
- [x] 空入力で空セット

## 📊 Progress

| Step | 内容 | Status |
|------|------|--------|
| 1 | tree_ops.rs — カーソルパス復元 | 🟢 |
| 2 | dialog_ops.rs — conflict_cache クリア | 🟢 |
| 3 | merge_batch.rs — ref_tree 深さ同期 + collect_merge_dirs 抽出 | 🟢 |
| Review | Round 1 BLOCK 3件修正 → Round 2 ALL PASS | 🟢 |
| Commit | 2589d72 | 🟢 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done
