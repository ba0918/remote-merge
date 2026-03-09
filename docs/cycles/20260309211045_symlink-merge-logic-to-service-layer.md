# symlink merge ロジックのサービス層集約 + TUI 側バグ修正

**Cycle ID:** `20260309211045`
**Started:** 2026-03-09 21:10:45
**Status:** 🟡 Planning

---

## 📝 What & Why

symlink merge のビジネスロジック（`determine_merge_action`, `find_symlink_target`, `MergeAction`）が
CLI 層 (`src/cli/merge.rs`) に漏洩しており、TUI 側で同じ保護が効かない。
TUI では symlink merge 時にターゲット側の既存ファイル削除やバックアップが行われず、データ破壊のリスクがある。

ビジネスロジックをサービス層に移動し、TUI/CLI 両方から同一ロジックを使うようにする。

## 🎯 Goals

- `determine_merge_action` / `find_symlink_target` / `MergeAction` をサービス層に移動
- TUI の symlink merge を修正（remove_file + バックアップ対応）
- TUI のバッチマージで symlink を正しく処理
- TUI と CLI で symlink merge の挙動を完全一致させる

## 📐 Design

### 原則: 判定と実行の分離

- **判定ロジック（純粋関数）** → サービス層 (`service/merge.rs`)
  - `determine_merge_action`, `find_symlink_target`, `MergeAction`
  - 入力: ツリー参照 + パス → 出力: アクション enum（副作用なし）
- **実行ロジック（I/O）** → ハンドラ層
  - `MergeAction` の結果に基づく I/O 操作（backup, remove_file, create_symlink, write_file_bytes）
  - CLI は `CoreRuntime` を、TUI は `TuiRuntime` を使用（ランタイム依存のためサービス層に置けない）
  - **ただしハンドラ層は独自のビジネスロジック判定を一切行わない** — サービス層の判定結果を `match` するだけ

### Files to Change

```
src/
  service/merge.rs          - determine_merge_action, find_symlink_target, MergeAction を移動
  cli/merge.rs              - service::merge から import に切り替え（ロジック削除）
  handler/merge_exec.rs     - symlink 分岐で determine_merge_action を使用（冒頭で全マージパスをガード）
  handler/symlink_merge.rs  - MergeAction に基づく分岐実行に書き換え（TuiRuntime デリゲート使用）
  handler/merge_batch.rs    - symlink スキップを削除、MergeAction で分岐
  handler/merge_content.rs  - is_symlink_in_tree フィルタを削除
  handler/merge_file_io.rs  - is_symlink_in_tree を削除（呼び出し元は merge_content.rs の2箇所のみ）
```

### Step 1: ビジネスロジックをサービス層に移動

**src/service/merge.rs** に以下を移動（cli/merge.rs から）:

```rust
// cli/merge.rs:298-310 から移動
pub enum MergeAction {
    CreateSymlink { link_target: String, target_exists: bool },
    ReplaceSymlinkWithFile,
    Normal,
}

// cli/merge.rs:289-295 から移動
pub fn find_symlink_target(tree: &FileTree, path: &str) -> Option<String>

// cli/merge.rs:317-338 から移動
pub fn determine_merge_action(
    source_tree: &FileTree,
    target_tree: &FileTree,
    path: &str,
) -> MergeAction
```

依存する use 文: `crate::tree::{FileTree, NodeKind}` を service/merge.rs に追加。

テスト（13件: determine_merge_action 9件 + find_symlink_target 4件）も一緒に移動。

### Step 2: CLI 側を import に切り替え

**src/cli/merge.rs**:
- `MergeAction`, `determine_merge_action`, `find_symlink_target` のローカル定義を削除
- `use crate::service::merge::{MergeAction, determine_merge_action};` に切り替え
- `execute_single_merge` 内のロジックは変更なし（import 元が変わるだけ）

### Step 3: TUI 単一ファイルマージの修正

**src/handler/merge_exec.rs**:

`execute_merge()` の冒頭で `determine_merge_action` を呼び、
symlink 関連アクションを **ハンクマージ・通常マージより先に** ガードする。
これにより `execute_hunk_merge` / `execute_write_changes` に到達する前に symlink が検出される。

