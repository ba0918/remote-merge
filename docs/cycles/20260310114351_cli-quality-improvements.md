# CLI 品質改善（merge . / バイナリ diff / help 説明追加）

**Cycle ID:** `20260310114351`
**Started:** 2026-03-10 11:43:51
**Status:** 🟡 Planning

---

## 📝 What & Why

CLI 検証（status/diff/merge 全コマンドを多様なパターンで実行）で発見した問題点を修正する。
`merge .` の出力不具合、バイナリファイルの文字化け、help テキストの説明不足の3件を対応。

## 🎯 Goals

- `merge .` / `diff .` がルート全ファイルを正しく展開して表示する
- NUL バイトを含まないバイナリファイルの diff でも文字化けを出さない
- CLI の --help 出力で全オプションに説明がある

## 📐 Design

### Step 1: `"."` パス解決修正（path_resolver.rs）

**問題:** `resolve_target_files_from_statuses()` で `"."` が `is_directory_in_tree()` で `false` になり、ファイルとしてそのまま渡される。`resolve_target_files()` も同様。

**修正方針:** `"."` を `""` に変換するのではなく、`"."` / `"./"` を検出したら **全ファイル返却パス** にジャンプする。
空文字（`"/"` のトリム結果）も同様に全ファイル扱いにする。

具体的には両関数の paths ループの前に以下を追加：

```rust
// "." or "./" → treat as "all files" (same as empty paths)
let has_root_marker = paths.iter().any(|p| {
    let n = p.trim_end_matches('/');
    n == "." || n.is_empty()
});
if paths.is_empty() || has_root_marker {
    // 全ファイル返却（既存ロジック）
}
```

**影響範囲:** `resolve_target_files` と `resolve_target_files_from_statuses` の2関数。
CLI の diff (`cli/diff.rs:80`) と merge (`cli/merge.rs:131`) の両方がこの関数を使うため、
path_resolver の修正だけで `diff .` / `merge .` が両方直る。

### Step 2: バイナリ判定強化（engine.rs `is_binary()`）

**問題:** `is_binary()` は NUL バイト検出のみ。NUL なしの不正 UTF-8 バイト列（画像、コンパイル済みバイナリ等）がテキストとして diff され文字化けする。

**修正方針:** `is_binary()` 自体を拡張して、NUL バイト検出に加え **不正 UTF-8 シーケンス検出** を行う。
`engine.rs` に閉じた修正で、バイナリ判定ロジックが一箇所に集約される。

```rust
pub fn is_binary(content: &[u8]) -> bool {
    let check_len = content.len().min(8192);
    let slice = &content[..check_len];
    // NUL バイト検出
    if slice.contains(&0) {
        return true;
    }
    // 不正 UTF-8 シーケンス検出（lossy 変換不要で効率的）
    std::str::from_utf8(slice).is_err()
}
```

**代替案検討:**
- `from_utf8_lossy` + `U+FFFD` チェック（CLI 側） → バイナリ判定が engine.rs と cli/diff.rs に分散。設計原則「責務混在禁止」に反する
- 拡張子ベース判定 → 拡張子の網羅が困難、false positive のリスク
- `file` コマンド呼び出し → 外部依存、クロスプラットフォーム問題
- **`from_utf8` によるバイナリ判定（採用）** → 責務集約、lossy 変換不要で高効率、1行追加のみ

**注意:** 正常な UTF-8 テキスト（日本語含む）は `from_utf8` が `Ok` を返すため影響なし。

### Step 3: help テキスト改善（main.rs）

**問題:** 以下のオプション/引数に `///` doc comment（clap の help 属性）がない。

**対象一覧（全サブコマンド横断）:**

| サブコマンド | 対象 | 現状 |
|------------|------|------|
| `status` | `--format` | help なし |
| `status` | `--summary` | help なし |
| `diff` | `--format` | help なし |
| `diff` | `--max-lines` | help なし |
| `diff` | `<PATHS>` | help なし |
| `merge` | `--dry-run` | help なし |
| `merge` | `--force` | help なし |
| `merge` | `<PATHS>` | help なし |

### Files to Change

```
src/
  service/
    path_resolver.rs — "." / "./" を全ファイルとして解決するロジック追加
  diff/
    engine.rs — is_binary() に不正 UTF-8 判定追加
  main.rs — clap 引数の help 属性追加（status/diff/merge 横断）
```

### Key Points

- **path_resolver.rs**: `"."` は `trim_end_matches('/')` 後も `"."` のまま。`"."` を全ファイル返却パスへジャンプさせる
- **engine.rs**: `is_binary()` 自体を拡張。CLI 層には変更なし。責務が engine.rs に集約される
- **main.rs**: doc comment を追加するだけ。既存の動作は変わらない
- **cli/diff.rs**: 変更不要（is_binary の判定強化により自動的に文字化け防止）
- **cli/merge.rs**: 変更不要（path_resolver の修正により自動的に `merge .` 対応）

## ✅ Tests

### path_resolver.rs
- [ ] `resolve_target_files` で `"."` を渡すと全ファイルが返る
- [ ] `resolve_target_files` で `"./"` を渡すと全ファイルが返る
- [x] `resolve_target_files_from_statuses` で `"."` を渡すと全 status ファイルが返る
- [x] `resolve_target_files_from_statuses` で `"./"` を渡すと全 status ファイルが返る
- [x] 既存のディレクトリパス（`"src/"`, `"src"`）の動作が壊れていないこと（既存テスト通過で確認）

### engine.rs（is_binary 強化）
- [x] NUL なしの不正 UTF-8 バイト列が binary 判定される
- [x] 正常な UTF-8 テキストが binary 判定されないこと
- [x] 日本語テキストが binary 判定されないこと
- [x] 空バイト列が binary 判定されないこと（既存テスト）
- [x] NUL バイト含むデータが binary 判定されること（既存テスト）

### main.rs（help）
- [x] `status --help` に全オプションの説明が表示される（目視確認）
- [x] `diff --help` に全オプションの説明が表示される（目視確認）
- [x] `merge --help` に全オプションの説明が表示される（目視確認）

## 📊 Progress

| Step | 内容 | Status |
|------|------|--------|
| 1 | path_resolver.rs — `"."` パス解決修正 | 🟢 |
| 2 | engine.rs — is_binary() 不正 UTF-8 判定追加 | 🟢 |
| 3 | main.rs — help 説明追加（status/diff/merge） | 🟢 |
| Commit | | 🟡 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done
