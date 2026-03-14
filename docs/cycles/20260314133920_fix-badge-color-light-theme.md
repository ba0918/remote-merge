# バッジ色のライトテーマ対応（黄色→紫系）

**Cycle ID:** `20260314133920`
**Started:** 2026-03-14 13:39:20
**Status:** 🟢 Complete

---

## 📝 What & Why

ライト系テーマ（base16-ocean.light 等）で、黄色系バッジ（`[M]`, `[3≠]` 等）が背景と同化して見辛い。
バッジ色をパレットに集約し、ライト/ダーク両対応の配色にする。

## 🎯 Goals

- ライトテーマで全バッジが視認可能になる
- バッジ色をハードコードから `TuiPalette` 経由に統一
- ダークテーマの見た目を大きく崩さない

## 📐 Design

### 現状の問題

`Color::Yellow` がハードコードされている全箇所（コード実査済み）:

**A. バッジ色（is_light で切り替えが必要）**
1. `src/ui/tree_view.rs:25` — `Badge::Modified => Color::Yellow`
2. `src/app/three_way.rs:39` — `ThreeWayFileBadge::Differs => Color::Yellow`
3. `src/app/three_way.rs:73` — `ThreeWayLineBadge::Differs => Color::Yellow`
4. `src/ui/dialog/batch_confirm.rs:226` — `Badge::Modified => Color::Yellow`

**B. ダイアログ枠・ラベル（dialog_accent で統一）**
5. `src/ui/dialog/batch_confirm.rs:127` — ダイアログ枠
6. `src/ui/dialog/batch_confirm.rs:175` — バッジラベル色
7. `src/ui/dialog/batch_confirm.rs:195` — Modified ラベル
8. `src/ui/dialog/batch_confirm.rs:267` — large batch ガイド
9. `src/ui/dialog/confirm.rs:90` — ダイアログ枠
10. `src/ui/dialog/hunk_preview.rs:79` — ダイアログ枠
11. `src/ui/dialog/three_way_summary.rs:51` — ダイアログ枠
12. `src/ui/dialog/help.rs:179` — ラベル色
13. `src/ui/dialog/mod.rs:302` — large batch ガイド
14. `src/ui/dialog/mtime_warning.rs:96,106,130,175,213,267` — ラベル・枠（6箇所）

**C. その他 UI 要素（dialog_accent で統一）**
15. `src/ui/render.rs:135` — エージェントステータス色
16. `src/ui/render.rs:195` — UnsavedChanges ダイアログ枠
17. `src/ui/render.rs:202` — MtimeWarning ダイアログ枠
18. `src/ui/render.rs:303` — プログレスダイアログ テキスト色
19. `src/ui/diff_view/content_render.rs:84` — バイナリファイル メッセージ色
20. `src/ui/diff_view/line_render.rs:119,275` — 確定待ちインジケータ(⏎)色

**変更不要（テストフィクスチャ）**
- `src/ui/diff_view/search.rs` — テスト内のハイライト色フィクスチャ
- `src/ui/diff_view/style_utils.rs` — テスト内のスタイルフィクスチャ

### 方針

**Phase A: バッジ色をパレットに集約**

`TuiPalette` にバッジ関連色フィールドを追加し、`is_light` 判定で紫系/黄色系を切り替え。

既存フィールド:
- `badge_modified` — **あるが `is_light` による切り替えがない（固定で `#ebcb8b`）。修正必要**
- `badge_equal` — **あるが `is_light` による切り替えがない。修正必要**

新規追加が必要なフィールド:
- `badge_differs` — 3way [3≠] 用
- `badge_left_only` — [+] 用
- `badge_right_only` — [-] 用
- `badge_unchecked` — [?] 用
- `badge_loading` — [..] 用
- `badge_error` — [!] 用
- `badge_conflict` — [C!] 用
- `badge_ref_exists` — 3way [3+] 用
- `badge_ref_missing` — 3way [3-] 用
- `dialog_accent` — ダイアログ枠・ラベル・確定待ちインジケータ用アクセント色

**Phase B: UI コード側をパレット参照に置換**

ハードコード `Color::Yellow` → `palette.badge_modified` / `palette.badge_differs` 等に置換。

### 配色案

