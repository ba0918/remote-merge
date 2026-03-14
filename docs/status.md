# Project Status

**Last Updated:** 2026-03-14 17:56

---

## 🎯 Current Session

### 20260314175606 - ハードコード色のパレット集約（ライトテーマ視認性修正）
- **Started:** 2026-03-14 17:56:06
- **Completed:** 2026-03-14
- **Status:** 🟢 Complete
- **Plan:** [Link](./cycles/20260314175606_fix-binary-hash-color-light-theme.md)
- **Summary:** lightテーマでバイナリハッシュ値やサーバ選択ダイアログが背景と同化する問題。src/ui/ 全体のハードコード Color:: をセマンティックパレットフィールド（positive/negative/info/muted/warning）に集約。3ステップ実装。

---

### 20260314171406 - ディレクトリ展開時の非同期バッジスキャン
- **Started:** 2026-03-14 17:14:06
- **Completed:** 2026-03-14
- **Status:** 🟢 Complete
- **Plan:** [Link](./cycles/20260314171406_async-badge-scan-on-expand.md)
- **Result:** [Link](./cycles/20260314171406_async-badge-scan-on-expand_result.md)
- **Summary:** ディレクトリ展開時にその直下ファイルのみ非同期でdiffスキャンし、バッジを `[?]` → `[..]` → `[M]`/`[=]` 等に順次更新。UIブロックなし、折りたたみ時キャンセル、キャッシュ活用、再接続時全スキャンキャンセル。badge_scan モジュール新設（helpers/task/mod/poll）、+49テスト、4コミット。

---

### 20260314133920 - バッジ色のライトテーマ対応
- **Started:** 2026-03-14 13:39:20
- **Completed:** 2026-03-14
- **Status:** 🟢 Complete
- **Plan:** [Link](./cycles/20260314133920_fix-badge-color-light-theme.md)
- **Summary:** ライト系テーマで黄色バッジが背景に同化する問題を修正。バッジ色を TuiPalette に集約し、ライト時は紫系、ダーク時は従来の黄色系に切り替え。

---

### 20260314002623 - コードベース品質改善リファクタリング
- **Started:** 2026-03-14 00:26:23
- **Completed:** 2026-03-14
- **Status:** 🟢 Complete
- **Plan:** [Link](./cycles/20260314002623_codebase-refactoring.md)
- **Result:** [Link](./cycles/20260314002623_codebase-refactoring_result.md)
- **Summary:** コードベース全体の品質改善。side_io.rs の純粋関数抽出（6関数）、badge.rs のバッジ計算純粋関数化（3関数）、dialog_ops.rs のマージ候補フィルタリング純粋関数化、Handler 層薄化（tree_keys/merge_batch/merge_exec）、selection.rs の I/O と状態更新分離、ホットパスのクローン削減。7ステップ全完了、+56テスト（2254テスト通過）、7コミット、testenv QA全パス。

---

### 20260313220450 - Side-by-Side シンタックスハイライト
- **Started:** 2026-03-13 22:04:50
- **Status:** 🟢 Complete
- **Completed:** 2026-03-13
- **Plan:** [Link](./cycles/20260313220450_side-by-side-syntax-highlight.md)
- **Summary:** TUI の Side-by-Side diff 表示にシンタックスハイライトを適用。truncate_spans() 純粋関数で Span ベースの幅制限＋パディングを実現し、render_sbs_half() 抽出で text_spans() → truncate_spans() パイプラインを統合。検索ハイライトも Side-by-Side で動作。+20テスト（2198テスト通過）、clippy 警告ゼロ。

---

### 20260313204742 - include フィルタ（ホワイトリスト方式スキャン制限）
- **Started:** 2026-03-13 20:47:42
- **Status:** 🟢 Complete
- **Completed:** 2026-03-13
- **Plan:** [Link](./cycles/20260313204742_include-filter.md)
- **Summary:** `[filter] include` パターンを追加し、スキャン対象をホワイトリスト方式で限定。20万件超プロジェクトでのスキャン破綻を解決。SSH find コマンドの起点パス自体を絞り、リモート負荷も削減。FilterConfig + normalize_include_paths 純粋関数、ローカル/SSH/Agent 全スキャンパスに include 伝播、三重セキュリティ防御（config正規化 + canonicalize + root_prefix_check）。+48テスト（2225テスト通過）、17ファイル変更、+1122行、clippy警告ゼロ。

---

### 20260313194427 - plan-reviewer 汎用化 + review loop 強化
- **Started:** 2026-03-13 19:44:27
- **Status:** 🟡 Planning
- **Plan:** [Link](./cycles/20260313194427_plan-reviewer-generalization.md)
- **Summary:** plan-reviewer の review-dimensions.md を言語非依存に書き直し、`.claude/review-rules.md` によるプロジェクト固有ルール追加の仕組みを実装。review-timestamped-plan-loop をイテレーション制限・差分レビュー・責務明確化で強化。

---

### 20260313171720 - クロスプラットフォーム Agent デプロイ
- **Started:** 2026-03-13 17:17:20
- **Status:** 🟡 Planning
- **Plan:** [Link](./cycles/20260313171720_cross-platform-agent-deploy.md)
- **Summary:** Agent デプロイをクロスプラットフォーム対応に。リモートの OS/arch を検出し適切なバイナリを選択してデプロイ。インストールスクリプトで Linux musl Agent バイナリを同梱。5ステップ実装計画。

---

### 20260313002514 - スキャン信頼性・Agent 堅牢性の致命的バグ修正
- **Started:** 2026-03-13 00:25:14
- **Completed:** 2026-03-13
- **Status:** 🟢 Complete
- **Plan:** [Link](./cycles/20260313002514_fix-scan-agent-reliability.md)
- **Summary:** testenv 100,000ファイル環境で発見された致命的バグ4件を修正。(1) status truncation エラー化 + `--max-entries` 設定可能化、(2) Agent pipe 部分書き込み対応、(3) tree_scan ディレクトリ除外でカウント一致、(4) `agent_unavailable` HashMap で失敗キャッシュ。`should_invalidate_agent_error()` でエラーチェーン探索。+24テスト（2093テスト通過）、clippy警告ゼロ。

