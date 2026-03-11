# パフォーマンスチューニング

**Cycle ID:** `20260311225740`
**Started:** 2026-03-11 22:57:40
**Status:** 🟢 Done

---

## 📝 What & Why

Agent 利用時・SSH フォールバック時のパフォーマンスボトルネックを特定し、SSHコマンド回数の削減・バッチ処理の最適化で体感速度を大幅に向上させる。

## 🎯 Goals

- Agent デプロイの SSH exec 回数を **6回 → 3回** に削減（write_file_bytes は SSH チャネル送信のため分離必須）
- SSH フォールバック時のファイル読み込みを **逐次 → バッチ** に変更（バイナリ判定を維持）
- SSH フォールバック時のツリー展開を **階層ごとの find → 1回の再帰 find** に変更
- リモート rollback の **ファイルごとSSH exec → バッチ cp**（ARG_MAX 考慮でチャンク分割）
- バックアップセッション一覧取得の **N+1 SSH → 1回** に統合

## 📐 Design

### ボトルネック分析

調査で特定した主要なパフォーマンス低下箇所:

| # | 箇所 | 現状 | 問題 | 改善案 |
|---|------|------|------|--------|
| 1 | Agent デプロイ (`core.rs:200-306`) | mkdir → symlink check → write → chmod → checksum → verify → mv の **6 exec + 1 write** | チャネル開閉オーバーヘッド × 6 | **pre-write(1) + write + post-write(1) = 3** |
| 2 | SSH フォールバック時ツリー展開 (`task.rs:708-802`) | `expand_subtree_recursive` が **階層ごとに** SSH `list_dir` | 深さ D のツリー = D 回の SSH exec | **既存の `list_tree_recursive` で1回取得** |
| 3 | SSH フォールバック時コンテンツ読み込み (`task.rs:583-648`) | `read_all_contents` が **1ファイルずつ逐次** SSH exec | N ファイル = N 回の SSH exec | **既存の batch_read を活用** |
| 4 | ref コンテンツ読み込み (`task.rs:651-704`) | **1ファイルずつ逐次** SSH exec（Agent なし時） | N ファイル = N 回の SSH exec | **batch_read を適用** |
| 5 | リモート rollback (`remote_io.rs:511-621`) | **ファイルごとに** mkdir + cp + echo を SSH exec | N ファイル = N 回の SSH exec | **バッチ cp（1000ファイルごとにチャンク分割）** |
| 6 | バックアップセッション一覧 (`remote_io.rs:456-505`) | セッション数 × find SSH exec (**N+1 問題**) | 10 セッション = 11 回の SSH exec | **1回の find で全ファイルを取得** |

### Files to Change

```
src/
  agent/deploy.rs             - build_pre_write_command(), build_post_write_script() 純粋関数追加
  runtime/core.rs             - deploy_agent_binary を3 exec化
  runtime/merge_scan/task.rs  - ツリー展開1回化 + コンテンツbatch_read + 共通ヘルパー切り出し
  runtime/remote_io.rs        - セッション一覧1回取得 + rollback I/O実行のみ
  service/rollback.rs         - build_batch_restore_script() 純粋関数を追加（レイヤー分離）
  backup/mod.rs               - parse_all_backup_entries() 純粋関数を追加（レイヤー分離）
  ssh/client.rs               - (変更なし、既存 batch_read / list_tree_recursive を活用)
```

### Key Points

