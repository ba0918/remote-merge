# Codebase Review 改善

**Cycle ID:** `20260406231119`
**Started:** 2026-04-06 23:11:19
**Status:** 🔵 Implementing

---

## 📝 What & Why

コードベースレビュー (71/100 B ランク) で発見された Critical・Warning 級の問題を修正し、コード品質を A ランク (80+) に引き上げる。セキュリティ・パフォーマンス・正確性・衛生の4領域を優先順位に従って改善する。

## 🎯 Goals

- セキュリティ: パスワード漏洩リスクの排除
- パフォーマンス: 100k ファイル規模でのホットパス最適化
- 正確性: 本番 panic の地雷除去、undo データ消失バグ修正
- 衛生: コード重複の統合、`.context()` 改善

---

## 📐 Design

### Step 1: ServerConfig Debug パスワード漏洩修正 (Security Critical)

```
src/config.rs
  - ServerConfig の #[derive(Debug)] を手動 Debug impl に変更
  - password フィールドを [REDACTED] 表示
```

**Key Points:**
- `#[derive(Debug, Clone)]` → `#[derive(Clone)]` + 手動 `impl Debug`
- `password` のみマスク、他フィールドはそのまま
- テスト: Debug 出力にパスワード文字列が含まれないことを検証

### Step 2: resolve_password の Zeroizing 化 (Security Critical)

```
src/ssh/client.rs
  - resolve_password() の戻り値を Option<(Zeroizing<String>, PasswordSource)> に変更
  - 環境変数から取得した val も Zeroizing::new() でラップ
  - config_password の to_string() も Zeroizing::new() でラップ
  - 呼び出し元の authenticate_password も Zeroizing<String> を受け取るよう調整
```

**Key Points:**
- `zeroize` クレートは既に依存にあるため追加不要
- 環境変数値・config 値の両方を `Zeroizing::new()` でラップ、ドロップ時にメモリゼロ化保証
- テスト: `resolve_password` の戻り値型が `Zeroizing<String>` であることのコンパイル時検証 + 環境変数/config 両パスで Zeroizing が返ることのランタイムテスト

### Step 3: undo_all() データ消失バグ修正 (Logic Critical)

```
src/app/undo.rs
  - undo_all() で selected_path が None の場合のスタック消失を修正
```

**Key Points:**
- 現状のバグ: `pop_front()` → `clear()` → `selected_path` チェックの順で、`selected_path = None` のとき `initial` がスタックから取り出され `clear()` 後に使われず消失する
- 修正: `selected_path` を先にチェックし、`None` の場合は `pop_front()` / `clear()` を実行しない。これによりスタックが保持される
- `is_empty()` チェック直後の `.expect()` → `if let Some(...)` に書き換え（防御的プログラミング）
- テスト: `selected_path = None` のときスタックが保持されること、`selected_path = Some(...)` のとき正常に undo されること、空スタックで安全であることを検証

### Step 4: unreachable!() の安全化 (Logic Critical)

```
src/runtime/merge_scan/poll.rs
  - unreachable!() を tracing::warn! + return に変更

src/handler/dialog_keys.rs
  - 二重パターンマッチの解消、unreachable!() 除去

src/config.rs
  - (None, None) の unreachable!() を型レベル保証に変更

src/diff/engine.rs
  - _ => {} を _ => unreachable!() に変更（サイレント不正動作防止）
```

**Key Points:**
- 方針: 到達可能な `unreachable!()` → graceful handling
- 方針: 到達不能な `_ => {}` → `unreachable!()` で明示化
- テスト: 各箇所のエッジケースを追加

### Step 5: FileTree::find_node の BTreeMap 化 (Performance Critical)

```
src/tree.rs
  - FileNode の children を Option<Vec<FileNode>> → Option<BTreeMap<String, FileNode>> に変更
  - find_node_recursive を O(log N) ルックアップに置換
  - 影響範囲: tree_parser.rs, scan.rs, status.rs, diff.rs, output.rs 等
```

**Key Points:**
- **`BTreeMap<String, FileNode>` を採用**（HashMap ではなく BTreeMap）
  - 理由: TUI のファイルツリー表示でソート順が必要。BTreeMap はイテレーション時にキー順序を保証する
  - `children.get(name)` — O(log N) ルックアップ（現状 O(N) から大幅改善）
- **影響範囲が最大のため最後に実施**
- テスト: 既存テスト全通過 + find_node の O(log N) 動作確認テスト（criterion 等のベンチマークフレームワークは追加しない。単純な `#[test]` で動作確認のみ）

### Step 6: SSH stdout ゼロコピー化 (Performance)

```
src/ssh/client.rs
  - from_utf8_lossy().to_string() → String::from_utf8().unwrap_or_else(...) に変更
```

**Key Points:**
- valid UTF-8 時はゼロコピー（大多数のケース）
- invalid UTF-8 時のみ lossy 変換でフォールバック
- テスト: valid/invalid UTF-8 両方のケースを検証