---

### 20260312212041 - CLI Targeted I/O（フルスキャン廃止 + batch_read チャンク分割）
- **Started:** 2026-03-12 21:20:41
- **Status:** 🟡 Planning
- **Plan:** [Link](./cycles/20260312212041_cli-targeted-io.md)
- **Summary:** CentOS 5 テスト環境（testenv/）で P1（1ファイル diff でフルスキャン）と P2（ARG_MAX 超過）を再現確認。指定パス時は fast path（ツリースキャンなし）、フルスキャン時は batch_read チャンク分割で安全化。6ステップ実装計画。

---

### 20260312203010 - CentOS 5 問題分析（問題記録）
- **Started:** 2026-03-12 20:30:10
- **Status:** 📋 Analysis Complete → 20260312212041 に引き継ぎ
- **Plan:** [Link](./cycles/20260312203010_cli-targeted-io-no-fullscan.md)
- **Summary:** CentOS 5 実機検証で発見した致命的問題4件の記録。P1-P4 の問題分析と初期方針。

---

### 20260312163332 - コード品質改善 — Cycle C
- **Started:** 2026-03-12 16:33:32
- **Completed:** 2026-03-12
- **Status:** 🟢 Complete
- **Plan:** [Link](./cycles/20260312163332_code-quality-cycle-c.md)
- **Summary:** format_size 共通化（format.rs 新設、binary.rs/metadata.rs/content_render.rs の重複排除）、side_io.rs with_agent() ヘルパー（6メソッドのボイラープレート集約）、bootstrap.rs validate_server_params テスト7件追加。+18テスト（1967テスト通過）、clippy警告ゼロ。

---

### 20260312152430 - パフォーマンス最適化 — Cycle B (Step 5-8)
- **Started:** 2026-03-12 15:24:30
- **Completed:** 2026-03-12
- **Status:** 🟢 Complete
- **Plan:** [Link](./cycles/20260312152430_perf-optimization-cycle-b.md)
- **Summary:** undo_stack から DiffResult 削除（compute_diff 再計算方式）、build_tree_from_flat() BTreeMap 中間構造体で往復変換排除、DiffHunk.lines を line_range: Range<usize> に変更（行データ一元化）、is_line_in_hunk() O(1) 化（line_range.contains）、merge_hunk_line_indices 削除。+15テスト（1959テスト通過）、clippy警告ゼロ。

---

### 20260312131426 - MVP品質改善（Major Issues 全9件）— Cycle A 完了
- **Started:** 2026-03-12 13:14:26
- **Status:** 🟢 Cycle A Complete
- **Plan:** [Link](./cycles/20260312131426_mvp-quality-improvements.md)
- **Summary:** Cycle A (Step 1-4) 完了。TOFU ホストキー確認（HostKeyVerifier trait + oneshot channel + --yes）、パスフレーズ付き SSH 鍵（PassphraseProvider trait + Zeroizing + rpassword）、optimistic_lock ファイル削除検出（ConflictReason enum）、handler 層テスト追加（45テスト + unwrap_or_default 修正）。+91テスト（1853→1944テスト通過）、clippy警告ゼロ。

---

### 20260312110725 - Agent シェル互換性 + バックアップパス修正
- **Started:** 2026-03-12 11:07:25
- **Completed:** 2026-03-12
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260312110725_agent-shell-compat-and-backup-path.md)
- **Summary:** CLI 実機検証で発見した2つのバグ修正。Agent デプロイコマンドの fish シェル互換性 + Agent backup の絶対パスエラー修正

---

### 20260312004348 - sudo Agent 起動 + ファイルパーミッション設定
- **Started:** 2026-03-12 00:43:48
- **Completed:** 2026-03-12
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260312004348_sudo-agent-and-permissions.md)
- **Summary:** sudo=true で Agent を sudo 起動、ファイル書き込み時の owner/group/permissions 自動復元、DefaultsConfig + parse_permissions + MetadataConfig、pre-flight チェック（NOPASSWD + uid/gid 取得）、invalidated_sudo_servers でフォールバック禁止。+62テスト（1853テスト通過）、clippy警告ゼロ。

---

### 20260311225740 - パフォーマンスチューニング
- **Started:** 2026-03-11 22:57:40
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260311225740_performance-tuning.md)
- **Summary:** Agent デプロイ SSH 6→2 exec、SSH フォールバック ツリー展開 D→1回、コンテンツ読み込み N→バッチ（batch_read共通ヘルパー）、rollback N→ceil(N/1000) exec、セッション一覧 N+1→1回。純粋関数8つ追加、+33テスト（1766テスト通過）、clippy警告ゼロ。

---

### 20260311212707 - テストカバレッジ向上（78% → 85%）
- **Started:** 2026-03-11 21:27:07
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260311212707_test-coverage-improvement.md)
- **Summary:** handler 層 0% カバレッジ解消（純粋関数切り出し）、CLI/Service/UI/Runtime テスト拡充。+209テスト（1624→1833）、15ファイル変更、+2,179行。オーケストレーターパターン（実装Agent→レビューAgent→修正ループ×5回）で品質管理。

---

### 20260311193028 - merge dry-run の RightOnly ファイル処理修正
- **Started:** 2026-03-11 19:30:28
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260311193028_fix-merge-dryrun-rightonly-handling.md)
- **Summary:** QA テスト（67パターン）で発見した dry-run 不整合2件を修正。RightOnly ファイルの merge/delete 処理を一貫化。

---

### 20260311181905 - CLI QA テスト結果に基づく改善
- **Started:** 2026-03-11 18:19:05
- **Status:** 🟢 Completed
- **Completed:** 2026-03-11
- **Plan:** [Link](./cycles/20260311181905_cli-qa-improvements.md)
- **Focus:** 65パターンの実機テストで発見した BUG 1件 + 改善 8件を一括修正。JSON出力の安全性・一貫性向上。

---

### 20260311170035 - sync CLIサブコマンド + 削除セマンティクス + --delete
- **Started:** 2026-03-11 17:00:35
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260311170035_sync-cli-and-delete-semantics.md)
- **Summary:** 1:N マルチサーバ同期 sync コマンド、削除セマンティクス明文化、--delete オプション（sync/merge 両方）