```rust
pub fn execute_merge(state: &mut AppState, runtime: &mut TuiRuntime, confirm: &ConfirmDialog) {
    let path = &confirm.file_path;
    let direction = confirm.direction;

    // ★ symlink 判定を最初に行う（ハンクマージ・通常マージより先）
    let (source_tree, target_tree) = match direction {
        MergeDirection::LeftToRight => (&state.left_tree, &state.right_tree),
        MergeDirection::RightToLeft => (&state.right_tree, &state.left_tree),
    };
    let action = crate::service::merge::determine_merge_action(source_tree, target_tree, path);

    match action {
        MergeAction::CreateSymlink { .. } | MergeAction::ReplaceSymlinkWithFile => {
            // source_side / target_side を先に取得（borrow 分離）
            let (source_side, target_side) = match direction {
                MergeDirection::LeftToRight => {
                    (state.left_source.clone(), state.right_source.clone())
                }
                MergeDirection::RightToLeft => {
                    (state.right_source.clone(), state.left_source.clone())
                }
            };
            super::symlink_merge::execute_symlink_merge(
                state, runtime, path, direction, action,
                &source_side, &target_side,
            );
            return;
        }
        MergeAction::Normal => {
            // 既存の通常マージフロー（DiffResult ベースの分岐）へ
        }
    }

    // 以降は既存コード（SymlinkDiff 分岐は不要になるため削除）
    // Binary チェック → ハンクマージ → 通常マージ
}
```

**borrow checker 対策**: `source_side` / `target_side` は `Side` の `Clone` で取得し、
`state` の `&mut` borrow と分離する。`Side` enum は `Clone` 実装済み。

**ハンクマージ / execute_write_changes への影響**:
`determine_merge_action` のチェックが `execute_merge` の最初に来るため、
symlink ファイルに対してハンクマージや write_changes に到達することはない。
既存の `DiffResult::SymlinkDiff` 分岐（L29-31）は削除する。

**src/handler/symlink_merge.rs**:

`execute_symlink_merge` を `MergeAction` ベースに書き換え。
`source_side` / `target_side` は引数で受け取る（borrow 問題回避）。
I/O は `TuiRuntime` のデリゲートメソッドを使用（`runtime.core.xxx` ではなく `runtime.xxx`）。

```rust
pub fn execute_symlink_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    path: &str,
    direction: MergeDirection,
    action: MergeAction,
    source_side: &Side,
    target_side: &Side,
) {
    let (src_label, dst_label) = match direction {
        MergeDirection::LeftToRight => (
            state.left_source.display_name(),
            state.right_source.display_name(),
        ),
        MergeDirection::RightToLeft => (
            state.right_source.display_name(),
            state.left_source.display_name(),
        ),
    };

    match action {
        MergeAction::CreateSymlink { link_target, target_exists } => {
            // バックアップ（target_exists の場合）
            if target_exists {
                if let Err(e) = runtime.create_backups(target_side, &[path.to_string()]) {
                    tracing::warn!("Backup failed (continuing): {}", e);
                }
                if let Err(e) = runtime.remove_file(target_side, path) {
                    state.set_status_error(format!("Failed to remove target: {}", e));
                    return;
                }
            }
            match runtime.create_symlink(target_side, path, &link_target) {
                Ok(()) => {
                    state.set_status_message(format!(
                        "Symlink merged: {} -> {} ({})", path, link_target, dst_label
                    ));
                }
                Err(e) => {
                    state.set_status_error(format!("Symlink merge failed: {}", e));
                }
            }
        }
        MergeAction::ReplaceSymlinkWithFile => {
            // バックアップ → symlink 削除 → ファイル書き込み
            if let Err(e) = runtime.create_backups(target_side, &[path.to_string()]) {
                tracing::warn!("Backup failed (continuing): {}", e);
            }
            if let Err(e) = runtime.remove_file(target_side, path) {
                state.set_status_error(format!("Failed to remove symlink: {}", e));
                return;
            }
            // ソース側のコンテンツをバイト列で読み込み（バイナリ安全）
            match runtime.read_file_bytes(source_side, path, false) {
                Ok(content) => {
                    if let Err(e) = runtime.write_file_bytes(target_side, path, &content) {
                        state.set_status_error(format!("Write failed: {}", e));
                        return;
                    }
                    state.set_status_message(format!(
                        "Symlink replaced with file: {} ({})", path, dst_label
                    ));
                }
                Err(e) => {
                    state.set_status_error(format!("Read source failed: {}", e));
                }
            }
        }
        MergeAction::Normal => {
            unreachable!("Normal action should not reach symlink_merge");
        }
    }

    // ★ ツリーキャッシュの更新
    // symlink → 通常ファイルに変わった場合、ツリーノードの kind を更新する必要がある。
    // ただし TUI では merge 後に `reload_trees()` が呼ばれるため（merge_exec.rs の後続処理）、
    // ここでの手動キャッシュ更新は不要。reload_trees() がサーバから最新ツリーを再取得する。
    // コンテンツキャッシュ（left_cache / right_cache）は reload_trees() 内でクリアされる。
}
```

### Step 4: TUI バッチマージの symlink 対応

**src/handler/merge_content.rs**:
- `is_symlink_in_tree` フィルタ（L26）を削除 — symlink もバッチマージ対象にする

**src/handler/merge_batch.rs**:
- バッチマージループ内で各ファイルに `determine_merge_action` を呼ぶ
- borrow checker 対策: ループ前に `source_side` / `target_side` を `Clone` で取得

