# E2E テスト充実化

**Cycle ID:** `20260310135716`
**Started:** 2026-03-10 13:57:16
**Status:** 🟢 Done

---

## 📝 What & Why

E2Eテストが圧倒的に不足している。既存は `tui_e2e.rs` に7テスト、`cli_exit_code.rs` に4テストのみで、主要操作のカバレッジが低い。テストシナリオごとにファイルを分けて整理し、各操作の「正しい状態」を定義した上でテストケースを網羅する。

## 🎯 Goals

- テストシナリオごとにファイルを分離して見通しをよくする
- CLI サブコマンド（status / diff / merge）の E2E テストを充実させる
- TUI 操作（ナビゲーション / 検索 / マージ / undo / 3way）の E2E テストを充実させる
- 各テストで「正しい状態」を明確に定義し、検証する
- エッジケース・境界条件・リグレッション防止テストを網羅する

## 📐 Design

### テストの分類方針

**2種類のE2Eテスト:**

1. **CLI E2E** — バイナリをプロセスとして実行し、stdout/stderr/exit code を検証。SSH不要のテストとSSH必要のテストを分離。
2. **TUI E2E** — PTY（expectrl）経由でバイナリを起動し、画面出力を検証。localhost SSH が必要。

### テスト実行時間の管理

- SSH不要テストは `cargo test` で常時実行（目標: 30秒以内）
- SSH必要テストは `#[ignore]` + `cargo test -- --ignored` で分離（目標: 120秒以内）
- CI では SSH不要テストのみ実行。SSH必要テストはローカル/専用環境で実行

### ファイル構成

```
tests/
  # ── 共通ヘルパー ──
  common/
    mod.rs              - 共通ヘルパー（E2eEnv, CliEnv, place_files, strip_ansi, remote_merge_cmd）

  # ── CLI E2E テスト（SSH不要）──
  cli_error_handling.rs - エラーハンドリング: exit code, 不正config, バリデーション
  cli_init.rs           - init サブコマンド: テンプレート生成
  cli_logs_events.rs    - logs/events サブコマンド: ログ読み込み, イベント読み込み

  # ── CLI E2E テスト（SSH必要）──
  cli_exit_codes.rs     - 終了コード区分: exit 0(成功/Equal) / 1(diff found) / 2(error)
  cli_status.rs         - status サブコマンド: text/json出力, --all, --summary, --ref, フィルタ
  cli_diff.rs           - diff サブコマンド: text/json出力, バイナリ, symlink, sensitive, --ref, 複数パス
  cli_merge.rs          - merge サブコマンド: --dry-run, 実マージ, --ref, バイナリ, symlink, --with-permissions

  # ── TUI E2E テスト（SSH必要）──
  tui_startup.rs        - 起動: 正常起動, ファイルツリー表示, オフライン表示
  tui_navigation.rs     - ナビゲーション: カーソル移動, ディレクトリ展開/折り畳み, 境界条件
  tui_diff_view.rs      - Diff表示: ファイル選択→diff表示, Enter連打, unified/side-by-side切替
  tui_search.rs         - 検索: ファイル検索(/), 結果ジャンプ(n/N), Esc解除, エッジケース
  tui_merge.rs          - マージ: ファイルマージ(m), undo(u), hunkマージ(l/h→Enter), sensitive警告
  tui_3way.rs           - 3way比較: バッジ表示, swap(X), conflict検知, サマリーパネル(W)
  tui_server_switch.rs  - サーバ切替: sキー, 状態維持, 再接続(F)

  # ── 既存（移行後削除） ──
  tui_e2e.rs            → tui_startup / tui_diff_view / tui_3way に分割移行
  cli_exit_code.rs      → cli_error_handling に移行
  tui_integration.rs    - 維持（ユニットテスト相当の結合テスト）
  ssh_integration.rs    - 維持（SSH結合テスト）
```

### 共通ヘルパー（tests/common/mod.rs）

既存の `E2eEnv`, `place_files`, `strip_ansi`, `regex_lite_strip`, `remote_merge_cmd` を集約。

**追加ヘルパー:**
- `CliEnv` — CLI E2E 用の環境（SSH接続あり。config生成 + tmpdir管理）
- `expect_screen_contains(session, text, timeout)` — 画面ダンプ→テキスト検索
- `send_keys(session, keys, delay)` — キー送信 + ウェイト
- `assert_exit_success(output)` / `assert_exit_error(output, code)` — exit code アサート
- `place_binary_file(dir, path, content)` — バイナリファイル配置（NULバイト含む）
- `place_symlink(dir, link_path, target_path)` — シンボリックリンク配置

