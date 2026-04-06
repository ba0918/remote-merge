# Fix: Hunk Merge 分割適用時の収束問題

**Created:** 2026-04-06
**Status:** 🔵 Implementing

## 背景

Hunk merge を分割適用（例: 5個中3個だけ適用 → 残り2個を適用）すると、
最初の適用で行番号がズレて残りの hunk のコンテキストが変わり、
再 diff 時に新しい hunk 構成になる → 全部適用しても残差が出るケースがある。

### 根本原因

現行の `apply_selected_hunks()` は降順ループで `apply_hunk_to_text()` を繰り返し呼ぶ。
`apply_hunk_to_text()` は `old_start`/`new_start`（初回 diff 時に計算済み）を使ってテキスト上の位置を決定する。
降順適用は「前方の hunk に影響しない」前提だが、**hunk が行数を増減させると後続の適用で位置がズレる**。

例: hunk 1（下方）が5行削除 → テキストが5行短くなる → hunk 0 の `new_start` はもう正しくない。
降順なので hunk 0 を後から適用するが、hunk 0 の `new_start` は**元テキスト基準**のまま。

### テスト結果（ユーザー報告）

| テスト項目 | 結果 |
|-----------|------|
| dry-run プレビュー | OK |
| 部分 hunk 適用 (0,1,2) | OK |
| 適用後の diff 再計算 | OK |
| 残り hunk 一括適用 | **要注意** — hunk 境界ズレで取りこぼし |
| ファイル全体マージ | OK |
| バックアップ作成 | OK |
| 複数ファイル同時マージ | OK |

## 対策方針

レビューで指摘された **Alternative C（single-pass 方式）** を採用する。

### なぜ single-pass か

現行方式の問題は「テキストを変更 → 変更済みテキストに次の hunk を適用」というループ構造にある。
テキストを中間状態にするから行番号がズレる。

Single-pass 方式は `all_lines`（diff の全行リスト）を **1回だけ走査** して最終テキストを構築する：
- **選択された hunk の行** → 変更を適用（keep_tag の行を出力、replace_tag の行をスキップ）
- **選択されなかった hunk の行** → 元テキストを維持（replace_tag の行を出力、keep_tag の行をスキップ）
- **Equal 行** → そのまま出力

これなら行番号ズレは**原理的に発生しない**。O(n) で1パス、再diff不要、content matching 不要。

### 全 hunk 一括適用のフォールバック

全 hunk が指定された場合はソーステキストをそのまま採用する（サービス層で判定）。
Single-pass でも正しい結果になるが、不要な計算を避けるシンプルな最適化。

## 実装ステップ

### Step 1: `apply_selected_hunks_single_pass()` 新設
**ファイル:** `src/diff/engine.rs`
**変更内容:**

新しい純粋関数を追加。既存の `apply_selected_hunks()` は残して互換性を維持し、
新関数で置き換えた後に旧関数を削除する。

```rust
/// all_lines を1回走査して、選択された hunk のみを適用した最終テキストを構築する。
///
/// - `all_lines`: compute_diff() で得た全行リスト
/// - `merge_hunks`: compute_diff() で得た操作用ハンク一覧（コンテキスト0行）
/// - `selected`: 適用する hunk インデックスの集合（0-based）
/// - `direction`: マージ方向
///
/// **純粋関数** — 副作用なし。O(n) single-pass。
pub fn apply_selected_hunks_single_pass(
    all_lines: &[DiffLine],
    merge_hunks: &[DiffHunk],
    selected: &HashSet<usize>,
    direction: HunkDirection,
    target_trailing_newline: bool,
) -> String
```

**アルゴリズム:**
```
1. selected_ranges = selected な hunk の line_range を HashSet に展開
2. direction から (keep_tag, replace_tag) を決定
   - LeftToRight: keep_tag=Delete, replace_tag=Insert
   - RightToLeft: keep_tag=Insert, replace_tag=Delete
3. for (i, line) in all_lines.iter().enumerate():
   in_selected = selected_ranges.contains(i)
   match line.tag:
     Equal → result.push(line.value)
     keep_tag →
       if in_selected → result.push(line.value)  // 変更を適用
       else → skip                                  // 元テキスト維持
     replace_tag →
       if in_selected → skip                        // 変更を適用（元行をスキップ）
       else → result.push(line.value)              // 元テキスト維持
4. return result.join("\n") + trailing newline handling
```

**ポイント:**
- `all_lines` は diff の「両側」を表現している。Equal は共通行、Delete は左のみ、Insert は右のみ
- 「選択された hunk を適用」= keep_tag を出力 + replace_tag をスキップ
- 「選択されなかった hunk を維持」= replace_tag を出力 + keep_tag をスキップ
- trailing newline は `target_trailing_newline: bool` パラメータで呼び出し元から渡す（呼び出し元で `target_text.ends_with('\n')` を計算）

### Step 2: テスト追加
**ファイル:** `src/diff/engine.rs`（tests モジュール）

**テストマトリクス:**

