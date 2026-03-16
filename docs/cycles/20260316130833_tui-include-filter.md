# TUI include フィルター対応（bootstrap + lazy load）

**Cycle ID:** `20260316130833`
**Started:** 2026-03-16 13:08:33
**Status:** 🟡 In Progress

---

## 📝 What & Why

前回の include-filter サイクル（20260313204742）で **再帰スキャン**（CLI status / diff filter Shift+F）には include フィルターが正しく適用されるようになったが、TUI の **bootstrap（初期ツリー取得）** と **lazy load（ディレクトリ展開）** には include が渡されていない。

これにより `include = ["src", "docs"]` を設定しても、TUI 起動時に root 直下の全ディレクトリ・ファイルが表示され、展開もできてしまう。

### 根本原因

1. **bootstrap**: `scan_local_tree()` と `fetch_remote_tree()` は `exclude` のみ受け取る浅いスキャン関数。`include` パラメータが存在しない
2. **lazy load**: `fetch_children()` → `scan_dir()` / `list_dir()` も `exclude` のみ。展開先が include パス配下かどうかのチェックがない
3. **reference ツリー**: `apply_reference_from_runtime()` も同じ浅いスキャン関数を使用

### CLI の状況

| コマンド | フルスキャン | 部分スキャン | 備考 |
|---------|------------|------------|------|
| **status** | ✓ include 適用 | N/A | 問題なし |
| **diff** | ✓ include 適用 | ✗ 未適用 | 部分スキャンは意図的（ユーザーが明示的にパス指定） |
| **merge** | ✓ include 適用 | ✗ 未適用 | 同上 |
| **sync** | ✓ include 適用 | ✗ 未適用 | 同上 |

CLI の部分スキャン（`fetch_tree_for_subpath`）は、ユーザーが明示的にパスを指定しているため include を適用しない設計判断。これは妥当。**今回のスコープ外**。

## 🎯 Goals

- TUI bootstrap で include フィルターを適用し、指定パス外のエントリを表示しない
- TUI lazy load で include パス外のディレクトリ展開を防ぐ
- reference ツリーも同様に include を適用
- 既存の再帰スキャン（CLI / diff filter）の動作に影響しない

## 📐 Design

### include フィルターの浅いスキャン向けロジック

再帰スキャンでは「スキャン起点自体を include パスに絞る」アプローチだが、浅いスキャン（1階層）では異なるロジックが必要。

`include = ["vendor/current", "src"]` の場合:
- root 直下: `vendor`（include パスの祖先）、`src`（include パスそのもの）だけ表示
- `vendor` 展開: `current` のみ表示（include パスの子孫セグメント）
- `src` 展開: 全て表示（include パス配下）
- `vendor/current` 展開: 全て表示（include パス配下）

#### 新しい純粋関数: `is_path_included()`

```rust
/// エントリの相対パスが include フィルターに基づいて表示すべきか判定する。
///
/// 表示条件（いずれかを満たす）:
/// 1. include が空 → 常に true（フィルタなし）
/// 2. エントリが include パスの祖先（例: "vendor" は "vendor/current" の祖先）
/// 3. エントリが include パスと完全一致
/// 4. エントリが include パス配下（例: "src/main.rs" は "src" の配下）
pub fn is_path_included(entry_rel_path: &str, include_paths: &[String]) -> bool
```

この関数を `filter.rs` に追加。浅いスキャン結果のフィルタリングに使用。

### 変更方針

**浅いスキャン関数の API は変えない**。代わりに、呼び出し側で結果をフィルタリングする。

理由:
- `scan_local_tree()` は `scan_dir()` に委譲しており、`scan_dir()` は lazy load でも使われる汎用関数
- API を変えると影響範囲が大きい
- フィルタリングは結果に対して `is_path_included()` を適用するだけで済む

### Files to Change

```
src/
  filter.rs            - is_path_included() 純粋関数を追加
                       - filter_tree_by_include() ヘルパー関数を追加
  app/mod.rs           - AppState に include_patterns フィールド追加（exclude_patterns と同様）
  runtime/bootstrap.rs - fetch_left_side / fetch_right_side で include フィルタ適用
                       - apply_reference_from_runtime で include フィルタ適用
  runtime/side_io.rs   - fetch_children で include チェック追加（ローカル・リモート共通）
  handler/reconnect.rs - reconnect / server switch 時の include フィルタ適用
```

## 🔧 Implementation Steps

### Step 1: `is_path_included()` + `filter_tree_by_include()` の追加

**ファイル**: `src/filter.rs`

#### 1a: `is_path_included()`

`is_path_included(entry_rel_path: &str, include_paths: &[String]) -> bool` を追加。

ロジック:
- `include_paths` が空 → `true`（フィルタなし）
- `entry_rel_path` がいずれかの include パスの祖先 → `true`
- `entry_rel_path` がいずれかの include パスと完全一致 → `true`
- `entry_rel_path` がいずれかの include パス + "/" で始まる → `true`
- それ以外 → `false`

テスト:
- include 空 → 常に true
- 完全一致: `is_path_included("src", &["src"])` → true
- 配下: `is_path_included("src/main.rs", &["src"])` → true
- 祖先: `is_path_included("vendor", &["vendor/current"])` → true
- 無関係: `is_path_included("docs", &["src"])` → false
- 前方一致の誤爆防止: `is_path_included("srclib", &["src"])` → false
- 深い祖先: `is_path_included("a", &["a/b/c"])` → true
- 複数パス: `is_path_included("docs", &["src", "docs"])` → true

#### 1b: `filter_tree_by_include()`

`filter_tree_by_include(tree: &mut FileTree, include: &[String])` を追加。
bootstrap（Step 3）と reconnect（Step 5）から再利用する公開関数。