### Step 7: .context() 追加 (Quality)

```
src/handler/merge_exec.rs     - マージ実行時のエラーに context 追加
src/handler/merge_batch.rs    - バッチマージのエラーに context 追加（失敗ファイル名を含む）
src/handler/reconnect.rs      - 再接続操作のエラーに context 追加
src/handler/merge_content.rs  - コンテンツ取得のエラーに context 追加
src/handler/merge_tree_load.rs - ツリー読み込みのエラーに context 追加
src/runtime/remote_io.rs      - リモート I/O 操作のエラーに context 追加
src/runtime/side_io.rs        - サイド I/O 操作の主要関数のエラーに context 追加
```

**Key Points:**
- handler 層で `context()` が 0 件 → ファイル操作・マージ操作など主要操作に追加
- フォーマット: `.context("merge file: {path}")` のように操作名 + 対象パスを含む
- **動作は変更しない** — エラーメッセージの情報量が増えるだけ
- スナップショットテストがないため、エラーメッセージ変更による既存テスト破壊リスクは低い
- テスト: `cargo test` 全通過を確認

### Step 8: コード重複の統合 (Hygiene)

```
src/config.rs + src/ssh/client.rs
  - expand_tilde を config.rs 側の PathBuf 版に統一
  - ssh/client.rs の String 版を削除し、config.rs の pub(crate) fn expand_tilde を呼び出す
  - ssh/client.rs 側の呼び出し元は .to_string_lossy().into_owned() で String に変換

src/app/dialog_ops.rs
  - filter_merge_candidates を filter_badge_merge_candidates にリネーム
  - テスト内の参照も同時にリネーム
```

**Key Points:**
- `expand_tilde`: `config.rs` に `pub(crate)` として残す（パス変換はインフラ層のロジックであり `format.rs` ではなく `config.rs` が適切）
- 戻り値型の差異: `config.rs` は `PathBuf` を返す。`ssh/client.rs` の呼び出し元は `PathBuf` → `String` 変換を追加
- `filter_merge_candidates` のリネーム: `dialog_ops.rs` 内のプライベート関数 + テスト内の参照を同時修正
- テスト: 既存テストのリネーム + ビルド成功確認

---

## ✅ Tests

### Step 1: ServerConfig Debug
- [ ] `Debug` 出力に `password` の値が含まれないこと
- [ ] 他フィールドは正常に表示されること

### Step 2: resolve_password Zeroizing
- [ ] 戻り値の型が `Zeroizing<String>` であること（コンパイル時検証）
- [ ] 環境変数パスで `Zeroizing<String>` が返ること
- [ ] config パスで `Zeroizing<String>` が返ること

### Step 3: undo_all バグ修正
- [ ] `selected_path = None` のとき undo スタックが保持されること
- [ ] `selected_path = Some(...)` のとき正常に undo されること
- [ ] 空スタックで `undo_all()` 呼び出しが安全であること

### Step 4: unreachable!() 安全化
- [ ] `merge_scan/poll.rs` — Progress メッセージ受信時にパニックしないこと
- [ ] `dialog_keys.rs` — 二重マッチ解消後も全ダイアログが正常動作すること
- [ ] `diff/engine.rs` — 予期しないタグ到達時のエラーハンドリング

### Step 5: FileTree BTreeMap 化
- [ ] find_node が O(log N) で動作すること
- [ ] ツリー構築・走査の既存テスト全通過
- [ ] ソート順表示が維持されること（BTreeMap のキー順序保証）

### Step 6: SSH stdout ゼロコピー
- [ ] valid UTF-8 入力で String が正しく生成されること
- [ ] invalid UTF-8 入力で lossy 変換が正しく動作すること

### Step 7: .context() 追加
- [ ] 既存テスト全通過（動作変更なし）

### Step 8: コード重複統合
- [ ] `expand_tilde` の統合後、既存テストが全通過すること
- [ ] `ssh/client.rs` の呼び出し元で PathBuf → String 変換が正しく動作すること
- [ ] `filter_badge_merge_candidates` リネーム後にビルド成功・テスト全通過

## 🔒 Security

- [ ] `ServerConfig::Debug` でパスワードがマスクされること
- [ ] `resolve_password` が `Zeroizing<String>` を返すこと
- [ ] パスワード文字列が意図せずログに出力されないこと

## 📊 Progress

| Step | 内容 | Status |
|------|------|--------|
| 1 | ServerConfig Debug パスワード漏洩修正 | 🟢 |
| 2 | resolve_password Zeroizing 化 | 🟢 |
| 3 | undo_all() データ消失バグ修正 | 🟢 |
| 4 | unreachable!() の安全化 | 🟢 |
| 5 | FileTree::find_node HashMap 化 | ⚪ |
| 6 | SSH stdout ゼロコピー化 | ⚪ |
| 7 | .context() 追加 | ⚪ |
| 8 | コード重複の統合 | ⚪ |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**Next:** Write tests → Implement → Commit with `claude-skills:commit` 🚀
