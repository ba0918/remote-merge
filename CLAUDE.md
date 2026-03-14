# CLAUDE.md

## Project

Rust CLI/TUI tool for displaying and merging file diffs between local and remote servers via SSH. Supports multi-server comparison (local, develop, staging, release) with interactive TUI and non-interactive CLI modes.

Full spec: `spec.md` | Progress: `docs/status.md`

## Tech Stack

Rust, ratatui, tokio, russh, similar, toml+serde, anyhow, tracing

## Commands

```
cargo build / cargo run / cargo test / cargo clippy / cargo fmt
cargo nextest run              # nextest でテスト実行（推奨）
cargo run -- --server develop      # specify server
cargo test <name>                  # single test
```

## Design Principles — MANDATORY, NO EXCEPTIONS

### 1. Compose small parts into larger parts
- Goal: prevent mixed responsibilities, NOT minimize line count
- 200 lines is a heuristic signal, not a hard limit. Do NOT split just to reduce lines
- A module with a single responsibility MAY exceed 200 lines

### 2. No business logic in glue code
- Each module has ONE responsibility
- Upper modules compose domain logic only — no business logic in routing/wiring

### 3. Layer separation
- **Domain**: pure logic, no side effects
- **Service**: domain composition + I/O
- **Handler**: thin event → service translation
- **UI**: rendering only

### 4. Testability
- Small modules = easy to test. Few dependencies = no mocks needed
- Domain logic MUST be pure functions testable by input/output alone
- **Every code change MUST include tests. Commits without tests are FORBIDDEN**
- If you cannot write a test → the design is wrong. Extract logic into pure functions

### 5. UI text in English
- All user-facing text (dialogs, status, errors, CLI help) MUST be English
- Code comments and doc comments may be Japanese

## Behavioral Rules — MANDATORY

### Do NOT start implementation during design discussions
- When the conversation is about design, planning, or architecture: ONLY discuss, do NOT write code
- Wait for explicit user approval (e.g., "implement it", "do it", "go ahead") before coding
- If unclear whether to discuss or implement: ASK

### Commit only requested files
- NEVER include files the user did not ask to change (README, docs/, status.md, etc.)
- Before committing, list the files you plan to include
- If in doubt about scope: ASK

## Architecture

### Connection Model
- SSH exec: tree listing + file content retrieval (lightweight)
- SFTP: file transfer (merge operations only)
- Lazy loading: directory contents on expand, file contents on demand

### Dual Interface
1. TUI (default): two-pane diff viewer with file tree, hunk-level merge
2. CLI (subcommands: `status`, `diff`, `merge`): JSON output for LLM agent integration

### Config Hierarchy
- Global: `~/.config/remote-merge/config.toml`
- Project: `.remote-merge.toml` (overrides global; `[filter]` merged as union)

### Key Design Decisions
- No auto-reconnect on SSH disconnect (safety)
- Optimistic locking on merge (re-check remote mtime before write)
- Symlinks compared by link target path, not dereferenced content
- Binary files use SHA-256 hash comparison
- Sensitive files (`.env`, `*.pem`) trigger warning before merge/diff
- Remote-to-remote merge requires server name confirmation (bypass: `--force`)
- CLI uses `--left`/`--right` consistently (not `--from`/`--to`)

## Commit Messages

Format: Conventional Commits. Language: **Japanese**.

```
<type>: <Japanese summary>

<body in Japanese (optional)>
```

Types: `feat|fix|refactor|docs|test|style|perf|chore`

Rules:
- Subject: concise Japanese
- Body: Japanese, explain why/context
- No footer (no Co-Authored-By) by default
- MUST pass pre-commit hooks (fmt + clippy + tests). `--no-verify` is FORBIDDEN
- Pre-commit hook runs full test suite (2300+ tests). Use `timeout: 600000` (10 min) for `git commit` calls to avoid timeout failures

Example:
```
feat: exclude パターンでパス全体マッチに対応

config/*.toml や vendor/legacy/** のようなパスパターンが
ローカル・リモート両方の遅延読み込みで動作するようにした。
```

## Test Environment (testenv/)

CentOS 5.11 Docker container for legacy/load testing.

Prerequisites: WSL2 with `kernelCommandLine = vsyscall=emulate` in `.wslconfig`, Docker

```
cd testenv && ./setup.sh     # full setup (100k files, takes minutes)
docker compose down           # stop
```

Test commands:
```
cargo run -- --config testenv/config.toml diff --right centos5 app/controllers/file_0.php
cargo run -- --config testenv/config.toml status --right centos5
```

Specs: CentOS 5.11, bash 3.2, OpenSSH 4.3 (no ed25519 — uses RSA), ARG_MAX 131072, 100k remote files, 500 local files, ~99.6k RightOnly. Legacy kex configured in config.toml.

## Implementation Phases

Phase 1 (MVP) → Phase 2 (hunk merge, 3-way diff) → Phase 3 (UX/robustness) → Phase 4 (CLI for LLM agents). See spec.md for details.