```rust
// execute_batch_merge() 内
let (source_tree, target_tree) = match direction {
    MergeDirection::LeftToRight => (&state.left_tree, &state.right_tree),
    MergeDirection::RightToLeft => (&state.right_tree, &state.left_tree),
};
let source_side = match direction {
    MergeDirection::LeftToRight => state.left_source.clone(),
    MergeDirection::RightToLeft => state.right_source.clone(),
};
let target_side = match direction {
    MergeDirection::LeftToRight => state.right_source.clone(),
    MergeDirection::RightToLeft => state.left_source.clone(),
};

for path in &file_paths {
    let action = crate::service::merge::determine_merge_action(
        source_tree, target_tree, path,
    );
    match action {
        MergeAction::CreateSymlink { .. } | MergeAction::ReplaceSymlinkWithFile => {
            // symlink_merge に委譲（state の &mut は一時的に渡す）
            super::symlink_merge::execute_symlink_merge(
                state, runtime, path, direction, action,
                &source_side, &target_side,
            );
        }
        MergeAction::Normal => {
            // 既存の通常マージ処理
        }
    }
}
```

**注意**: `source_tree` / `target_tree` は `&state.left_tree` への immutable borrow。
`execute_symlink_merge` は `state: &mut AppState` を受け取るため、ループ内で
immutable borrow と mutable borrow が競合する。

**解決策**: ループ前に symlink アクションを事前計算する:

```rust
// ループ前に symlink 判定を事前計算
let symlink_actions: Vec<(String, MergeAction)> = file_paths.iter()
    .map(|p| {
        let action = crate::service::merge::determine_merge_action(
            source_tree, target_tree, p,
        );
        (p.clone(), action)
    })
    .collect();
// ここで source_tree / target_tree の borrow が終了

// ループでは事前計算結果を使う
for (path, action) in &symlink_actions {
    match action {
        MergeAction::CreateSymlink { .. } | MergeAction::ReplaceSymlinkWithFile => {
            super::symlink_merge::execute_symlink_merge(
                state, runtime, path, direction, action.clone(),
                &source_side, &target_side,
            );
        }
        MergeAction::Normal => {
            // 既存の通常マージ処理
        }
    }
}
```

### Step 5: 不要コードのクリーンアップ

- `src/handler/merge_file_io.rs` の `is_symlink_in_tree` を削除
  - 呼び出し元は `merge_content.rs` の2箇所のみ（調査済み）
  - Step 4 でフィルタ削除済みのため不要
- `src/handler/merge_exec.rs` の旧 `DiffResult::SymlinkDiff` 分岐（L29-31）を削除
  - Step 3 で `determine_merge_action` ベースに置き換え済み

## ✅ Tests

### サービス層テスト（既存テストの移動）
- [ ] `determine_merge_action` のテスト全9件が service/merge.rs に移動して通ること
- [ ] `find_symlink_target` のテスト全4件が service/merge.rs に移動して通ること

### TUI 側テスト（symlink_merge.rs に追加）
- [ ] `CreateSymlink` + `target_exists: true` で backup → remove_file → create_symlink が呼ばれること
- [ ] `CreateSymlink` + `target_exists: false` で create_symlink のみ呼ばれること
- [ ] `ReplaceSymlinkWithFile` で backup → remove_file → read_file_bytes → write_file_bytes が呼ばれること
- [ ] `ReplaceSymlinkWithFile` で remove_file 失敗時に早期リターンすること
- [ ] `Normal` で unreachable（panic テスト）

### 回帰テスト
- [ ] CLI の symlink merge が引き続き正常に動作すること（既存テスト全通過）
- [ ] TUI の通常マージ（テキスト・バイナリ）が影響を受けないこと
- [ ] ハンクマージが symlink ファイルに対して到達しないこと
- [ ] バッチマージが symlink を含むディレクトリで正しく動作すること

## 🔒 Security

- [ ] remove_file のパストラバーサル検証は既存の validate_path_within_root で保護済み
- [ ] create_symlink のパストラバーサル検証は既存実装で保護済み
- [ ] バッチマージでの symlink 追加時、シンボリックリンク先の安全性チェックは既存の warning 機構で対応

## 📊 Progress

| Step | Description | Status |
|------|-------------|--------|
| 1 | ビジネスロジックをサービス層に移動 + テスト移動 | ⚪ |
| 2 | CLI 側を import に切り替え | ⚪ |
| 3 | TUI 単一ファイルマージの修正（merge_exec + symlink_merge） | ⚪ |
| 4 | TUI バッチマージの symlink 対応（merge_batch + merge_content） | ⚪ |
| 5 | 不要コードのクリーンアップ | ⚪ |
| 6 | 全テスト実行 + clippy | ⚪ |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**Next:** Write tests → Implement → Commit with `smart-commit`