```rust
/// FileTree のルートノードを include フィルターで絞り込む。
pub fn filter_tree_by_include(tree: &mut FileTree, include: &[String]) {
    if include.is_empty() { return; }
    tree.nodes.retain(|node| is_path_included(&node.name, include));
}
```

テスト:
- include 空 → ノード変更なし
- include 設定 → マッチしないノードが除去される

### Step 2: AppState に include_patterns を追加

**ファイル**: `src/app/mod.rs`（AppState 定義箇所）, `src/runtime/bootstrap.rs`

`AppState` に `include_patterns: Vec<String>` フィールドを追加。
bootstrap で `config.filter.include.clone()` を代入（`exclude_patterns` と同じパターン）。

> **注意**: AppState は `src/app/mod.rs` に定義されている（`types.rs` ではない）。
> `exclude_patterns` が同ファイル内で定義されていることを参照。

> **用途**: `include_patterns` は現時点では直接参照箇所が限定的だが、
> `exclude_patterns` と対称に AppState に保持することで、
> 将来的な TUI 上での include フィルタ表示・編集 UI に備える。
> 実際のフィルタリングは `config.filter.include`（runtime 経由）を使用する。

### Step 3: bootstrap の include フィルタ適用

**ファイル**: `src/runtime/bootstrap.rs`

`fetch_left_side()` と `fetch_right_side()` で:
1. 既存の `scan_local_tree()` / `fetch_remote_tree()` はそのまま呼ぶ
2. 結果の `tree.nodes` に対して `is_path_included()` で**フィルタリング**
3. include パスの先頭セグメントにマッチしないノードを除去

`apply_reference_from_runtime()` にも同様のフィルタリングを適用。

Step 1b で定義した `filter_tree_by_include()` を使用する。

テスト:
- include 未設定 → 全ノード表示（後方互換）
- include 設定 → 指定パスとその祖先のみ表示
- include が深いパス → 中間ディレクトリも表示

### Step 4: lazy load の include チェック（ローカル・リモート共通）

**ファイル**: `src/runtime/side_io.rs`（`CoreRuntime::fetch_children`）

`fetch_children()` の結果に対して include フィルタリングを追加:
1. `Side::Local` / `Side::Remote` 両分岐の **後** で、取得した children に `is_path_included()` を適用
2. 各 child の相対パス（`dir_rel_path/child.name`）で判定
3. include が空の場合はフィルタなし（従来動作）

**設計判断**: フィルタリングは `fetch_children`（side_io.rs）で統一的に行う。
`fetch_remote_children`（remote_io.rs）側には追加しない。
理由: `fetch_children` は Side の分岐を吸収する統一 API であり、
ここでフィルタすることで local/remote 双方に一貫して適用される。
remote_io.rs にも入れると二重フィルタになり DRY 違反になる。

**注意**: `fetch_children` は `&self` で `config.filter.include` にアクセスできるので、追加の引数は不要。
reference ツリーの lazy load（`handler/merge_tree_load.rs`）も同じ `fetch_children` を経由するため、追加の変更は不要。

テスト:
- include パス配下のディレクトリ展開 → 全 children 表示
- include パスの祖先ディレクトリ展開 → include に沿った children のみ
- include 未設定 → 全 children 表示

### Step 5: reconnect / server switch の include フィルタ適用

**ファイル**: `src/handler/reconnect.rs`

`reconnect.rs` にも `scan_local_tree()` / `fetch_remote_tree()` の呼び出しがあり、
サーバー切り替え・再接続時にツリーを再取得している。
ここでも取得後に `filter_tree_by_include()` を適用する。

Step 1 で `filter.rs` に定義した `filter_tree_by_include()` を呼び出す。

テスト:
- reconnect 後のツリーに include フィルタが適用されていること

## ✅ Tests

### filter.rs — is_path_included + filter_tree_by_include
- [ ] include 空 → 常に true
- [ ] 完全一致 → true
- [ ] 配下パス → true
- [ ] 祖先パス → true
- [ ] 無関係パス → false
- [ ] 前方一致の誤爆防止（セグメント境界チェック）
- [ ] 深い祖先チェーン
- [ ] 複数 include パスの OR 評価
- [ ] filter_tree_by_include: include 空でノード変更なし
- [ ] filter_tree_by_include: include 設定でマッチしないノード除去

### bootstrap フィルタリング
- [ ] include 未設定で全ノード表示（後方互換）
- [ ] include 設定でフィルタリング動作
- [ ] 深い include パスで中間ディレクトリ表示

### lazy load フィルタリング（side_io.rs fetch_children — ローカル・リモート共通）
- [ ] include パス配下の展開 → 全表示
- [ ] include パス祖先の展開 → 部分表示
- [ ] include 未設定の展開 → 全表示
- [ ] reference ツリーの展開も同じパスを通ること（handler/merge_tree_load.rs 経由）

### reconnect / server switch フィルタリング
- [ ] reconnect 後のツリーに include フィルタが適用されていること

## 📊 Progress

| Step | Description | Status |
|------|-------------|--------|
| 1 | `is_path_included()` + `filter_tree_by_include()` 純粋関数 | 🟢 |
| 2 | AppState に include_patterns 追加 | ⚪ |
| 3 | bootstrap の include フィルタ | ⚪ |
| 4 | lazy load の include チェック（ローカル・リモート共通） | ⚪ |
| 5 | reconnect / server switch の include フィルタ | ⚪ |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**前提サイクル:** [20260313204742_include-filter.md](./20260313204742_include-filter.md) — include フィルタ基盤（再帰スキャン対応済み）
