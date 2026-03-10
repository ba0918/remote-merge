# SSH 大容量ファイル書き込みバグ修正

**Cycle ID:** `20260310233345`
**Started:** 2026-03-10 23:33:45
**Status:** 🟢 Done

---

## 📝 What & Why

`SshClient::write_file_bytes()` が巨大バイナリ（11MB の Agent バイナリ等）を
SSH チャネル経由で転送する際、base64 エンコード済みデータ（約15MB）を
**単一の `channel.data()` 呼び出し**で送信している。

SSH チャネルのウィンドウサイズ制限により、データが途中で切れて
128 バイトのゴミファイルが書き込まれ、Agent デプロイが exit code 126
（Exec format error）で失敗する。

**影響範囲:**
- Agent バイナリデプロイ → Agent が使えず常に SSH フォールバック
- 大容量バイナリファイルの merge（base64 方式）も同様に壊れる可能性

## 🎯 Goals

- `write_file_bytes()` でチャンク分割送信し、任意サイズのバイナリを正しく転送する
- `write_file()` も同様にチャンク分割する（テキストでも巨大ファイルで同じ問題が起きうる）
- Agent デプロイが localhost で正常に完了することを確認する

## 📐 Design

### Files to Change

```
src/ssh/client.rs
  - write_file(): channel.data() をチャンク分割ループに変更
  - write_file_bytes(): 同上
  - CHANNEL_DATA_CHUNK_SIZE 定数追加（32KB）
```

### Key Points

- **チャンクサイズ**: 32KB（SSH ウィンドウサイズのデフォルト 64KB より小さく、安全なサイズ）
- **変更箇所**: `channel.data()` の呼び出しを `for chunk in data.chunks(CHUNK_SIZE)` ループに置換
- **既存テスト**: E2E テスト（agent_ssh_deploy, ssh_integration）で大容量転送が通ることを確認
- **リスク**: 低。チャンク分割はデータの意味を変えない純粋な転送最適化

### 修正パターン

Before:
```rust
channel.data(encoded_with_newline.as_slice()).await?;
```

After:
```rust
const CHANNEL_DATA_CHUNK_SIZE: usize = 32 * 1024; // 32KB

for chunk in encoded_with_newline.chunks(CHANNEL_DATA_CHUNK_SIZE) {
    channel.data(chunk).await?;
}
```

## ✅ Tests

- [x] `write_file()` で 100KB テキストを書き込み、チャンク分割でサーバーに正しく到達することを確認
- [x] `write_file_bytes()` で 64KB バイナリを書き込み、base64 ラウンドトリップが一致することを確認
- [x] `CHANNEL_DATA_CHUNK_SIZE` が SSH ウィンドウサイズ未満であることの const assert
- [x] 既存 E2E テスト（Agent デプロイ含む）が全て通過

## 📊 Progress

| Step | Status |
|------|--------|
| write_file チャンク分割 | 🟢 |
| write_file_bytes チャンク分割 | 🟢 |
| send_and_finish_channel 共通ヘルパー抽出 | 🟢 |
| テスト追加・確認（E2E 3件 + ユニット 1件） | 🟢 |
| 全1476テスト通過 + clippy 警告ゼロ | 🟢 |
| Commit | 🟢 |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**Next:** 実装 → テスト → Commit with `smart-commit` 🚀