### Key Points

- **SSH不要のCLIテストとSSH必要のテストを分離** — CI環境でSSH不要テストだけ実行可能に
- **SSH必要テストは `#[ignore]` で分離** — 通常の `cargo test` を高速に保つ
- **各テストで「初期状態 → 操作 → 期待状態」を明記** — コメントで正しい状態を定義
- **既存テストは移行完了後に旧ファイルを削除** — 段階的移行

## ✅ Tests

### Step 1: 共通ヘルパー抽出

- [ ] `tests/common/mod.rs` に `E2eEnv`, `CliEnv`, `place_files`, `strip_ansi` を集約
- [ ] `remote_merge_cmd()` ヘルパーを共通化
- [ ] `expect_screen_contains()`, `send_keys()` ヘルパー追加
- [ ] `place_binary_file()`, `place_symlink()` ヘルパー追加
- [ ] `assert_exit_success()` / `assert_exit_error()` ヘルパー追加

### Step 2: CLI E2E — エラーハンドリング（SSH不要）

`tests/cli_error_handling.rs` — 既存 `cli_exit_code.rs` の移行 + 拡充

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_missing_config_exits_with_code_2` | config なし | `status --config /nonexistent` | exit=2, stderr に "Error:" |
| 2 | `test_invalid_toml_exits_with_code_2` | 不正TOML | `status --config invalid.toml` | exit=2, stderr に parse error |
| 3 | `test_empty_config_exits_with_code_2` | 空TOML | `status --config empty.toml` | exit=2 |
| 4 | `test_invalid_server_name_rejected` | 正常config | `--right nonexistent_server` | exit≠0, "not found in config" |
| 5 | `test_self_compare_rejected` | 正常config | `status --left develop --right develop` | exit≠0, エラーメッセージ |
| 6 | `test_merge_requires_both_left_and_right` | 正常config | `merge file.txt --left develop` | exit≠0, "--right" 要求 |
| 7 | `test_ref_equals_left_warns` | 正常config | `status --left develop --ref develop` | stderr に "same as left side" 警告、ref無効化 |
| 8 | `test_ref_equals_right_warns` | 正常config | `status --right develop --ref develop` | stderr に "same as right side" 警告、ref無効化 |
| 9 | `test_merge_no_paths_given` | 正常config | `merge --left local --right develop` | exit≠0, パス指定必須 |
| 10 | `test_help_shows_usage` | なし | `--help` | exit=0, "Usage" 含む |
| 11 | `test_status_help_shows_options` | なし | `status --help` | exit=0, "--left" "--right" 含む |
| 12 | `test_diff_help_shows_options` | なし | `diff --help` | exit=0, "--format" 含む |
| 13 | `test_merge_help_shows_options` | なし | `merge --help` | exit=0, "--dry-run" 含む |
| 14 | `test_logs_help_shows_options` | なし | `logs --help` | exit=0, "--level" 含む |
| 15 | `test_events_help_shows_options` | なし | `events --help` | exit=0, "--type" 含む |

### Step 3: CLI E2E — init（SSH不要）

`tests/cli_init.rs`

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_init_creates_config_file` | 空ディレクトリ | `init` (cwd指定) | `.remote-merge.toml` 生成, 有効なTOML |
| 2 | `test_init_does_not_overwrite_existing` | 既存config | `init` | エラーまたはスキップ |

### Step 4: CLI E2E — 終了コード区分（SSH必要, `#[ignore]`）

`tests/cli_exit_codes.rs` — exit code 0/1/2 の区分を検証

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_status_exit_0_when_no_diff` | 同一ファイルのみ | `status` | exit=0（差分なし） |
| 2 | `test_status_exit_1_when_diff_found` | 差分ありファイル | `status` | exit=1（差分あり） |
| 3 | `test_diff_exit_0_when_equal` | 同一ファイル | `diff file.txt` | exit=0 |
| 4 | `test_diff_exit_1_when_diff_found` | 差分ファイル | `diff file.txt` | exit=1 |
| 5 | `test_merge_exit_0_on_success` | 差分ファイル | `merge file.txt --left local --right develop` | exit=0 |

### Step 5: CLI E2E — status（SSH必要, `#[ignore]`）