---

### 20260311160242 - CLI 改善: リモート rollback + 品質向上
- **Started:** 2026-03-11 16:02:42
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260311160242_cli-improvements-remote-rollback.md)
- **Summary:** Agent クライアント list_backups/restore_backup、Runtime リモート統合（SSH フォールバック付き）、diff --max-lines context除外カウント、status 3-way sensitive ref_badge、merge text format 改善、diff JSON total_files リネーム。1494テスト通過。

---

### 20260311144819 - バックアップ・ロールバック E2E テスト充実化
- **Started:** 2026-03-11 14:48:19
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260311144819_backup-rollback-e2e-tests.md)
- **Summary:** cli_rollback_local.rs（SSH不要6件）、cli_rollback.rs（SSH12件）を新設。cli_merge.rs にバックアップ構造検証テスト2件追加。merge→rollbackラウンドトリップ、--list/--dry-run/--force/--session、JSON出力構造、sensitiveスキップ、exit code、セッションディレクトリ構造を網羅。1494テスト通過、clippy警告ゼロ。

---

### 20260311124242 - rollback CLIサブコマンド
- **Started:** 2026-03-11 12:42:42
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260311124242_rollback-cli-subcommand.md)
- **Summary:** バックアップ構造をセッションディレクトリ方式に変更（フラットファイル → session_id/rel_path）。rollback CLIサブコマンド（--list/--dry-run/--force/--session/--format）。service/rollback.rs 純粋関数（mark_expired/plan_restore/rollback_exit_code）。Agent プロトコル v2（ListBackups/RestoreBackup）。セキュリティ: shell_escape/パストラバーサル防御/session_id 検証。1476テスト通過、clippy警告ゼロ。

---

### 20260311004829 - Merge Scan Agent 統合（SSH exec 1200回 → 3回）
- **Started:** 2026-03-11 00:48:29
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260311004829_merge-scan-agent-integration.md)
- **Summary:** agent_clients を Arc<Mutex<>> に変更し、TUI merge scan と CLI で Agent バッチ API を活用。agent_read_files_batch 共通ヘルパー（256ファイルずつ）、error_paths 交差セマンティクス（local∩remote 両方失敗のみ）、poison-safe lock パターン、AgentFailed メッセージ経由の invalidation。CLI は fetch_contents_tolerant でバッチ化。1414テスト通過、clippy警告ゼロ。

---

### 20260311000008 - Agent デプロイ堅牢性強化
- **Started:** 2026-03-11 00:00:08
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260311000008_agent-deploy-robustness.md)
- **Summary:** atomic write（.tmp → verify → mv）方式導入。SHA-256チェックサム照合、デバッグバイナリ検出、sha256sum不在時graceful degradation。openssl base64 -d の -A フラグ欠落によるbase64デコード破損を発見・修正。バージョン検証をclap出力形式に修正。1395テスト通過。

---

### 20260310233345 - SSH 大容量ファイル書き込みバグ修正
- **Started:** 2026-03-10 23:33:45
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260310233345_fix-ssh-large-file-write.md)
- **Summary:** write_file/write_file_bytes で channel.data() を 32KB チャンク分割送信に変更。send_data_chunked + send_and_finish_channel ヘルパーで DRY 化 + エラー時 channel.close() 保証。TestServer に data/eof ハンドラ追加、write_file E2E テスト3件 + 定数テスト1件追加。1476テスト通過、clippy警告ゼロ。

---

### 20260310220840 - Agent クロスコンパイル + E2E 動作確認
- **Started:** 2026-03-10 22:08:40
- **Completed:** 2026-03-11
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260310220840_agent-cross-compile-and-e2e.md)
- **Summary:** musl 静的リンクビルド成功（arboard feature gate 不要、strip後11MB）。resolve_binary_path() + 環境変数オーバーライド、checksum_cmd + chmod 700、ListTree マルチチャンクストリーミング、E2E プロセステスト8件 + SSH E2Eテスト4件追加。resolve_scan_root + validate_path に絶対パス拒否を追加（セキュリティ改善）。CI/release に musl ターゲット追加。1471テスト通過。

---

### 20260310211133 - Agent SSH Transport + Quick Check + TUI/CLI 統合
- **Started:** 2026-03-10 21:11:33
- **Completed:** 2026-03-10
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260310211133_agent-ssh-transport-and-quick-check.md)
- **Summary:** SshAgentTransport（async↔sync ブリッジ + TransportGuard）、deploy.rs 純粋関数、core.rs start_agent_via_ssh() 完全実装、TUI Agent ステータスフッター、CLI agent フィールド（AgentStatus enum）、--checksum オプション。1358テスト通過。コミット aeab147。

### 20260310190552 - Remote Agent Protocol（高速メタデータ一括取得 + Quick Check）
- **Started:** 2026-03-10 19:05:52
- **Completed:** 2026-03-10
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260310190552_remote-agent-protocol.md)
- **Summary:** Agent プロトコル基盤 Cycle 1 完了。protocol/framing/tree_scan/file_io/dispatch/server/client/deploy/config/CLI の11モジュール新規作成。152テスト追加（合計1319テスト）。

### 20260310154632 - 再接続後ディレクトリマージで [3-] バッジ不具合修正
- **Started:** 2026-03-10 15:46:32
- **Completed:** 2026-03-10
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260310154632_fix-reconnect-dir-merge-badge.md)
- **Summary:** 再接続後ディレクトリマージで [3-] バッジが表示される不具合の修正。コミット 6dcede2。

### 20260310135716 - E2E テスト充実化
- **Started:** 2026-03-10 13:57:16
- **Completed:** 2026-03-10
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260310135716_e2e-test-enrichment.md)
- **Summary:** E2Eテスト充実化完了。111テスト（SSH不要23 + SSH必要88）をテストシナリオごとに15ファイルに分離。

### 20260310120132 - TUI バグ修正: Enter 連打で diff 消失 + 再接続後マージで [3-] バッジ
- **Started:** 2026-03-10 12:01:32
- **Completed:** 2026-03-10
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260310120132_tui-bugfix-enter-and-ref-badge.md)
- **Summary:** Enter連打でdiff消失（カーソル復元不備）、再接続後マージで[3-]バッジ（ref_tree深さ不一致+conflict_cache残留）を修正。

