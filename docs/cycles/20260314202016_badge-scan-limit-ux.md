# バッジスキャン上限の UX 改善

**Cycle ID:** `20260314202016`
**Started:** 2026-03-14 20:20:16
**Status:** 🟢 Done

---

## 📝 What & Why

バッジスキャンのファイル数上限（100件）が少なすぎる上、上限超過時のスキップがサイレントに行われるため、ユーザーにはバッジが `[?]` のまま放置されているように見える。上限値がハードコードされていてユーザーが制御できない点も問題。

## 🎯 Goals

- バッジスキャン上限をデフォルト 500 に引き上げる
- 上限を設定ファイルで制御可能にする（`badge_scan_max_files`、トップレベルキー）
- 上限超過時にツリー上でユーザーに視覚的に伝える（ディレクトリバッジに `[~]`）
- status_message への表示も維持（補助情報として）

## 📐 Design

### アプローチ

#### Step 1: 上限値を設定可能にする

`AppConfig` と `RawConfig` に `badge_scan_max_files` フィールドを追加。`merge_configs()` でプロジェクト設定優先のマージを行う。バリデーション関数 `validate_badge_scan_max_files()` で範囲チェック（1〜10,000）。TOML のキーは `badge_scan_max_files`（トップレベル、`max_scan_entries` と同じパターン）。

```toml
# .remote-merge.toml
badge_scan_max_files = 500  # デフォルト: 500、範囲: 1〜10,000
```

```rust
// config.rs — AppConfig
pub struct AppConfig {
    // ...
    pub badge_scan_max_files: usize,  // デフォルト: 500
}

// config.rs — RawConfig
struct RawConfig {
    // ...
    badge_scan_max_files: Option<usize>,
}

// config.rs — バリデーション
pub const DEFAULT_BADGE_SCAN_MAX_FILES: usize = 500;

pub fn validate_badge_scan_max_files(n: usize) -> Result<(), String> {
    if n == 0 || n > 10_000 {
        return Err(format!(
            "badge_scan_max_files must be between 1 and 10,000, got {}", n
        ));
    }
    Ok(())
}
```

#### Step 2: デフォルト値を 500 に引き上げ + 定数統一

`helpers.rs` の `BADGE_SCAN_MAX_FILES` 定数を削除し、`config.rs` の `DEFAULT_BADGE_SCAN_MAX_FILES`（= 500）に統一する。`start_badge_scan()` は `runtime.core.config.badge_scan_max_files` を参照するため、ハードコード定数は不要になる。既存テスト（`badge_scan_max_files_value` 等）はデフォルト値の確認に更新する。

#### Step 3: 上限超過時の視覚フィードバック

上限超過でスキャンがスキップされたディレクトリに対して：
1. ディレクトリパスを `AppState.scan_skipped_dirs: HashSet<String>` に記録する
2. `Badge` enum に `ScanSkipped` バリアントを追加（表示: `[~]`、既存の `Error` `[!]` と区別）
3. `compute_dir_badge()` で `scan_skipped_dirs` をチェックし、該当ディレクトリに `Badge::ScanSkipped` を返す
4. `tree_view.rs` で `Badge::ScanSkipped` のスタイルを追加
5. status_message にも "Badge scan skipped: {dir} ({N} files, limit: {max})" を表示

### Files to Change

```
src/config.rs                      - AppConfig に badge_scan_max_files フィールド追加
                                     RawConfig にも対応フィールド追加 + merge_configs でマージ
                                     validate_badge_scan_max_files() バリデーション関数追加
src/runtime/badge_scan/helpers.rs  - BADGE_SCAN_MAX_FILES 定数を削除（config.rs の DEFAULT_BADGE_SCAN_MAX_FILES に統一）
src/runtime/badge_scan/mod.rs      - start_badge_scan() で runtime.core.config.badge_scan_max_files を参照
                                     上限超過時に state.scan_skipped_dirs に記録
src/app/mod.rs                     - AppState に scan_skipped_dirs: HashSet<String> フィールド追加
src/app/types.rs                   - Badge enum に ScanSkipped バリアント追加（表示: [~]）
src/app/badge.rs                   - compute_dir_badge() で scan_skipped_dirs をチェック → Badge::ScanSkipped
src/theme/palette.rs               - TuiPalette に badge_scan_skipped: Color フィールド追加
src/ui/tree_view.rs                - Badge::ScanSkipped の表示スタイル追加（palette.badge_scan_skipped を参照）
```

### Key Points

- **設定値の伝播**: `start_badge_scan` は `TuiRuntime` を受け取るため、`runtime.core.config.badge_scan_max_files` から上限値を取得する。呼び出し側の変更は不要
- **`scan_skipped_dirs` のクリア**: サーバ切替時に `cancel_all_badge_scans` でクリアする。また `reset_diff_state()` でもクリアする
- **`[~]` バッジの意味**: 「スキャン不完全」を示す。`Badge::Error` の `[!]` とは区別する。ユーザーがディレクトリを折りたたんで再展開すれば、手動でスキャンをやり直せる
- **デフォルト 500 の根拠**: testenv の構成では1ディレクトリ最大500ファイル程度。実用プロジェクトの大半をカバー
- **Badge::ScanSkipped**: 新しい Badge バリアント。既存の `Badge::Error` (`[!]`) と衝突しないよう `[~]` を使用する

## ✅ Tests

### Domain (純粋関数)
- [ ] `DEFAULT_BADGE_SCAN_MAX_FILES` が 500 であること
- [ ] 設定ファイルから `badge_scan_max_files` を読み込めること
- [ ] 設定ファイルに `badge_scan_max_files` がない場合デフォルト 500 が使われること
- [ ] `validate_badge_scan_max_files(0)` がエラーを返すこと
- [ ] `validate_badge_scan_max_files(10_001)` がエラーを返すこと
- [ ] `validate_badge_scan_max_files(500)` が OK を返すこと
- [ ] `scan_skipped_dirs` にディレクトリが記録されること
- [ ] `compute_dir_badge()` で `scan_skipped_dirs` 内のディレクトリが `Badge::ScanSkipped` になること
- [ ] `Badge::ScanSkipped` の表示文字列が `[~]` であること

### Handler / Runtime
- [ ] 上限超過時に `scan_skipped_dirs` にディレクトリが追加されること
- [ ] サーバ切替時（`reset_diff_state`）に `scan_skipped_dirs` がクリアされること
- [ ] `cancel_all_badge_scans` で `scan_skipped_dirs` がクリアされること
- [ ] `runtime.core.config.badge_scan_max_files` の値が正しく参照されること

### UI
- [ ] `Badge::ScanSkipped` のスタイルが tree_view に追加されていること

## 🔒 Security

- 設定値はユーザーのローカル設定ファイルから取得（外部入力ではない）
- `validate_badge_scan_max_files()` で範囲制限（1〜10,000）を強制し、巨大値による OOM を防止

## 📊 Progress

| Step | Description | Status |
|------|-------------|--------|
| 1 | 設定ファイルに `badge_scan_max_files` 追加 | 🟢 |
| 2 | デフォルト値を 500 に変更 | 🟢 |
| 3 | 上限超過時の視覚フィードバック（`scan_skipped_dirs` + バッジ表示） | 🟢 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**Next:** Write tests → Implement → Commit with `smart-commit` 🚀