- **Agent デプロイ 3 exec 化**: `deploy.rs` に `build_pre_write_command()`（mkdir+symlink_check 結合）と `build_post_write_script()`（chmod+checksum+verify+mv 統合）を追加。write_file_bytes は SSH チャネルデータ送信なので分離が必須
- **ツリー展開の1回化**: SSH フォールバック時の `expand_subtree_recursive`（階層ごとに list_dir）を、既存の `list_tree_recursive`（1回の再帰 find）に置換。ツリー更新データは find 結果から構築
- **コンテンツの batch_read**: `read_all_contents` と `read_ref_contents` を共通ヘルパー `read_remote_contents_batch()` で DRY 化。バイナリ判定は batch_read の String 結果に `is_binary()` を適用して維持
- **rollback バッチ化**: 純粋関数 `build_batch_restore_script()` は `service/rollback.rs` に配置（レイヤー分離）。ARG_MAX 対策で1000ファイルごとにチャンク分割
- **セッション一覧 N+1 解消**: パース純粋関数 `parse_all_backup_entries()` は `backup/mod.rs` に配置（既存パース関数群と同居）
- **進捗表示**: batch_read 前後に Progress メッセージを送信し、UX を維持

## 🏗️ Implementation Steps

### Step 1: Agent デプロイスクリプト集約

**影響ファイル:** `agent/deploy.rs`, `runtime/core.rs`

`deploy.rs` に2つの純粋関数を追加:

```rust
/// write 前の前処理コマンド（mkdir + symlink_check を結合）
pub fn build_pre_write_command(remote_path: &Path) -> String
// 出力例: mkdir -p /var/tmp/remote-merge-user && \
//         test -L /var/tmp/remote-merge-user/remote-merge && echo SYMLINK || echo OK

/// write_file_bytes 後に実行する検証+アトミックリネームスクリプト
/// 検証失敗時は .tmp を削除して非ゼロ exit で終了
pub fn build_post_write_script(
    remote_path: &Path,
    tmp_path: &str,
    local_hash: &str,
) -> String
// 出力例: chmod 700 /tmp/.tmp && \
//         HASH=$(sha256sum /tmp/.tmp | awk '{print $1}') && \
//         [ "$HASH" = "abc123..." ] && \
//         /tmp/.tmp --version | grep -q 'remote-merge X.Y.Z' && \
//         mv /tmp/.tmp /var/tmp/remote-merge-user/remote-merge || \
//         { rm -f /tmp/.tmp; exit 1; }
```

`core.rs` の `deploy_agent_binary` を変更:
- 現状: mkdir(1) → symlink_check(1) → write → chmod(1) → checksum(1) → verify(1) → mv(1) = **6 exec**
- 改善: pre_write_command(1) → write → post_write_script(1) = **2 exec + 1 write = 3**

**削減効果:** 6 exec → 2 exec（write 除く）

### Step 2: SSH フォールバック時ツリー展開の1回化

**影響ファイル:** `runtime/merge_scan/task.rs`

`expand_subtree_recursive`（階層ごとに `client.list_dir()`）を、既存の `client.list_tree_recursive()` に置換:

```rust
// 現状: ディレクトリ深さ D 回の SSH exec
// 改善: 1回の find -P で全ツリーを取得
fn expand_subtree_ssh(
    rt: &Runtime, client: &mut SshClient,
    remote_root: &str, dir_path: &str, exclude: &[String],
) -> Result<Vec<(String, Vec<FileNode>)>, String> {
    let sub_dir = format!("{}/{}", remote_root.trim_end_matches('/'), dir_path);
    let all_nodes = rt.block_on(client.list_tree_recursive(&sub_dir, exclude))?;
    // all_nodes を親ディレクトリごとにグルーピングして tree_updates を構築
    group_nodes_by_parent(all_nodes, dir_path)
}
```

**削減効果:** D exec → 1 exec（ツリー深さに依存しなくなる）

### Step 3: SSH フォールバック時のバッチ読み込み + 共通ヘルパー

**影響ファイル:** `runtime/merge_scan/task.rs`

共通ヘルパー `read_remote_contents_batch()` を新設（DRY 化）:

```rust
/// SSH フォールバック時のリモートファイルバッチ読み込み共通ヘルパー
/// batch_read で取得した String に is_binary() を適用してテキスト/バイナリを分類
fn read_remote_contents_batch(
    rt: &Runtime,
    client: &mut SshClient,
    root: &str,
    file_paths: &[String],
    tx: Option<&mpsc::Sender<MergeScanMsg>>,
) -> (HashMap<String, String>, HashMap<String, BinaryInfo>, HashSet<String>) {
    let mut text_cache = HashMap::new();
    let mut binary_cache = HashMap::new();
    let mut error_paths = HashSet::new();

    let full_paths: Vec<String> = file_paths.iter()
        .map(|p| format!("{}/{}", root.trim_end_matches('/'), p))
        .collect();

    // バッチ読み込み（read_files_batch は String を返す）
    match rt.block_on(client.read_files_batch(&full_paths)) {
        Ok(batch) => {
            for (i, path) in file_paths.iter().enumerate() {
                if let Some(content) = batch.get(&full_paths[i]) {
                    // バイナリ判定を維持
                    if crate::diff::engine::is_binary(content.as_bytes()) {
                        binary_cache.insert(path.clone(), BinaryInfo::from_bytes(content.as_bytes()));
                    } else {
                        text_cache.insert(path.clone(), content.clone());
                    }
                } else {
                    error_paths.insert(path.clone());
                }
            }
        }
        Err(e) => {
            tracing::warn!("Batch read failed: {}", e);
            for path in file_paths { error_paths.insert(path.clone()); }
        }
    }

    // 進捗表示更新
    if let Some(sender) = tx {
        let _ = sender.send(MergeScanMsg::Progress {
            files_found: file_paths.len(),
            current_path: file_paths.last().cloned(),
        });
    }

    (text_cache, binary_cache, error_paths)
}
```

`read_all_contents` と `read_ref_contents` の Remote ブランチを共通ヘルパーで置換。

**削減効果:** N exec → ceil(N/batch_size) exec

### Step 4: リモート rollback のバッチ化

**影響ファイル:** `service/rollback.rs`（純粋関数）, `runtime/remote_io.rs`（I/O実行）

`service/rollback.rs` に純粋関数を追加:

```rust
/// バッチ復元スクリプトを生成する（ARG_MAX 対策でチャンク分割）
/// 1チャンクあたり最大 MAX_BATCH_FILES(1000) ファイル
pub fn build_batch_restore_scripts(
    root_dir: &str,
    backup_dir_name: &str,
    session_id: &str,
    files: &[String],
) -> Vec<String>  // 複数スクリプトのリスト

/// OK/FAIL マーカー出力をパースして結果を返す
pub fn parse_batch_restore_output(
    output: &str,
) -> (Vec<String>, Vec<(String, String)>)  // (ok_paths, fail_paths_with_reason)
```

`runtime/remote_io.rs` の `restore_remote_backup_ssh` を変更:
- パストラバーサル検証はバッチスクリプト生成前に実施（既存ロジック維持）
- 生成されたスクリプト群を順次 SSH exec
- 戻り値の型は変更なし（後方互換）

**削減効果:** N exec → ceil(N/1000) exec

### Step 5: バックアップセッション一覧の N+1 解消

**影響ファイル:** `backup/mod.rs`（純粋関数）, `runtime/remote_io.rs`（I/O実行）

`backup/mod.rs` にパース純粋関数を追加:

```rust
/// find -mindepth 2 の出力から全セッションのファイルを一括パースする
/// 行フォーマット: "session_id/rel_path\tsize"
/// パストラバーサル防御 + タイムスタンプ検証 + 降順ソートを維持
pub fn parse_all_backup_entries(find_output: &str) -> Vec<BackupSession>
```

`runtime/remote_io.rs` の `list_remote_backup_sessions_ssh` を変更:
```bash
# 現状: 1回(セッション一覧) + N回(各セッションのファイル一覧)
# 改善: 1回で全ファイルを取得
find /backup_dir -mindepth 2 -type f -printf '%P\t%s\n' 2>/dev/null | sort
# 出力例: 20240115-140000/src/a.ts\t1234
```

