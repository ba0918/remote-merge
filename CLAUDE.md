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

## Implementation Phases

Phase 1 (MVP) → Phase 2 (hunk merge, 3-way diff) → Phase 3 (UX/robustness) → Phase 4 (CLI subcommands for LLM agents). See spec.md "実装フェーズ" section for details.
