# Project Status

**Last Updated:** 2026-04-06

---

## 🎯 Current Session

**Plan:** [CLI Hunk Merge](./plans/20260406123146_cli-hunk-merge.md)  
**Status:** 🔵 Implementing  
**Goal:** CLI merge コマンドに `--hunks` オプションを追加し、hunk 単位の部分マージを可能にする

---

## 📜 Session History

_Archived sessions can be found in [session-history.md](./session-history.md)._

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