| # | テストケース | 検証内容 |
|---|-------------|---------|
| 1 | 空 indices → 元テキスト維持 | selected が空なら target テキストと同一 |
| 2 | 単一 hunk 適用 | 1つだけ適用して他は元のまま |
| 3 | 全 hunk 適用 → source テキストと一致 | 完全マージと同等 |
| 4 | 部分適用 → 残り適用 → 収束 | **核心テスト**: hunk 0,1 を適用後、残り hunk を適用して source と一致 |
| 5 | 行数増減のある hunk | Insert のみ/Delete のみの hunk を部分適用 |
| 6 | 隣接する2 hunk 中1つだけ適用 | 隣接 hunk の境界処理 |
| 7 | ファイル先頭の hunk | old_start=0 / new_start=0 のケース |
| 8 | ファイル末尾の hunk | 最終行を含む hunk |
| 9 | 重複インデックス | HashSet なので自然に重複排除 |
| 10 | 範囲外インデックス | エラーハンドリング |
| 11 | RightToLeft 方向 | 逆方向でも正しく動作 |
| 12 | trailing newline 有無 | 末尾改行の保持/非保持 |

**特に重要 — 収束テスト (#4):**
```rust
// Step A: hunk 0,1 を適用
let partial = apply_selected_hunks_single_pass(lines, hunks, &{0,1}, dir);
// Step B: partial を元に再 diff → 残りの hunk を全適用
let diff2 = compute_diff(source, &partial);
// diff2 の全 hunk を適用 → source と一致すべき
// ※ single-pass なら Step A の結果が正確なので Step B も正確に動く
```

### Step 3: サービス層の統合
**ファイル:** `src/service/merge_flow.rs`
**変更内容:**

`execute_hunk_merge()` 内で：
1. 全 hunk フォールバック判定（dedup 後の indices 数 == merge_hunks.len()）
2. 部分適用は `apply_selected_hunks_single_pass()` を使用
3. `apply_selected_hunks()` の呼び出しを新関数に置き換え

```rust
// merge_flow.rs の execute_hunk_merge() 内
let unique_indices: HashSet<usize> = hunk_indices.iter().copied().collect();

let merged_text = if unique_indices.len() >= merge_hunks.len() {
    // 全 hunk → ソーステキストをそのまま使用
    source_text.to_string()
} else {
    // 部分適用 → single-pass
    let trailing_nl = target_text.ends_with('\n');
    apply_selected_hunks_single_pass(lines, merge_hunks, &unique_indices, hunk_dir, trailing_nl)
};
```

### Step 4: 旧関数の削除 + TUI 側の確認
**ファイル:** `src/diff/engine.rs`, `src/app/hunk_ops.rs`
**変更内容:**

1. 旧 `apply_selected_hunks()` を削除（呼び出し元を全て新関数に移行後）
2. `hunk_ops.rs` の `apply_hunk_to_text()` 呼び出し（L122, L163）は**変更不要**
   - TUI は1つずつ hunk を適用するので single-pass 不要
   - `apply_hunk_to_text()` 自体は変更しない
3. 旧テストを新関数のテストに置き換え

### Step 5: trailing newline のエッジケース対応
**ファイル:** `src/diff/engine.rs`
**変更内容:**

Single-pass で `result.join("\n")` した後の trailing newline 処理：
- `target_trailing_newline: bool` パラメータで呼び出し元から判定済みの値を受け取る
- true なら `result.join("\n")` の末尾に `\n` を追加、false ならそのまま
- 既存の `apply_hunk_to_text()` と同じロジック（L278-282）を踏襲

## 影響範囲

| ファイル | 変更内容 |
|---------|---------|
| `src/diff/engine.rs` | `apply_selected_hunks_single_pass()` 新設、旧 `apply_selected_hunks()` 削除 |
| `src/service/merge_flow.rs` | 全 hunk フォールバック + 新関数呼び出し |
| `src/diff/engine.rs` (tests) | 12 テストケース追加、旧テスト置き換え |
| `src/app/hunk_ops.rs` | **変更なし**（`apply_hunk_to_text()` はそのまま） |

## リスク

| リスク | 対策 |
|--------|------|
| trailing newline の扱い | 既存の `apply_hunk_to_text` と同じロジックを適用。テスト #12 で検証 |
| 既存テストの破壊 | 旧関数を残して段階的に移行。旧テストは新テストで網羅した後に削除 |
| merge_hunks のコンテキスト=0 前提 | `compute_diff()` の merge_hunks は常にコンテキスト0で生成される（build_hunks(lines, 0)）ことを確認済み |

## 旧計画との差分（レビュー反映）

| 旧計画 | 新計画 | 理由 |
|--------|--------|------|
| 再 diff + content matching | **Single-pass 方式** | O(n)、再diff不要、matching不要、行番号ズレが原理的に起きない |
| `apply_selected_hunks()` を改修 | **新関数を新設** | 責務分離（SRP）、旧関数との互換性維持 |
| テスト5件 | **テスト12件** | 境界ケース（隣接hunk、先頭/末尾、行数増減）を網羅 |
| 影響範囲に `hunk_ops.rs` なし | **明記（変更なし）** | TUI 側への影響がないことを明示 |
| 根本原因の分析なし | **根本原因を明記** | 降順適用でも行数増減で位置がズレるメカニズムを説明 |