`tests/cli_status.rs`

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_status_text_shows_modified_files` | local≠remote のファイル | `status` | Modified 行が表示 |
| 2 | `test_status_text_shows_left_only` | local のみのファイル | `status` | LeftOnly 行が表示 |
| 3 | `test_status_text_shows_right_only` | remote のみのファイル | `status` | RightOnly 行が表示 |
| 4 | `test_status_excludes_equal_by_default` | 同一ファイル | `status` | Equal 行なし |
| 5 | `test_status_all_includes_equal` | 同一ファイル | `status --all` | Equal 行あり |
| 6 | `test_status_summary_shows_counts` | 混合 | `status --summary` | 件数表示 |
| 7 | `test_status_json_format` | 混合 | `status --format json` | 有効なJSON, files配列 |
| 8 | `test_status_with_ref_shows_badges` | 3サーバー | `status --ref local` | ref バッジ表示 |
| 9 | `test_status_with_directory_filter` | ディレクトリ構造 | `status src/` | src/ 以下のみ |
| 10 | `test_status_exclude_filter_works` | .git含むツリー | `status` | .git 除外 |
| 11 | `test_status_empty_tree_both_sides` | 両side空 | `status` | 空の出力 or "No files" |
| 12 | `test_status_sensitive_files_included` | .env差分あり | `status` | .env がstatus結果に含まれる |
| 13 | `test_status_json_special_chars_in_path` | `"qu ote".txt` | `status --format json` | JSONエスケープ正常 |

### Step 6: CLI E2E — diff（SSH必要, `#[ignore]`）

`tests/cli_diff.rs`

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_diff_text_shows_unified_diff` | 差分ファイル | `diff file.txt` | +/- 行表示 |
| 2 | `test_diff_json_format` | 差分ファイル | `diff file.txt --format json` | 有効JSON, hunks含む |
| 3 | `test_diff_equal_file` | 同一ファイル | `diff file.txt` | "Equal" or 差分なし |
| 4 | `test_diff_left_only_file` | local のみ | `diff file.txt` | LeftOnly 表示 |
| 5 | `test_diff_right_only_file` | remote のみ | `diff file.txt` | RightOnly 表示 |
| 6 | `test_diff_binary_file` | バイナリファイル | `diff image.png` | SHA-256 ハッシュ表示 |
| 7 | `test_diff_symlink` | シンボリックリンク | `diff link.txt` | リンク先表示 |
| 8 | `test_diff_sensitive_file_warning` | .env ファイル | `diff .env` | Warning 表示 |
| 9 | `test_diff_sensitive_file_force` | .env ファイル | `diff .env --force` | 内容表示 |
| 10 | `test_diff_multiple_files` | 複数ファイル | `diff a.txt b.txt` | 両ファイルの diff |
| 11 | `test_diff_directory` | ディレクトリ | `diff src/` | ディレクトリ内全ファイル |
| 12 | `test_diff_with_ref` | 3サーバー | `diff file.txt --ref local` | ref diff 含む |
| 13 | `test_diff_max_lines` | 大きいファイル | `diff big.txt --max-lines 10` | 10行制限 |
| 14 | `test_diff_nonexistent_file` | なし | `diff nonexistent.txt` | エラーまたは "not found" |
| 15 | `test_diff_empty_file` | 0バイトファイル | `diff empty.txt` | Equal or 空diff |
| 16 | `test_diff_trailing_slash_normalized` | ディレクトリ | `diff src/` vs `diff src` | 同一結果 |
| 17 | `test_diff_dot_path_resolves` | configあり | `diff .` | カレントdir全体のdiff（リグレッション d62cd21） |
| 18 | `test_diff_null_bytes_detected_as_binary` | NULバイト含む | `diff mixed.bin` | SHA-256バイナリ表示 |

### Step 7: CLI E2E — merge（SSH必要, `#[ignore]`）