### 20260310114351 - CLI 品質改善（merge . / バイナリ diff / help 説明追加）
- **Started:** 2026-03-10 11:43:51
- **Completed:** 2026-03-10
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260310114351_cli-quality-improvements.md)
- **Summary:** "." パス解決（path_resolver has_root_marker）、is_binary 不正UTF-8検出追加、CLIヘルプテキスト全オプション説明追加。1142テスト通過。コミット d62cd21。

---

### 20260310001606 - コンフリクト検知・表示（3way diff）
- **Started:** 2026-03-10 00:16:06
- **Completed:** 2026-03-10
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260310001606_conflict-detection.md)
- **Summary:** ref（共通祖先）と left/right の差分を比較し、両側が同じ行を異なる内容に変更した領域をコンフリクトとして検出・表示。diff/conflict.rs 純粋関数、TUI [C!] バッジ、CLI conflict_count/regions 出力。1136テスト通過。コミット 85ea8f4。

### 20260309235718 - グローバルログレベルオプション（--debug / -v / --log-level）
- **Started:** 2026-03-09 23:57:18
- **Completed:** 2026-03-10
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260309235718_global-log-level-options.md)
- **Summary:** -v / --debug / --log-level グローバルオプションを追加。環境変数なしでログレベルを制御可能に。コミット 979640f。

### 20260309222834 - CLI 出力一貫性修正: JSON/バイナリ/symlink/sensitive
- **Started:** 2026-03-09 22:28:34
- **Completed:** 2026-03-09
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260309222834_cli-output-consistency-fixes.md)
- **Summary:** CLI出力の4つの一貫性問題を修正。コミット 92cb52b。

### 20260309211045 - symlink merge ロジックのサービス層集約 + TUI 側バグ修正
- **Started:** 2026-03-09 21:10:45
- **Completed:** 2026-03-09
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260309211045_symlink-merge-logic-to-service-layer.md)
- **Summary:** symlink merge のビジネスロジックをCLI層からサービス層に移動。TUI側のsymlink mergeバグ修正。コミット dbf218b。

### 20260309193908 - CLI バイナリ status 誤判定 + symlink merge 破壊 + diff バイナリ文字化け修正
- **Started:** 2026-03-09 19:39:08
- **Completed:** 2026-03-09
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260309193908_cli-binary-symlink-diff-bugfix.md)
- **Summary:** CLI symlink merge保護(determine_merge_action+remove_file+バックアップ)。Step2+3は前セッションで実装済み。1070テスト通過。

### 20260309185022 - CLI 総合バグ修正: バイナリ破壊 + symlink比較 + r2r確認 + diff改善
- **Started:** 2026-03-09 18:50:22
- **Completed:** 2026-03-09
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260309185022_cli-comprehensive-bugfix.md)
- **Summary:** バイナリmerge(base64方式)、symlink比較修正、r2r確認ガード、diff改善。1046テスト通過。

### 20260309000006 - CLI バグ修正: exit code + diff Warning + 日本語エラー
- **Started:** 2026-03-09 00:00:06
- **Completed:** 2026-03-09
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260309000006_cli-bugfix-exit-codes-warnings-i18n.md)
- **Summary:** exit code修正(try_main wrapper)、diff Warning抑制、エラーメッセージ英語化。

---

## 📜 Session History

### 20260308225819 - --config オプション追加
- **Started:** 2026-03-08 22:58:19
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260308225819_cli-config-option.md)
- **Summary:** CLI/TUI に --config トップレベルオプション追加。load_config_with_project_override() 新設、run_* 署名変更、テスト8件追加。1005テスト通過。

### 20260308224111 - CLI レビュー指摘修正
- **Started:** 2026-03-08 22:41:11
- **Completed:** 2026-03-09
- **Status:** 🟢 Completed
- **Plan:** [Link](./cycles/20260308224111_cli-review-fixes.md)
- **Summary:** レビュー指摘 Medium 3件 + Low 2件の修正。T-1(binary exit code), T-3(binary出力), R-1(symlink分離), D-1(tolerant_io共通化), C-1(is_binary 8KBコメント)。

### 20260308214910 - CLI バグ修正: 末尾スラッシュ + diff/merge ステータス精緻化
- **Started:** 2026-03-08 21:49:10
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260308214910_cli-bugfix-trailing-slash-and-status-refinement.md](./cycles/20260308214910_cli-bugfix-trailing-slash-and-status-refinement.md)
- **Summary:** 末尾スラッシュ解決失敗と diff/merge ステータス偽陽性を修正。

### 20260308204041 - CLI ディレクトリ対応 + status Equal 除外 + --server 削除
- **Started:** 2026-03-08 20:40:41
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260308204041_cli-directory-and-filter-support.md](./cycles/20260308204041_cli-directory-and-filter-support.md)
- **Summary:** --server 削除、status --all、diff/merge ディレクトリ・複数パス対応、path_resolver.rs 新設、MultiDiffOutput 型追加。982テスト通過、clippy警告ゼロ。

### 20260308184823 - CLI UX 一貫性改善（3項目）
- **Started:** 2026-03-08 18:48:23
- **Completed:** 2026-03-09
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260308184823_cli-ux-consistency.md](./cycles/20260308184823_cli-ux-consistency.md)
- **Summary:** left==right 自己比較の拒絶(source_pair.rs check_same_side)、--left のみ指定時のフォールバック統一、merge --format json 追加。

### 20260308175828 - CLI 安全性強化（7項目一括対応）
- **Started:** 2026-03-08 17:58:28
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260308175828_cli-safety-hardening.md](./cycles/20260308175828_cli-safety-hardening.md)
- **Summary:** HashMap→BTreeMap、merge --left/--right必須化、dry-run出力改善、ref重複検知、diff片側不在トレラント、statusヘッダ行、help改善、Skill更新。943テスト通過、clippy警告ゼロ。

