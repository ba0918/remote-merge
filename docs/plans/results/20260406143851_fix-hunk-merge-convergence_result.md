# Cycle Result: Hunk Merge 分割適用時の収束問題

**Plan:** docs/plans/20260406143851_fix-hunk-merge-convergence.md
**Executed:** 2026-04-06

## Refine
- Iterations: 2
- Final verdict: PASS
- 全6観点（Feasibility, Security, Performance, Architecture, Completeness, Alternatives）PASS
- Codex セカンドオピニオンにより当初の再diff方式から single-pass 方式に全面刷新

## Implementation
- Steps completed: 5/5（Step 1-5 を統合実装）
- Files changed: 2 (engine.rs, merge_flow.rs)
- Tests added: 15
- Tests removed: 7（旧 apply_selected_hunks テスト）
- Commits: 2

## Commits
- e219da7 feat: hunk merge に single-pass 方式を導入し収束問題を解決
- 51a34c8 docs: hunk merge 収束問題の計画ステータスを 🟢 Complete に更新

## Changes Summary

### src/diff/engine.rs
- `apply_selected_hunks_single_pass()` 新設 — all_lines を O(n) で1回走査して最終テキストを構築
- 旧 `apply_selected_hunks()` 削除 — 降順ループ方式は行数増減時に位置ズレが発生する根本原因
- 15 テスト追加（収束テスト2件、境界ケース、方向、trailing newline 等）

### src/service/merge_flow.rs
- 全 hunk フォールバック: 全 hunk 指定時はソーステキストをそのまま使用
- 部分適用: `apply_selected_hunks_single_pass()` を使用
- インデックス範囲チェックをサービス層で実施

## Notes
- TUI 側の `hunk_ops.rs` は変更なし（`apply_hunk_to_text()` は1つずつ適用する用途で引き続き使用）
- Codex レビューで「根本原因の分析欠落」「再diff方式の矛盾」を指摘され、Alternative C（single-pass）に方針転換した