`tests/cli_merge.rs`

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_merge_dry_run_shows_plan` | 差分ファイル | `merge file.txt --left local --right develop --dry-run` | "would" 表示, ファイル未変更 |
| 2 | `test_merge_writes_file` | 差分ファイル | `merge file.txt --left local --right develop` | remote ファイルが local と同一に |
| 3 | `test_merge_json_format` | 差分ファイル | `merge file.txt --left local --right develop --format json` | 有効JSON |
| 4 | `test_merge_multiple_files` | 複数差分 | `merge a.txt b.txt --left local --right develop` | 全ファイルマージ |
| 5 | `test_merge_directory` | ディレクトリ | `merge src/ --left local --right develop` | ディレクトリ内全マージ |
| 6 | `test_merge_creates_backup` | 差分ファイル | `merge file.txt --left local --right develop` | バックアップ作成 |
| 7 | `test_merge_sensitive_file_requires_force` | .env | `merge .env --left local --right develop` | 拒否, --force 要求 |
| 8 | `test_merge_sensitive_file_with_force` | .env | `merge .env --left local --right develop --force` | マージ成功 |
| 9 | `test_merge_binary_file` | バイナリ | `merge image.png --left local --right develop` | バイナリコピー |
| 10 | `test_merge_remote_to_remote_requires_force` | r2r | `merge file.txt --left develop --right staging` | サーバー名確認 |
| 11 | `test_merge_equal_file_skipped` | 同一ファイル | `merge same.txt --left local --right develop` | "Equal" スキップ |
| 12 | `test_merge_dry_run_does_not_modify` | 差分あり | `merge --dry-run ...` | ファイル内容がマージ前と完全一致 |
| 13 | `test_merge_duplicate_paths_deduplicated` | — | `merge a.txt a.txt --left local --right develop` | 1回だけマージ |
| 14 | `test_merge_r2r_with_dry_run_skips_guard` | r2r | `merge ... --left develop --right staging --dry-run` | サーバー名確認なし |

### Step 8: CLI E2E — logs/events（SSH不要）

`tests/cli_logs_events.rs` — logs/events サブコマンドの基本動作

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_logs_runs_without_log_file` | ログファイルなし | `logs` | exit=0, 空出力 or "No log" |
| 2 | `test_events_runs_without_event_file` | イベントファイルなし | `events` | exit=0, 空出力 or "No events" |
| 3 | `test_logs_with_level_filter` | ログファイルあり | `logs --level error` | error レベルのみ |
| 4 | `test_events_with_tail` | イベントファイルあり | `events --tail 5` | 最新5件 |

### Step 9: TUI E2E — 起動（SSH必要, `#[ignore]`）

`tests/tui_startup.rs` — 既存テストの移行 + 拡充

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_tui_starts_and_shows_file_tree` | 1ファイル | 起動 | ファイル名がツリーに表示 |
| 2 | `test_tui_shows_badge_for_modified_file` | 差分あり | 起動待ち | [M] バッジ表示 |
| 3 | `test_tui_shows_badge_for_left_only` | local のみ | 起動待ち | [+] バッジ |
| 4 | `test_tui_shows_header_with_server_names` | 正常config | 起動 | "local <-> develop" ヘッダー |
| 5 | `test_tui_quit_with_q` | 起動済み | q | プロセス終了 |
| 6 | `test_tui_help_dialog_with_question_mark` | 起動済み | ? | ヘルプ内容表示 |

### Step 10: TUI E2E — ナビゲーション（SSH必要, `#[ignore]`）

`tests/tui_navigation.rs`

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_cursor_down_with_j` | 複数ファイル | j | 2番目のファイルにカーソル移動 |
| 2 | `test_cursor_up_with_k` | 2番目にカーソル | k | 1番目のファイルにカーソル移動 |
| 3 | `test_directory_expand_with_enter` | ディレクトリ | Enter | 子ファイル表示 |
| 4 | `test_directory_collapse_with_h` | 展開済みdir | h | 子ファイル非表示 |
| 5 | `test_tab_switches_focus` | FileTree focus | Tab | DiffView focus |
| 6 | `test_cursor_does_not_go_above_first` | カーソル先頭 | k | カーソル位置変化なし（先頭維持） |
| 7 | `test_cursor_does_not_go_below_last` | カーソル末尾 | j | カーソル位置変化なし（末尾維持） |
| 8 | `test_deep_directory_expand_collapse` | 3階層ネスト | Enter×3 → h×3 | 正常に展開/折畳 |

### Step 11: TUI E2E — Diff 表示（SSH必要, `#[ignore]`）

`tests/tui_diff_view.rs` — 既存 Enter 連打テスト移行 + 拡充

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_file_select_shows_diff` | 差分ファイル | Enter | diff 内容表示 |
| 2 | `test_enter_spam_does_not_lose_diff` | 差分ファイル | Enter×5 | diff 維持 |
| 3 | `test_enter_spam_with_directory` | dir+ファイル | 展開→選択→Tab→Enter×3 | diff 維持 |
| 4 | `test_toggle_unified_sidebyside_with_d` | diff 表示中 | d | 表示モード切替 |
| 5 | `test_equal_file_shows_content` | 同一ファイル | 選択 | ファイル内容表示（diff なし） |

### Step 12: TUI E2E — 検索（SSH必要, `#[ignore]`）