| バッジ     | ダークテーマ                | ライトテーマ              |
|-----------|---------------------------|-------------------------|
| Modified  | 黄色 `#ebcb8b`            | 紫 `#7c3aed` (violet-600) |
| Differs   | 黄色 `#ebcb8b`            | 紫 `#7c3aed`             |
| Equal     | 緑 `#a3be8c`              | 緑 `#16a34a` (green-600)  |
| LeftOnly  | シアン `Color::Cyan`       | ティール `#0d9488`        |
| RightOnly | マゼンタ `Color::Magenta`  | ピンク `#db2777`          |
| Unchecked | ダークグレイ               | グレイ `#6b7280`          |
| Loading   | 青 `Color::Blue`           | 青 `#2563eb`              |
| Error     | 赤ボールド                  | 赤 `#dc2626` ボールド      |
| Conflict  | 赤ボールド                  | 赤 `#dc2626` ボールド      |
| RefExists | シアン                     | ティール `#0d9488`         |
| RefMissing| マゼンタ                   | ピンク `#db2777`           |
| Dialog枠  | 黄色 `#ebcb8b`             | 紫 `#7c3aed`              |

### Files to Change

```
src/
  theme/palette.rs              - バッジ色フィールド追加 + 既存フィールドの is_light 切り替え追加
  app/three_way.rs              - style() にパレット引数追加
  ui/tree_view.rs               - badge_style() をパレット参照に変更
  ui/diff_view/three_way_badge.rs - badge_to_span() にパレット伝搬
  ui/dialog/batch_confirm.rs    - バッジ色 + ダイアログ枠をパレット参照に（4箇所）
  ui/dialog/mod.rs              - large batch ガイド色をパレット参照に
  ui/dialog/help.rs             - ラベル色をパレット参照に
  ui/dialog/confirm.rs          - ダイアログ枠色をパレット参照に
  ui/dialog/hunk_preview.rs     - ダイアログ枠色をパレット参照に
  ui/dialog/three_way_summary.rs - ダイアログ枠色をパレット参照に
  ui/dialog/mtime_warning.rs    - ラベル・枠をパレット参照に（6箇所）
  ui/render.rs                  - ステータス色・ダイアログ枠色をパレット参照に（4箇所）
  ui/diff_view/content_render.rs - バイナリメッセージ色をパレット参照に
  ui/diff_view/line_render.rs   - 確定待ちインジケータ(⏎)色をパレット参照に（2箇所）
```

### Key Points

- **`ThreeWayFileBadge::style()` / `ThreeWayLineBadge::style()`**: 現在パレットを受け取らない。引数に `&TuiPalette` を追加する
- **`badge_to_span()` (three_way_badge.rs)**: `badge.style()` を呼んでいるので、パレットの伝搬が必要。`badge_to_span()` に `&TuiPalette` 引数追加 → `unified_line_badge()`, `side_by_side_line_badge()` にも伝搬
- **`tree_view.rs:151`**: `ref_badge.style()` を呼んでいるため、`ThreeWayFileBadge::style()` にもパレット引数が必要
- **ダイアログ系の `Color::Yellow`**: バッジとは異なる「アクセント色」として `dialog_accent` を追加
- **`line_render.rs` の `Color::Yellow`**: バッジではなく確定待ちハンクインジケータ(⏎)の色。`dialog_accent` に統一
- **既存テストへの影響**: `three_way.rs` の `file_badge_styles`, `line_badge_styles` テストはハードコード色を検証しているため、パレット引数追加時に更新が必要
- **変更不要**: `search.rs`, `style_utils.rs` のテスト内 `Color::Yellow` はテーマ非依存のフィクスチャ

## 🔧 Implementation Steps

### Step 1: TuiPalette にバッジ色フィールド追加 + 既存フィールドの is_light 切り替え
- 新規フィールド追加: `badge_differs`, `badge_left_only`, `badge_right_only`, `badge_unchecked`, `badge_loading`, `badge_error`, `badge_conflict`, `badge_ref_exists`, `badge_ref_missing`, `dialog_accent`
- 既存フィールド修正: `badge_modified`, `badge_equal` に `is_light` 分岐を追加（現在は固定色）
- `from_theme()` で `is_light` に基づいて適切な色を設定
- **影響ファイル:** `src/theme/palette.rs`

### Step 2: tree_view.rs のバッジ色をパレット化
- `badge_style()` を `badge_style(badge, palette)` に変更、全バッジ色をパレット参照に
- `ref_badge.style()` 呼び出し（151行）を `ref_badge.style(&palette)` に変更
- **影響ファイル:** `src/ui/tree_view.rs`

