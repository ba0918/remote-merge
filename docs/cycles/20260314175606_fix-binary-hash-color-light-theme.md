# ハードコード色のパレット集約（ライトテーマ視認性修正）

**Cycle ID:** `20260314175606`
**Started:** 2026-03-14 17:56:06
**Status:** 🟢 Complete

---

## 📝 What & Why

lightテーマで以下の表示が背景と同化して見えない:
- バイナリファイルのSHA-256ハッシュ値（`Color::White` → 白背景に白文字）
- サーバ選択ダイアログの非アクティブテキスト（`Color::DarkGray` → 薄い）
- 各種ダイアログのボタン・ラベル色（`Color::Cyan`, `Color::Green` 等 → lightで薄い）

`src/ui/` 全体に散在するハードコード `Color::` をパレット参照に一括変更し、light/dark 両テーマで視認性を保証する。

## 🎯 Goals

- ハードコード色を全てパレット参照に置換
- light/dark 両テーマで全UI要素の視認性を確保
- 新パレットフィールドを最小限に抑える（既存フィールドを最大限再利用）

## 📐 Design

### 新パレットフィールド（palette.rs に追加）

既存フィールドでカバーできないセマンティック色を追加:

```rust
// -- セマンティック色 --
/// 肯定色（接続OK, identical, Yes ボタン等）
pub positive: Color,        // light: green-700, dark: Green
/// 否定色（接続NG, different, No ボタン等）
pub negative: Color,        // light: red-700, dark: Red
/// 情報色（ダイアログ枠, ヒント, リンク等）
pub info: Color,            // light: blue-700, dark: Cyan
/// 控えめテキスト（非アクティブ, 補足, スクロール続き等）
pub muted: Color,           // light: gray-500, dark: DarkGray
/// 警告色（バッチ件数, mtime 不一致等）
pub warning: Color,         // light: amber-700, dark: Yellow
```

### Why semantic colors

- `Color::Green` が「接続OK」「identical」「Yes ボタン」「After ラベル」など全く別の文脈で使われている
- セマンティック名にすることで意図が明確になり、テーマ側で適切な色を選べる
- 5フィールド追加で 50箇所以上のハードコードを集約

### Files to Change

```
src/
  theme/palette.rs              - 5フィールド追加 + light/dark 色定義
  ui/render.rs                  - 接続インジケータ, DIFF ONLY/SCANNING バッジ, Agent状態, ダイアログ枠, WriteConfirmation/Info/Progress ダイアログ色
  ui/tree_view.rs               - ref_only グレイ, シンボリックリンク [L], 検索ハイライト
  ui/diff_view/content_render.rs - バイナリ hash/status, シンボリックリンク表示
  ui/diff_view/search.rs        - 変更不要（Color::Black は特殊ケースとして維持）
  ui/dialog/mod.rs              - ガイドボタン色 (Yes/No/OK/Cancel) ※ガイド関数にpalette引数追加が必要
  ui/dialog/confirm.rs          - 確認メッセージテキスト
  ui/dialog/batch_confirm.rs    - バッチメッセージ, センシティブ警告, ファイルリスト
  ui/dialog/help.rs             - ヘルプダイアログ枠, キーバインド表示
  ui/dialog/pair_server_menu.rs - サーバ選択全般（枠, 列, 選択状態, ペア表示, フッター）
  ui/dialog/server_menu.rs      - サーバメニュー（枠, 選択/接続中/未選択）
  ui/dialog/filter_panel.rs     - フィルター（枠 Color::Magenta, 選択, 無効, フッター）
  ui/dialog/mtime_warning.rs    - mtime警告（FILE DELETED, reload/cancel）
  ui/dialog/hunk_preview.rs     - プレビュー（テキスト, パス, Before/After）
  ui/dialog/three_way_summary.rs - 3way サマリー（差分なし, カーソル, 行番号, カラム, ボタン）
```

### 置換マッピング