`tests/tui_search.rs`

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_search_file_by_name` | 複数ファイル | / → "main" → Enter | main.rs にカーソル移動 |
| 2 | `test_search_next_with_n` | 検索結果あり | n | 次の結果に移動 |
| 3 | `test_search_cancel_with_esc` | 検索中 | Esc | 検索解除 |
| 4 | `test_search_no_match_shows_message` | ファイルあり | / → "zzzzz" → Enter | "Not found" 的メッセージ or 結果0件 |
| 5 | `test_search_empty_query_does_nothing` | — | / → Enter | 検索実行されない or 無視 |

### Step 13: TUI E2E — マージ（SSH必要, `#[ignore]`）

`tests/tui_merge.rs`

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_file_merge_with_m_and_confirm` | 差分ファイル選択済み | m → y | マージ完了、バッジ変化 |
| 2 | `test_merge_undo_with_u` | マージ済み | u | 元のバッジに復帰 |
| 3 | `test_merge_cancel_with_n` | 差分ファイル選択済み | m → n | 状態変化なし |
| 4 | `test_merge_undo_multiple_times` | 2回マージ済み | u × 2 | 2回分アンドゥ |
| 5 | `test_merge_on_equal_file_ignored` | Equal選択中 | m | ダイアログ出ない or 無視 |
| 6 | `test_sensitive_file_merge_shows_warning` | .env選択中 | m | 警告ダイアログ表示 |
| 7 | `test_hunk_merge_left_to_right_with_l` | diff表示中(複数hunk) | Tab → l | ハンク右方向適用、diff更新 |
| 8 | `test_hunk_merge_right_to_left_with_h` | diff表示中(複数hunk) | Tab → h | ハンク左方向適用、diff更新 |

### Step 14: TUI E2E — 3way（SSH必要, `#[ignore]`）

`tests/tui_3way.rs` — 既存テスト移行 + 拡充

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_3way_right_side_content_loads` | 3サーバー | ファイル選択 | left/right 両方表示 |
| 2 | `test_3way_conflict_badge_survives_reenter` | 3way conflict | Enter→Tab→Enter | [C!] 維持 |
| 3 | `test_3way_swap_with_x` | 3way | X | left↔right 入れ替え |
| 4 | `test_3way_summary_panel_toggle_with_w` | 3way表示中 | W → W | パネル表示→非表示 |
| 5 | `test_3way_ref_only_file_shows_badge` | refのみ存在 | 起動 | 適切なバッジ表示 |

### Step 15: TUI E2E — サーバ切替（SSH必要, `#[ignore]`）

`tests/tui_server_switch.rs`

| # | テスト名 | 初期状態 | 操作 | 期待状態 |
|---|---------|---------|------|---------|
| 1 | `test_invalid_server_name_rejected_at_startup` | 不正サーバー名 | 起動 | エラー終了 |

### Step 16: 既存テスト移行・削除

- [ ] `tui_e2e.rs` の全テストを新ファイルに移行完了後、削除
- [ ] `cli_exit_code.rs` の全テストを `cli_error_handling.rs` に移行後、削除

## 🔒 Security

- [ ] テスト用の一時ディレクトリは `TempDir` で自動クリーンアップ
- [ ] テストで `.env` や `*.pem` を作成する場合、一時ディレクトリ内のみ

## 📊 Progress

| Step | Description | Tests | Status |
|------|------------|-------|--------|
| 1 | 共通ヘルパー抽出 | — | 🟢 |
| 2 | CLI エラーハンドリング | 15 | 🟢 |
| 3 | CLI init | 2 | 🟢 |
| 4 | CLI 終了コード区分 | 5 | 🟢 |
| 5 | CLI status | 13 | 🟢 |
| 6 | CLI diff | 18 | 🟢 |
| 7 | CLI merge | 14 | 🟢 |
| 8 | CLI logs/events | 6 | 🟢 |
| 9 | TUI 起動 | 6 | 🟢 |
| 10 | TUI ナビゲーション | 8 | 🟢 |
| 11 | TUI Diff表示 | 5 | 🟢 |
| 12 | TUI 検索 | 5 | 🟢 |
| 13 | TUI マージ | 8 | 🟢 |
| 14 | TUI 3way | 5 | 🟢 |
| 15 | TUI サーバ切替 | 1 | 🟢 |
| 16 | 既存テスト移行・削除 | — | 🟢 |
| | **合計** | **111** | |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**Completed:** 全111テスト実装済み。SSH不要23テスト + SSH必要88テスト。clippy警告ゼロ。