### Step 3: three_way.rs のバッジスタイルをパレット化
- `ThreeWayFileBadge::style(&self, palette: &TuiPalette)` に引数追加
- `ThreeWayLineBadge::style(&self, palette: &TuiPalette)` に引数追加
- 呼び出し元の更新:
  - `three_way_badge.rs`: `badge_to_span()` にパレット引数追加 → `unified_line_badge()`, `side_by_side_line_badge()` にも伝搬
  - `batch_confirm.rs:226`: パレット参照に変更
- 既存テスト `file_badge_styles`, `line_badge_styles` をパレット引数付きに更新
- `content_render.rs` の `unified_line_badge()` / `side_by_side_line_badge()` 呼び出しにパレット引数を追加（`self.state.palette` を渡す）
- `three_way_badge.rs` のテスト（18個）にもパレット引数を追加
- **影響ファイル:** `src/app/three_way.rs`, `src/ui/diff_view/three_way_badge.rs`, `src/ui/diff_view/content_render.rs`, `src/ui/dialog/batch_confirm.rs`

### Step 4: ダイアログ系の Color::Yellow をパレット化
- `dialog_accent` を使って枠・ラベルの黄色を置換
- **パレット伝搬方式**: ダイアログウィジェットの `new(dialog, bg: Color)` を `new(dialog, palette: &TuiPalette)` に変更。`bg` は `palette.bg` から取得、枠色は `palette.dialog_accent` を使用。対象は `Color::Yellow` を使うウィジェットのみ（`ServerMenu`, `FilterPanel` 等は変更不要）
- `render.rs` の `draw_dialog()` で `state.palette.bg` → `&state.palette` に変更
- `render_simple_dialog()` の枠色引数を `palette.dialog_accent` に変更（UnsavedChanges 等）
- `MtimeWarningDialogWidget` の `border_color` フィールドを削除、パレットから取得に変更
- **影響ファイル:**
  - `src/ui/dialog/batch_confirm.rs` — ウィジェット変更 + 枠色・ラベル色（3箇所、Step 3 のバッジ色とは別）
  - `src/ui/dialog/confirm.rs` — ウィジェット変更 + 枠色
  - `src/ui/dialog/hunk_preview.rs` — ウィジェット変更 + 枠色
  - `src/ui/dialog/three_way_summary.rs` — ウィジェット変更 + 枠色
  - `src/ui/dialog/help.rs` — ウィジェット変更 + ラベル色
  - `src/ui/dialog/mod.rs` — large batch ガイド色（引数の `Color::Yellow` → パレット参照）
  - `src/ui/dialog/mtime_warning.rs` — ウィジェット変更 + ラベル・枠色（6箇所）、`border_color` フィールド削除
  - `src/ui/render.rs` — `draw_dialog()` のパレット伝搬（4箇所）+ ステータス色

### Step 5: diff_view 内の残り Color::Yellow をパレット化
- `content_render.rs:84` — バイナリファイルメッセージ色 → `palette.dialog_accent`
- `line_render.rs:119,275` — 確定待ちインジケータ(⏎)色 → `palette.dialog_accent`
- **影響ファイル:** `src/ui/diff_view/content_render.rs`, `src/ui/diff_view/line_render.rs`

## ✅ Tests

- [ ] ライトテーマでパレットのバッジ色が紫系（`#7c3aed`）であること（`badge_modified`, `badge_differs`）
- [ ] ダークテーマでパレットのバッジ色が黄色系（`#ebcb8b`）であること
- [ ] ライトテーマで `badge_equal` が `#16a34a`、ダークが `#a3be8c` であること
- [ ] ライトテーマで `dialog_accent` が紫系であること
- [ ] `badge_style()` がパレットの色を返すこと（tree_view テスト）
- [ ] 3way バッジスタイルがパレットの色を返すこと（既存テスト更新 + 新規）
- [ ] 全ビルトインテーマでパレット生成がパニックしないこと（既存テストで担保）
- [ ] `cargo clippy` 警告ゼロ
- [ ] **変更不要の確認**: `search.rs`, `style_utils.rs` のテスト内 `Color::Yellow` が残っていること

## 📊 Progress

| Step | Status |
|------|--------|
| Step 1: パレット拡張 | 🟢 |
| Step 2: tree_view バッジ色 | 🟢 |
| Step 3: 3way バッジ色 | 🟢 |
| Step 4: ダイアログ色 | 🟢 |
| Step 5: diff_view 残り | 🟢 |
| Tests | 🟢 |
| Commit | 🟢 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**Next:** Write tests → Implement → Commit 🚀