| ハードコード色 | → パレットフィールド | 用途 |
|--------------|---------------------|------|
| `Color::Green` | `palette.positive` | 接続OK, identical, Yes, After, 選択中 |
| `Color::Red` | `palette.negative` | 接続NG, different, No/Cancel, Before, 警告 |
| `Color::Cyan` | `palette.info` | ダイアログ枠, ヒント, リンク, [L]バッジ |
| `Color::DarkGray` | `palette.muted` | 非アクティブ, 補足, ref_only, スクロール続き |
| `Color::Yellow` | `palette.warning` | バッチ件数警告 |
| `Color::White` | `palette.fg` | テキスト本文（テーマ前景色） |
| `Color::Blue` | `palette.info` | 3way カラム色（Blue → info に統合） |
| `Color::Magenta` | `palette.badge_right_only` (既存) or `palette.dialog_accent` | フィルター枠, 3way カラム色 |
| `Color::Black` | 検索ハイライトは `Color::Black` のまま（背景が黄色で固定） | 特殊ケース |

### dialog/mod.rs のガイド関数シグネチャ変更

`confirm_cancel_guide()`, `ok_guide()`, `cancel_guide()` は現在パレット引数を取らず `Color::Green`/`Color::Red`/`Color::Cyan` をハードコードしている。
これらを `&TuiPalette` 引数を追加する形に変更する必要がある。

```rust
// Before
pub fn confirm_cancel_guide(suffix: Option<(&str, Color)>) -> Line<'static>
pub fn ok_guide() -> Line<'static>
pub fn cancel_guide() -> Line<'static>

// After
pub fn confirm_cancel_guide(palette: &TuiPalette, suffix: Option<(&str, Color)>) -> Line<'static>
pub fn ok_guide(palette: &TuiPalette) -> Line<'static>
pub fn cancel_guide(palette: &TuiPalette) -> Line<'static>
```

呼び出し元（render.rs, 各ダイアログ）も全て更新が必要。

### render.rs の残存ハードコード色

以下のダイアログ描画関数内にもハードコード色が残っている:
- `render_info_dialog`: `Color::Cyan`（枠）, `Color::White`（メッセージ）
- `render_progress_dialog`: `Color::Cyan`（枠, プログレスバー）, `Color::DarkGray`（パス表示）
- `render_simple_dialog`: `Color::White`（メッセージ）, `Color::Green`（WriteConfirmation の枠色）

これらも全てパレット参照に変更する。`render_simple_dialog` と `render_info_dialog` に palette 引数を追加。

### hunk_preview.rs の Before/After 背景色

```rust
// これらは bg ベースで blend すべき
Color::Rgb(30, 0, 0)  → palette.diff_delete_bg（既存）
Color::Rgb(0, 30, 0)  → palette.diff_insert_bg（既存）
```

### 実装順序

影響範囲が大きいので、3ステップに分割:

## 📊 Progress

| Step | Description | Files | Status |
|------|-------------|-------|--------|
| 1 | palette.rs に5フィールド追加 + テスト | palette.rs | 🟢 |
| 2 | content_render.rs + render.rs + tree_view.rs（非ダイアログ） | 3 files | 🟢 |
| 3 | dialog/mod.rs ガイド関数シグネチャ変更 + dialog/*.rs 全般 + render.rs ダイアログ描画関数 | 11 files | 🟢 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

## ✅ Tests

### Step 1: palette.rs
- [ ] lightテーマで positive が green-700 (コントラスト確保)
- [ ] darkテーマで positive が Green 相当
- [ ] lightテーマで negative が red-700
- [ ] lightテーマで info が blue-700
- [ ] lightテーマで muted が gray-500
- [ ] lightテーマで warning が amber-700
- [ ] 全ビルトインテーマで5フィールドが Rgb であること

### Step 2: 非ダイアログ UI
- [ ] render_binary の label/value/status がパレット経由
- [ ] render_symlink_diff の arrow/status がパレット経由
- [ ] tree_view の ref_only DarkGray がパレット経由
- [ ] tree_view の [L] Cyan がパレット経由

### Step 3: ダイアログ
- [ ] pair_server_menu の列色がパレット経由
- [ ] 全ダイアログの枠色がパレット経由
- [ ] confirm_cancel_guide/ok_guide/cancel_guide がパレット引数を受け取ること
- [ ] render_info_dialog/render_progress_dialog/render_simple_dialog がパレット経由
- [ ] Color::White の使用が src/ui/ 本番コードから消えていること（検索ハイライトの Black 以外）
- [ ] ガイド関数の呼び出し元が全て更新されていること

---

**Next:** Write tests → Implement → Commit 🚀
