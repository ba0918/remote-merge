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