### 20260308165302 - CLI ref サーバ対応（status / diff / merge 3-way 出力）
- **Started:** 2026-03-08 16:53:02
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260308165302_cli-ref-server-support.md](./cycles/20260308165302_cli-ref-server-support.md)
- **Summary:** CLI status/diff/mergeに--refオプション追加。resolve_ref_source()、compute_ref_badges()、テキスト出力ref バッジ、JSON ref フィールドを実装。928テスト通過、clippy警告ゼロ。

### 20260308160024 - 3way サマリーパネル（W キー）
- **Started:** 2026-03-08 16:00:24
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260308160024_three-way-summary-panel.md](./cycles/20260308160024_three-way-summary-panel.md)
- **Summary:** 3way比較時の不一致箇所一覧パネル。Wキーでトグル表示、Enterで該当行にジャンプ。three_way_summary.rs + handler実装。

### 20260308145639 - サーバ切替時のツリー状態維持（展開・カーソル）
- **Started:** 2026-03-08 14:56:39
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260308145639_preserve-tree-state-on-server-switch.md](./cycles/20260308145639_preserve-tree-state-on-server-switch.md)
- **Summary:** サーバ切替（sキー）時にディレクトリ展開状態・カーソル位置が失われるUX問題を修正。再接続（rキー）の復元パターンを適用。

### 20260308122550 - Side::Remote("local") 不正状態の根絶
- **Started:** 2026-03-08 12:25:50
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260308122550_fix-side-remote-local-bug.md](./cycles/20260308122550_fix-side-remote-local-bug.md)
- **Summary:** Side::new() スマートコンストラクタ導入、全13箇所の直接構築を置換。コミット ffb58ef。

### 20260308022245 - Side-Agnostic I/O: local/remote 決め打ちの根絶
- **Started:** 2026-03-08 02:22:45
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260308022245_side-agnostic-io.md](./cycles/20260308022245_side-agnostic-io.md)
- **Summary:** CoreRuntime に Side ベース統一 I/O API（side_io.rs）を追加。handler 層・CLI を全面移行。AppState.server_name 廃止。reconnect/swap の Side::Local 対応。旧 API を pub(crate) に降格。828テスト通過、clippy警告ゼロ。

### 20260308002957 - Right↔Ref Swap + Equal時ref diff自動表示 + バッジ色分け
- **Started:** 2026-03-08 00:29:57
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260308002957_ref-preview-and-swap.md](./cycles/20260308002957_ref-preview-and-swap.md)
- **Summary:** right↔ref ワンキースワップ（X キー）、Equal時ref diff自動表示（read-only）、ディレクトリバッジの ref 差分色分け。805テスト通過、clippy警告ゼロ。

### 20260307215809 - 3way diff: バッジ表示 + ペア切り替え
- **Started:** 2026-03-07 21:58:09
- **Completed:** 2026-03-08
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260307215809_3way-diff-badges-and-pair-switching.md](./cycles/20260307215809_3way-diff-badges-and-pair-switching.md)
- **Summary:** 3way バッジ（ファイル・行レベル）、reference サーバ接続・キャッシュ、PairServerMenu 2列選択UI、--ref CLI引数。789テスト全通過。

### 20260307211246 - Phase 4-6: logs/events CLI + 構造化ログ
- **Started:** 2026-03-07 21:12:46
- **Completed:** 2026-03-07
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260307211246_phase4-6-logs-events-cli.md](./cycles/20260307211246_phase4-6-logs-events-cli.md)
- **Summary:** logs/events CLIサブコマンドと構造化ログ基盤を実装。Phase 4 完全完了。

