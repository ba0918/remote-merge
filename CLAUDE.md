# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`remote-merge` is a Rust CLI/TUI tool for graphically displaying and merging file differences between local and remote servers via SSH. It compares file trees across multiple servers (local, develop, staging, release) and supports both interactive TUI and non-interactive CLI modes.

**Status:** Pre-implementation (design phase). See `spec.md` for the full specification.

## Tech Stack

- **Language:** Rust (single binary distribution)
- **TUI:** ratatui
- **Async:** tokio
- **SSH/SFTP:** russh
- **Diff:** similar
- **Config:** toml + serde
- **Error handling:** anyhow
- **Logging:** tracing

## Build & Development Commands

```bash
cargo build                  # Build
cargo run                    # Run TUI (default)
cargo run -- --server develop  # Run with server specified
cargo test                   # Run all tests
cargo test <test_name>       # Run single test
cargo clippy                 # Lint
cargo fmt                    # Format
```

## Design Principles (MUST FOLLOW)

以下の原則は **全てのコード変更に対して適用** される。例外なし。

### 1. 小さいパーツの組み合わせで大きいパーツを構成する
- **行数を抑えることが目的ではなく、責務を混在させないことが目的**
- 200行は目安であり兆候。行数削減のための分割は本末転倒
- 責務が単一なら200行を超えていても問題ない

### 2. 責務が混在する場所にビジネスロジックを書かない
- 各モジュールは **単一責務** を持つ
- 上位モジュールは **ドメインロジックの組み合わせ** だけで構成する
- グルーコード（接続・ルーティング）にビジネスロジックを混ぜない

### 3. レイヤー分離
- **ドメイン層**: 純粋なロジック・計算・判定（副作用なし）
- **サービス層**: ドメインの組み合わせ + I/O操作
- **ハンドラ層**: イベント → サービス呼び出しの薄い変換
- **UI層**: 描画のみ

### 4. テスタビリティ
- 小さいモジュール = テストが書きやすい
- 依存が小さい = モック不要
- ドメインロジックは入出力だけでテストできる純粋関数にする
- **コード変更時は必ずテストを書く。テストなしのコミットは禁止**
- テストが書けない = 設計が悪い。ロジックを純粋関数に切り出してテスト可能にする

### 5. UI text in English
- All user-facing text (dialogs, status messages, error messages, CLI help) **MUST be in English**
- Code comments and doc comments may be in Japanese
- This ensures consistency across the TUI and CLI interfaces

## Architecture

### Connection Model

- **SSH exec** for tree listing and file content retrieval (lightweight)
- **SFTP** for file transfer (merge operations only)
- Lazy loading: only fetch directory contents on expand, file contents on demand

### Dual Interface

1. **TUI mode** (default): Two-pane diff viewer with file tree, supports hunk-level merge
2. **CLI mode** (subcommands: `status`, `diff`, `merge`): JSON output designed for LLM agent integration

### Config Hierarchy

- Global: `~/.config/remote-merge/config.toml`
- Project: `.remote-merge.toml` (overrides global; `[filter]` sections are merged as union)

### Key Design Decisions

- **No auto-reconnect on SSH disconnect** — intentional for safety
- **Optimistic locking** on merge — re-checks remote file mtime before writing
- **Symlinks** are compared by link target path, not dereferenced content
- **Binary files** use SHA-256 hash comparison instead of content diff
- **Sensitive files** (`.env`, `*.pem`, etc.) trigger warning before merge/diff
- **Remote-to-remote merge** requires server name confirmation (bypass with `--force`)
- CLI uses `--left`/`--right` consistently across all subcommands (not `--from`/`--to`)

## Commit Message Rules

コミットメッセージは **日本語** で書く。フォーマットは Conventional Commits に従う。

### フォーマット

```
<type>: <日本語の要約>

<本文（任意）>
```

### タイプ一覧

| type | 用途 |
|------|------|
| `feat` | 新機能 |
| `fix` | バグ修正 |
| `refactor` | 機能追加もバグ修正もないコード変更 |
| `docs` | ドキュメントのみの変更 |
| `test` | テストの追加・修正 |
| `style` | コードの意味に影響しない変更（フォーマット等） |
| `perf` | パフォーマンス改善 |
| `chore` | ビルドプロセスやツールの変更 |

### ルール

- subject（要約行）は日本語で簡潔に書く
- body（本文）も日本語。変更の理由や背景を書く
- フッター（Co-Authored-By 等）はデフォルトで付けない
- pre-commit hook（fmt + clippy）を通すこと。`--no-verify` 禁止

### 例

```
feat: exclude パターンでパス全体マッチに対応

config/*.toml や vendor/legacy/** のようなパスパターンが
ローカル・リモート両方の遅延読み込みで動作するようにした。
filter.rs を新設し、should_exclude と is_path_excluded を集約。
```

## Test Environment (testenv/)

CentOS 5.11 Docker コンテナによるレガシー環境負荷テスト。

### 前提

- WSL2: `.wslconfig` に `kernelCommandLine = vsyscall=emulate` が必要
- Docker

### Quick Start

```bash
cd testenv && ./setup.sh          # フルセットアップ（10万ファイル生成含む、数分かかる）
docker compose down               # 停止
```

### テストコマンド

```bash
# 1ファイル diff（P1: フルスキャン問題の検証）
cargo run -- --config testenv/config.toml diff --right centos5 app/controllers/file_0.php

# ステータス（P2: batch_read チャンク分割の検証）
cargo run -- --config testenv/config.toml status --right centos5
```

### 環境スペック

| 項目 | 値 |
|------|-----|
| OS | CentOS 5.11 (bash 3.2, OpenSSH 4.3) |
| ARG_MAX | 131072 (128KB) |
| リモートファイル数 | 100,000 |
| ローカルファイル数 | 500 |
| RightOnly | ~99,600 |

### 注意

- CentOS 5 の OpenSSH 4.3 は **ed25519 未対応**（RSA 鍵を使用）
- レガシー Kex のみ対応 → config.toml に `ssh_options.kex_algorithms` 設定済み

## Implementation Phases

Phase 1 (MVP) → Phase 2 (hunk merge, 3-way diff) → Phase 3 (UX/robustness) → Phase 4 (CLI subcommands for LLM agents). See spec.md "実装フェーズ" section for details.