**削減効果:** N+1 exec → 1 exec

## ✅ Tests

### Step 1: Agent デプロイ
- [ ] `build_pre_write_command` が mkdir + symlink_check を1コマンドに結合する
- [ ] `build_post_write_script` が chmod + checksum + verify + mv を正しく生成する
- [ ] `build_post_write_script` のパスにシェルエスケープが適用される
- [ ] checksum 不一致時に .tmp が削除され非ゼロ exit になる
- [ ] sha256sum 不在時の graceful degradation（既存挙動維持）

### Step 2: ツリー展開1回化
- [ ] `group_nodes_by_parent` が find 結果を正しくディレクトリごとに分類する
- [ ] 空ディレクトリの場合に空リストを返す
- [ ] exclude パターンが find 結果に適用される

### Step 3: SSH フォールバック batch_read + 共通ヘルパー
- [ ] `read_remote_contents_batch` がテキスト/バイナリを正しく分類する
- [ ] バイナリファイルが `is_binary()` で判定され `BinaryInfo` に格納される
- [ ] バッチ読み込みで一部ファイルが存在しない場合の error_paths 処理
- [ ] 空のファイルリストでパニックしない
- [ ] batch_read 前後の Progress メッセージ送信

### Step 4: rollback バッチ化
- [ ] `build_batch_restore_scripts` が正しいスクリプトを生成する
- [ ] 1000ファイル超で複数チャンクに分割される
- [ ] パストラバーサルを含むファイルがスクリプトから除外される
- [ ] `parse_batch_restore_output` が OK/FAIL マーカーを正しくパースする
- [ ] 空ファイルリストで空リストを返す

### Step 5: セッション一覧 N+1 解消
- [ ] `parse_all_backup_entries` が複数セッションを正しくグルーピングする
- [ ] 空の find 出力で空リストを返す
- [ ] パストラバーサルを含む session_id が除外される
- [ ] タイムスタンプ形式でない session_id が除外される
- [ ] session 内にファイルが0件のケースが正しく処理される
- [ ] 降順ソートが維持される

## 🔒 Security

- [ ] `build_pre_write_command` のパスに shell_escape を適用
- [ ] `build_post_write_script` のパスに shell_escape を適用
- [ ] `build_batch_restore_scripts` のパスに shell_escape を適用
- [ ] パストラバーサル検証を維持（既存ロジックを引き継ぐ）
- [ ] session_id のバリデーション（`..`, `/`, `\` 拒否）を維持

## 📊 Progress

| Step | Description | Status |
|------|-------------|--------|
| 1 | Agent デプロイスクリプト集約（6→2 exec） | 🟢 |
| 2 | SSH フォールバック ツリー展開1回化 | 🟢 |
| 3 | SSH フォールバック batch_read + 共通ヘルパー | 🟢 |
| 4 | rollback バッチ化（N→ceil(N/1000) exec） | 🟢 |
| 5 | セッション一覧 N+1 解消 | 🟢 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

## 📈 期待される改善効果

| 操作 | Before | After | 改善率 |
|------|--------|-------|--------|
| Agent デプロイ | 6 SSH exec | 2 SSH exec (+1 write) | -67% |
| ツリー展開（SSH, 深さ10） | 10+ SSH exec | 1 SSH exec | -90% |
| コンテンツ読み込み（SSH, 100ファイル） | 100 SSH exec | ~5 SSH exec | -95% |
| rollback（10ファイル） | 10 SSH exec | 1 SSH exec | -90% |
| rollback（2000ファイル） | 2000 SSH exec | 2 SSH exec | -99.9% |
| バックアップ一覧（5セッション） | 6 SSH exec | 1 SSH exec | -83% |
| ref 読み込み（SSH, 100ファイル） | 100 SSH exec | ~5 SSH exec | -95% |

---

**Next:** Write tests → Implement → Commit with `smart-commit` 🚀