### 20260307201911 - パス全体マッチ対応 exclude パターン
- **Started:** 2026-03-07 20:19:11
- **Completed:** 2026-03-07
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260307201911_path-aware-exclude-patterns.md](./cycles/20260307201911_path-aware-exclude-patterns.md)
- **Summary:** exclude パターンで config/*.toml や vendor/legacy/** のようなパス全体指定に対応。filter.rs 新設、should_exclude と is_path_excluded を集約。

### 20260307183825 - Phase 3: UX・堅牢性 残タスク一括実装
- **Started:** 2026-03-07 18:38:25
- **Completed:** 2026-03-07
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260307183825_phase3-ux-robustness.md](./cycles/20260307183825_phase3-ux-robustness.md)
- **Summary:** キーバインド整理(c=clipboard, r=reconnect/refresh)、レガシーSSHアルゴリズムヒント表示(ssh/hint.rs)、root_dir不在チェック(test -d)、パーミッション制御(MergeOptions+chmod_file+CLI --with-permissions)、クリップボードコピー(arboard+clipboard.rs)、Markdownレポート出力(report.rs+Shift+E)。671テスト全通過、clippy警告ゼロ。

### 20260307151653 - Phase 4: CLI + Skill（LLMエージェント連携）
- **Started:** 2026-03-07 15:16:53
- **Completed:** 2026-03-07
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260307151653_phase4-cli-subcommands.md](./cycles/20260307151653_phase4-cli-subcommands.md)
- **Summary:** CoreRuntime分離、Service層(status/diff/merge)、CLI(status/diff/merge)、TUI監視基盤(state.json/screen.txt/events.jsonl)、telemetry、Skill。logs/events CLIは後続サイクル(20260307211246)で実装完了。

### 20260307143110 - 責務分離リファクタリング
- **Started:** 2026-03-07 14:31:10
- **Completed:** 2026-03-07
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260307143110_responsibility-separation-refactoring.md](./cycles/20260307143110_responsibility-separation-refactoring.md)
- **Summary:** merge_scan.rs→ディレクトリモジュール化、main.rs→bootstrap.rs分離、hunk_ops.rs→undo.rs切り出し、dialog_ops.rs→server_switch.rs切り出し。全テスト通過、clippy警告ゼロ。

### 20260307005600 - Phase 2-4: サーバ間比較（remote ↔ remote）
- **Started:** 2026-03-07 00:56:00
- **Completed:** 2026-03-07
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260307005600_remote-to-remote-compare.md](./cycles/20260307005600_remote-to-remote-compare.md)
- **Summary:** Side enum抽象化、複数SSH接続(HashMap)、local/remote→left/rightフィールドリネーム、Badge/MergeDirectionリネーム、--leftオプション処理、リモート間マージ確認フロー、UIヘッダー動的表示。566テスト全通過、clippy警告ゼロ。

### 20260306223729 - ファイル名インクリメンタルサーチ
- **Started:** 2026-03-06 22:37:29
- **Completed:** 2026-03-07
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260306223729_file-search.md](./cycles/20260306223729_file-search.md)
- **Summary:** `/`キーでインクリメンタルサーチ、検索フィルタリング表示、Diff View内テキスト検索、検索ロジック共通化、Uncheckedバッジ更新修正。

### 20260306182441 - Phase 2-3: バイナリファイル + シンボリックリンク対応
- **Started:** 2026-03-06 18:24:41
- **Completed:** 2026-03-06
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260306182441_phase2-3-binary-symlink.md](./cycles/20260306182441_phase2-3-binary-symlink.md)
- **Summary:** SHA-256バイナリ比較、シンボリックリンクdiff/マージ/安全性検証（SharedTarget等5種警告）、[L]バッジ、ln -sfnリモートマージ。新規3ファイル+変更10ファイル。486テスト全通過、clippy警告ゼロ。

### 20260306173024 - Test Coverage Infrastructure + Gap Filling
- **Started:** 2026-03-06 17:30:24
- **Completed:** 2026-03-06
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260306173024_test-coverage-infrastructure.md](./cycles/20260306173024_test-coverage-infrastructure.md)
- **Summary:** cargo-llvm-cov導入、カバレッジスクリプト、CI coverageジョブ追加。app/配下7ファイル+UI dialog 2ファイルにテスト追加。307 -> 411テスト（+104）、行カバレッジ72.12% -> 76.40%。clippy警告ゼロ。

### 20260306160107 - ProgressDialog 設計改善 + UI ヘルパー整理
- **Started:** 2026-03-06 16:01:07
- **Completed:** 2026-03-06
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260306160107_progress-dialog-and-ui-cleanup.md](./cycles/20260306160107_progress-dialog-and-ui-cleanup.md)
- **Summary:** ProgressPhase enum導入、ProgressDialog::new()コンストラクタ、フッターヘルパー3種で5箇所のコピペ解消、files_found状態重複解消。全358テスト通過、clippy警告ゼロ。

### 20260306125842 - Phase 2-2: メタデータ表示 + バックアップ + 楽観的ロック
- **Started:** 2026-03-06 12:58:42
- **Completed:** 2026-03-06 16:00:00
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260306125842_phase2-2-metadata-backup-optimistic-lock.md](./cycles/20260306125842_phase2-2-metadata-backup-optimistic-lock.md)
- **Summary:** メタデータUI表示、マージ前バックアップ、楽観的ロック（mtime再チェック）の3機能。10ステップ実装。

### 20260306101436 - Diff Viewer シンタックスハイライト
- **Started:** 2026-03-06 10:14:36
- **Completed:** 2026-03-06 12:58:00
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260306101436_diff-syntax-highlight.md](./cycles/20260306101436_diff-syntax-highlight.md)
- **Summary:** syntect ベースのシンタックスハイライト + TuiPalette テーマシステム導入。theme/(palette/mod), highlight/(engine/convert/cache/mod) の6ファイル新規作成。diff_view/tree_view/render のハードコード色をパレット経由に置換。テーマ切替(T), ハイライトON/OFF(S) キーバインド追加。全245テスト通過、clippy警告ゼロ。

### 20260306002952 - UX致命的バグ修正 Round 4
- **Started:** 2026-03-06 00:29:52
- **Completed:** 2026-03-06 10:00:00
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260306002952_fix-ux-bugs-round4.md](./cycles/20260306002952_fix-ux-bugs-round4.md)
- **Summary:** resolve_remote_pathsパス検証漏れ修正、SSH接続安定性改善、バッチ読み込み、自動再接続、NodePresence導入

### 20260306000429 - リモートファイルI/O + ディレクトリマージ 致命的バグ修正
- **Started:** 2026-03-06 00:04:29
- **Completed:** 2026-03-06 00:29:00
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260306000429_fix-remote-io-and-merge.md](./cycles/20260306000429_fix-remote-io-and-merge.md)
- **Summary:** is_connected誤判定(SshExec vs SshConnection分離)、ツリー強制展開修正、write_file全件失敗修正、接続レジリエンス強化

### 20260305231004 - 非ブロッキング サブツリー走査 + プログレス表示
- **Started:** 2026-03-05 23:10:04
- **Completed:** 2026-03-06 00:00:00
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305231004_async-subtree-scan-progress.md](./cycles/20260305231004_async-subtree-scan-progress.md)
- **Summary:** ディレクトリ再帰マージ時の非ブロッキング化。scanner.rsパターン（スレッド+mpsc+poll）を再利用。MergeScanState/MergeScanMsg型、非同期走査スレッド、ポーリング、Escキャンセル、閾値ベースの同期/非同期切り替え。全201テスト通過、clippy警告ゼロ。

### 20260305224956 - UX 致命的改善 Round 3
- **Started:** 2026-03-05 22:49:56
- **Completed:** 2026-03-05 23:10:00
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305224956_ux-critical-fixes-round3.md](./cycles/20260305224956_ux-critical-fixes-round3.md)
- **Summary:** マージ方向キー反転、SSH KeepAlive+timeout 300秒、再接続後ツリーリストア、ディレクトリ再帰マージ+コンテンツロード。全201テスト通過。

### 20260305214250 - dialog.rs + ssh/client.rs 分割
- **Started:** 2026-03-05 21:42:50
- **Completed:** 2026-03-06
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305214250_split-dialog-ssh.md](./cycles/20260305214250_split-dialog-ssh.md)
- **Summary:** ui/dialog.rs (1282行) を7ファイルのディレクトリに分解、ssh/client.rs (1195行) から tree_parser.rs, known_hosts.rs, batch_read.rs を分離（575行に縮小）

### 20260305193207 - God Object 分解リファクタリング
- **Started:** 2026-03-05 19:32:07
- **Completed:** 2026-03-05 20:30:00
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305193207_decompose-god-objects.md](./cycles/20260305193207_decompose-god-objects.md)
- **Summary:** app.rs (2393行) と main.rs (1471行) の God Object を責務別モジュールに分解。app/ (9ファイル), runtime/ (3ファイル), handler/ (6ファイル), ui/render.rs に分離。全201テスト通過、clippy警告ゼロ、振る舞い変更なし。dialog.rs, ssh/client.rs の分割は別サイクルに延期。

### 20260305181303 - ディレクトリマージ + 変更ファイルフィルター
- **Started:** 2026-03-05 18:13:03
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305181303_directory-merge-and-diff-filter.md](./cycles/20260305181303_directory-merge-and-diff-filter.md)
- **Summary:** バッチマージ、Shift+F変更ファイルフィルター、センシティブファイル警告を実装。再接続時のステートリセット不足バグも修正。

### 20260305130756 - Viewport スクロール改善（VSCode準拠）
- **Started:** 2026-03-05 13:07:56
- **Completed:** 2026-03-05 14:10:09
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305130756_viewport-scroll-fix.md](./cycles/20260305130756_viewport-scroll-fix.md)
- **Summary:** diff_cursor/diff_scroll分離、VSCode準拠スクロールマージン（上下3行）、TreeViewも同ロジックで統一。Commit: 44172dc。

### 20260305173843 - GitHub Actions CI/Release
- **Started:** 2026-03-05 17:38:43
- **Completed:** 2026-03-05 17:57:32
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305173843_github-actions-ci-release.md](./cycles/20260305173843_github-actions-ci-release.md)
- **Summary:** push/PR時CI（fmt+clippy+test）、v*タグ時の自動リリース（Linux/macOS/Windows クロスビルド）、pre-commit/pre-pushフック追加。CI全グリーン確認済み。

### 20260305120550 - UX 改善 Round 2
- **Started:** 2026-03-05 12:05:50
- **Completed:** 2026-03-05
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305120550_ux-polish-round2.md](./cycles/20260305120550_ux-polish-round2.md)
- **Summary:** Equal時コンテンツ表示、フッターキーヒント、カーソルライン、Unified ハンクハイライトの4件。実装済み。

### 20260305105723 - UX 致命的バグ修正
- **Started:** 2026-03-05 10:57:23
- **Completed:** 2026-03-05 12:05:50
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305105723_ux-critical-fixes.md](./cycles/20260305105723_ux-critical-fixes.md)
- **Summary:** 6件の致命的UXバグ修正。j/k→1行スクロール, n/N→ハンクジャンプ、マージフロー改善（即時適用+undo）、SSH keep-alive、Badge拡張（Loading/Error）、ダイアログ方向バグ修正。全151テスト合格。

### 20260305020457 - Phase 2-1: ハンク単位マージ
- **Started:** 2026-03-05 02:04:57
- **Completed:** 2026-03-05 10:57:23
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305020457_phase2-1-hunk-merge.md](./cycles/20260305020457_phase2-1-hunk-merge.md)
- **Summary:** ハンク単位マージ + カーソルナビゲーション + ハイライト表示。Phase 2-1.5 UX品質改善（エラー表示・プレビュー確認・ヘルプ・スクロール・背景色・2ペイン）も実装。ただし致命的UXバグ6件を発見。

### 20260305014756 - Phase 1-4: initコマンド + フィルターTUI + タイムアウト改善
- **Started:** 2026-03-05 01:47:56
- **Completed:** 2026-03-05 02:04:57
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305014756_phase1-4-init-filter-tui-timeout.md](./cycles/20260305014756_phase1-4-init-filter-tui-timeout.md)
- **Summary:** initコマンドで対話的.remote-merge.toml生成、FilterPanel UI（fキー）、SSH/ローカルの30秒タイムアウト+10,000件エントリ制限、遅延読み込み、find_node_mut追加。全87テスト合格、clippy警告ゼロ。

### 20260305013200 - Phase 1-3: マージ機能 + 確認ダイアログ + サーバ切替
- **Started:** 2026-03-05 01:32:00
- **Completed:** 2026-03-05 02:00:00
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305013200_phase1-3-merge-dialog-server-switch.md](./cycles/20260305013200_phase1-3-merge-dialog-server-switch.md)
- **Summary:** SSH exec ベースでリモートファイル読み書き、merge/executor.rs（LeftMerge/RightMerge + パスサニタイズ）、確認ダイアログ + サーバ選択メニュー、TUI イベントループにマージ・サーバ切替統合。全84テスト合格、clippy 警告ゼロ。

### 20260305011612 - Phase 1-2: TUIフレームワーク + diff表示 + バッジ
- **Started:** 2026-03-05 01:16:12
- **Completed:** 2026-03-05 01:32:00
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305011612_phase1-2-tui-diff-badges.md](./cycles/20260305011612_phase1-2-tui-diff-badges.md)
- **Summary:** ratatui 0.30 + crossterm 0.29 + similar 2 導入。diff エンジン、AppState 一元管理、2ペインTUI（ツリー+diff）、差分バッジ、vim風キーバインド。全57テスト合格。

### 20260305005010 - Phase 1-1: プロジェクト基盤 + SSH接続 + ファイルツリー取得
- **Started:** 2026-03-05 00:50:10
- **Completed:** 2026-03-05 01:16:12
- **Status:** 🟢 Completed
- **Plan:** [docs/cycles/20260305005010_phase1-1-foundation.md](./cycles/20260305005010_phase1-1-foundation.md)
- **Summary:** Cargo構造確立、TOML設定パーサー、SSH鍵/パスワード認証、ファイルツリーデータ構造、ローカル/リモートツリー取得、インプロセスSSHテストサーバー。全34テスト合格。

---

## 🗺️ ロードマップ

### Phase 1 MVP 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **1-1** | プロジェクト基盤 + SSH接続 + ツリー取得 | 🟢 Completed |
| **1-2** | TUIフレームワーク + diff表示 + バッジ | 🟢 Completed |
| **1-3** | マージ機能 + 確認ダイアログ + サーバ切替 | 🟢 Completed |
| **1-4** | initコマンド + フィルターTUI + タイムアウト | 🟢 Completed |

### Phase 2 高度なマージ・比較機能
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **2-1** | ハンク単位マージ | 🟢 Done |
| **2-1.5** | UX品質改善 | 🟢 Done |
| **UX修正** | 致命的バグ修正 (6件) | 🟢 Done |
| **UX R2** | UX改善 Round 2 (4件) | 🟢 Done |
| **Scroll** | Viewport スクロール改善 | 🟢 Done |
| **2-2** | メタデータ表示 + バックアップ + 楽観的ロック | 🟢 Done |
| **2-3** | バイナリ + シンボリックリンク対応 | 🟢 Done |
| **2-4** | サーバ間比較（remote ↔ remote） | 🟢 Done |

### Phase 3 UX・堅牢性 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **Search** | ファイル名インクリメンタルサーチ | 🟢 Done |
| **Refactor** | 責務分離リファクタリング | 🟢 Done |
| **UX残タスク** | SSHヒント・root_dirチェック・パーミッション・クリップボード・レポート | 🟢 Done |

### Phase 4 CLI + Skill（LLMエージェント連携） 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **4-1** | CoreRuntime分離 + Service層基盤 + 型定義 | 🟢 Done |
| **4-2** | status サービス + CLI | 🟢 Done |
| **4-3** | diff サービス + CLI | 🟢 Done |
| **4-4** | merge サービス + CLI | 🟢 Done |
| **4-5** | TUI監視基盤 (state/screen dump) | 🟢 Done |
| **4-6** | ログ + イベントストリーム | 🟢 Done |
| **4-7** | Skill ファイル | 🟢 Done |

### Phase 2 残タスク 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **3way-1** | 3way diff バッジ表示 + ペア切り替え | 🟢 Done |
| **3way-1.5** | Right↔Ref Swap + Equal時ref diff + バッジ色分け | 🟢 Done |
| **3way-2** | 3way サマリーパネル (W キー) | 🟢 Done |
| **conflict** | コンフリクト検知・表示 | 🟢 Done |

### Phase 4 追加: CLI ref サーバ対応 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **4-ref** | CLI status/diff/merge の --ref 3-way 出力対応 | 🟢 Done |

### CLI 安全性強化 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **safe-1** | HashMap → BTreeMap（デフォルトサーバ不定問題） | 🟢 Done |
| **safe-1.5** | merge で --left/--right 両方必須化（破壊的操作の安全ネット） | 🟢 Done |
| **safe-2** | merge --dry-run 出力改善 | 🟢 Done |
| **safe-3** | ref 重複検知（ref_guard.rs 共通化） | 🟢 Done |
| **safe-4** | diff 片側不在トレラント | 🟢 Done |
| **safe-4.5** | status テキスト出力にヘッダ行追加（比較先明示） | 🟢 Done |
| **safe-5** | --ref help 説明改善 | 🟢 Done |
| **safe-6** | Skill ファイル更新（merge 例の同期） | 🟢 Done |

### CLI UX 一貫性改善 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **ux-1** | left==right 自己比較の拒絶 | 🟢 Done |
| **ux-2** | --left のみ指定時のフォールバック統一 | 🟢 Done |
| **ux-3** | merge --format json 追加 | 🟢 Done |

### CLI ディレクトリ対応 + status Equal 除外 + --server 削除 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **dir-1** | --server 削除（--right に統一） | 🟢 Done |
| **dir-2** | status --all + Equal 除外 | 🟢 Done |
| **dir-3** | path_resolver 新設 | 🟢 Done |
| **dir-4** | MultiDiffOutput 型追加 | 🟢 Done |
| **dir-5** | diff ディレクトリ・複数パス対応 | 🟢 Done |
| **dir-6** | merge ディレクトリ・複数パス対応 | 🟢 Done |
| **dir-7** | Skill ファイル更新 | 🟢 Done |

### TUI UX: 非同期バッジスキャン 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **badge-1** | BadgeScanMsg 型定義 + 純粋関数 | 🟢 Done |
| **badge-2** | ワーカースレッド + start/cancel + ポーリング | 🟢 Done |
| **badge-3** | TuiRuntime 統合 + ハンドラ連携 | 🟢 Done |
| **badge-4** | 再接続時全スキャンキャンセル | 🟢 Done |

### CLI バグ修正: 末尾スラッシュ + ステータス精緻化 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **fix-1** | path_resolver 末尾スラッシュ正規化 | 🟢 Done |
| **fix-2** | diff.rs ステータス精緻化 | 🟢 Done |
| **fix-3** | merge.rs ステータス精緻化 | 🟢 Done |

### Phase 5: 運用・同期機能 🟢 Complete
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **5-1** | --debug / -v / --log-level グローバルオプション | 🟢 Done |
| **5-2** | 削除セマンティクス明文化（デフォルト: 削除しない） | 🟢 Done |
| **5-3** | rollback CLIサブコマンド | 🟢 Done |
| **5-3.5** | リモート rollback + CLI 品質改善 | 🟢 Done |
| **5-4** | sync CLIサブコマンド（1:N マルチサーバ同期） | 🟢 Done |
| **5-5** | --delete オプション（完全同期） | 🟢 Done |
| **5-6** | CLI QA テスト改善（JSON安全性・出力一貫性） | 🟢 Done |

### Phase 6: Remote Agent Protocol 🟢 Complete (All Steps)
| サブフェーズ | 内容 | 状態 |
|------------|------|------|
| **A** | プロトコル基盤 + agent サブコマンド | 🟢 Done |
| **B** | クライアント + デプロイ + 統合 | 🟢 Done |
| **C** | SSH Transport + Quick Check + TUI/CLI統合 | 🟢 Done |
| **D** | クロスコンパイル + E2E 動作確認 | 🟢 Done |
| **E** | デプロイ堅牢性（atomic write + checksum） | 🟢 Done |
| **F** | Merge Scan Agent 統合 | 🟢 Done |

---

## 🔗 Quick Links

- [Spec](../spec.md)
- [CLAUDE.md](../CLAUDE.md)
- [All Cycles](./cycles/)
- [Project Root](../)

---

**Note:** このファイルは `plan` skill によって自動管理されています。
