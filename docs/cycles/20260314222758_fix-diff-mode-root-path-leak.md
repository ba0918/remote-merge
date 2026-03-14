# DIFFモード相対パスroot_dir露出バグ修正

**Cycle ID:** `20260314222758`
**Started:** 2026-03-14 22:27:58
**Status:** 🟢 Done

---

## 📝 What & Why

DIFFモード（Shift+F）でroot_dirが相対パス（`./testenv/data/local`等）の場合、`resolve_scan_roots`がパスを正規化せずに返すため、`walk_single_root`の`strip_prefix`が失敗し、`"."`から始まるプロジェクト構造がファイルツリーに露出する。共有画面やスクリーンショットでローカルのディレクトリ構造が漏洩するセキュリティリスク。

## 🎯 Goals

- 相対パスの`root_dir`でもDIFFモードが正しく動作する
- ファイルツリーに`"."`やプロジェクトルートの絶対パスが混入しない
- 既存の`include`フィルター付きスキャンに影響を与えない

## 🔍 Root Cause Analysis

### 問題のフロー

1. `config.toml` に `root_dir = "./testenv/data/local"` が設定
2. `config.rs:522` で `expand_tilde()` のみ適用 → `PathBuf::from("./testenv/data/local")` のまま
3. DIFFモード起動 → `scanner.rs:202` で `config.local.root_dir.clone()` を取得
4. `scan_local_tree_recursive_with_include()` が呼ばれる（`local/mod.rs:223`）
5. `resolve_scan_roots(root, &[])` → **include が空なので `vec![root.to_path_buf()]` を正規化なしで返す**（`local/mod.rs:145`）
6. `canonical_root = root.canonicalize()` → `/home/user/project/testenv/data/local`（正規化済み）
7. `walk_single_root(scan_root="./testenv/data/local", original_root="/home/.../testenv/data/local")`
8. WalkDir エントリ: `"./testenv/data/local/app/file.php"`
9. `strip_prefix("/home/.../testenv/data/local")` → **失敗**（相対 vs 絶対パス不一致）
10. `unwrap_or(entry.path())` → `"./testenv/data/local/app/file.php"` がそのまま相対パスとして使われる
11. `build_local_tree_from_flat` が `"."` → `"testenv"` → `"data"` → `"local"` → ... とツリー化
12. **UIに `"."` ディレクトリとプロジェクト構造が露出**

### 問題の本質

`resolve_scan_roots()` の `include` 空ケースが `root.to_path_buf()` をそのまま返す一方、`scan_local_tree_recursive_with_include()` は `canonical_root` で `strip_prefix` しようとする。パス形式の不一致（相対 vs 絶対）がバグの根因。

## 📐 Design

### 修正方針

**方針A（推奨）**: `resolve_scan_roots()` で `include` が空の場合も `canonicalize()` を適用する。

これにより `scan_root` と `canonical_root` の両方が絶対パスになり、`strip_prefix` が正しく動作する。

```rust
// Before (local/mod.rs:144-146)
if include_paths.is_empty() {
    return vec![root.to_path_buf()];  // 正規化なし
}

// After
if include_paths.is_empty() {
    return match root.canonicalize() {
        Ok(p) => vec![p],
        Err(_) => vec![root.to_path_buf()],
    };
}
```

### Files to Change

```
src/
  local/mod.rs       - resolve_scan_roots() の include 空ケースで canonicalize() 適用
  local/mod.rs       - テスト追加: 相対パスでのスキャン結果検証
```

### Key Points

- **最小変更**: `resolve_scan_roots()` の1箇所のみ修正。他のコードパスに影響なし
- **include 非空ケースは既に安全**: `resolve_scan_roots()` 内で `joined.canonicalize()` 済み
- **canonicalize 失敗時のフォールバック**: 元のパスをそのまま返す（既存動作と同じ）。ただし、この場合でも `walk_single_root` の `strip_prefix` 失敗フォールバック（315-318行目の `unwrap_or(entry.path())`）がパス漏洩を引き起こす可能性がある。`canonicalize` が失敗するケースは root_dir が存在しない場合に限られ、その場合は `scan_local_tree_recursive_with_include` の冒頭（229-233行目）で `root.exists()` チェックにより早期エラーとなるため、実際にはフォールバックパスに到達しない
- **config.rs 側での正規化は不採用**: config 読み込み時に canonicalize すると、テスト環境や CI で CWD 依存の問題が生じる可能性がある

### 検討した代替案

1. **config.rs の `expand_tilde()` 直後に `canonicalize()` を適用** — config 読み込みは起動時1回のため CWD は安定しているが、テストで任意の相対パスを config に設定するケースが制約される。また影響範囲が config 全体に波及するため最小変更の原則に反する
2. **`walk_single_root` 側で `scan_root` を `canonicalize` する** — `walk_single_root` は `resolve_scan_roots` の結果を受け取る設計であり、`resolve_scan_roots` が canonicalize 済みパスを返す仕様にする方が責務が明確。呼び出し元と呼び出し先の両方で canonicalize すると二重処理になり、パス正規化の責務が分散する
3. **`scan_local_tree_recursive_with_include` で `scan_roots` と `canonical_root` を同じソースから導出** — `resolve_scan_roots` が canonicalize 済みパスを返すようにすれば、`canonical_root = root.canonicalize()` との整合性が自動的に保たれるため、本修正はこのアプローチと等価

### 既存テストへの影響

- `test_resolve_scan_roots_empty_includes`（657-661行目）: `/tmp` を絶対パスで渡しているため、`canonicalize()` 後も同じ値を返す（Linux 環境）。macOS では `/tmp` → `/private/tmp` に解決される可能性があるが、CI は Linux のため影響なし。念のためテストを `TempDir` ベースに書き換える

## ✅ Tests

- [x] `test_resolve_scan_roots_relative_path_canonicalized` — 相対パスのルートが正規化されること
- [x] `test_resolve_scan_roots_empty_includes`（既存テスト修正） — `TempDir` ベースに書き換え、`canonicalize()` 適用後も正しく動作すること
- [x] `test_scan_local_tree_recursive_with_relative_root` — 相対パス root_dir でスキャンしてもツリーに "." が混入しないこと
- [x] `test_scan_local_tree_recursive_relative_root_no_project_path_leak` — スキャン結果のパスにプロジェクトルート構造が含まれないこと
- [x] `test_scan_local_tree_recursive_relative_root_strip_prefix_safety` — `strip_prefix` 結果にフルパスや `".."` セグメントが含まれないことを検証
- [x] 既存テスト全通過（`cargo nextest run`）

## 🔒 Security

- [x] パス露出の根本原因特定済み
- [x] 修正後、相対パス root_dir で DIFF モードを実行し `.` が表示されないことを確認
- [x] `strip_prefix` 失敗時にフルパスが漏洩しないことを確認

## 📊 Progress

| Step | Status |
|------|--------|
| Tests | 🟢 |
| Implementation | 🟢 |
| Commit | 🟢 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**Next:** Write tests → Implement → Commit with `smart-commit` 🚀
