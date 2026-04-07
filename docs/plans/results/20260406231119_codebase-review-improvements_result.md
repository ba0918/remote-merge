# Cycle Result: Codebase Review 改善

**Plan:** docs/plans/20260406231119_codebase-review-improvements.md
**Executed:** 2026-04-06 23:11 – 2026-04-07 (approx 1.5h)

## Refine

- Iterations: 2 (1 round + 1 fallback)
- Final verdict: PASS (max score 45)
- Phase 1.5 fallback: 1 BLOCK resolved (`expand_tilde` placement: format.rs → config.rs)
- Remaining WARN: minor (sort_children cleanup, impact file listing completeness) — addressed during implementation

## Implementation

- Steps completed: **8/8** ✅
- Commits: **3** (論理単位でグルーピング)
- Tests: **2677 passed** (baseline 2663 → +14)
- Clippy: 0 warnings
- Build: clean

## Commits

```
0f30ba6 refactor: FileNode.children を BTreeMap に変更し find_node を O(log N) 化
28ed2ba perf,refactor: SSH ゼロコピー化・context 追加・コード重複統合 (Step 6-8)
3399061 fix: セキュリティ・正確性・安全性の修正 (Step 1-4)
```

## Step Summary

| Step | 内容 | Commit |
|------|------|--------|
| 1 | ServerConfig Debug パスワードマスク | 3399061 |
| 2 | resolve_password Zeroizing 化 | 3399061 |
| 3 | undo_all() データ消失バグ修正 | 3399061 |
| 4 | unreachable!() の安全化 (4箇所) | 3399061 |
| 5 | FileTree::find_node BTreeMap 化 (O(log N)) | 0f30ba6 |
| 6 | SSH stdout ゼロコピー化 | 28ed2ba |
| 7 | handler/runtime 層に .context() 追加 | 28ed2ba |
| 8 | expand_tilde 統合 + filter_merge_candidates リネーム | 28ed2ba |

## Notes

- 実装順序は Step 1→2→3→4→6→7→8→5（Step 5 を最後に実施 — 影響範囲最大のため）
- Step 5 の BTreeMap 化では `src/local/mod.rs`, `src/ssh/tree_parser.rs`, `src/handler/`, `src/service/`, `src/runtime/`, `src/app/` 全層を更新
- `sort_children()` は BTreeMap のキー順序自動保証により no-op 化
- コードベースレビュースコア改善の期待値: 71/100 → 80+/100（B → A ランク想定）

## Resolved Issues

### Security (🔒)
- ✅ ServerConfig Debug でパスワード平文漏洩リスク排除
- ✅ resolve_password の認証情報を Zeroizing<String> で管理

### Correctness (🔍)
- ✅ undo_all() の selected_path=None 時のスタック消失バグ修正
- ✅ 到達可能な unreachable!() を graceful handling に変更（4箇所）

### Performance (⚡)
- ✅ FileTree::find_node を O(N) → O(log N) (100k ファイル規模で効果大)
- ✅ SSH stdout のメモリ2重アロケーションをゼロコピー化

### Quality (🧹)
- ✅ handler/runtime 層の主要エラーパスに context() 追加
- ✅ expand_tilde 重複統合 (config.rs に一本化)
- ✅ filter_merge_candidates 同名衝突解消
