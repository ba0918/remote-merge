---
name: remote-merge
description: >
  Operate the remote-merge CLI to compare and merge files between local and remote servers via SSH.
  Use when the user asks to: check file differences with a remote server, merge local changes to remote,
  inspect diffs between servers, monitor a running remote-merge TUI session, or diagnose SSH/merge issues.
  Triggers: "remote-merge", "compare with server", "push to remote", "sync files",
  "check remote diff", "merge to server", "monitor TUI", "deploy changes".
---

# remote-merge

CLI/TUI tool for comparing and merging files between local and remote servers via SSH.

## Autonomous Workflow

Follow this progression. Always use `--format json` for machine-parseable output.

### 1. Discover differences

```bash
remote-merge status --format json
```

Exit codes: 0 = no diff, 1 = diffs found, 2 = error.

Options:
- `--summary` — counts only (saves tokens), omits `files` array
- `--left <side> --right <side>` — specify comparison sides (e.g., `local`, `develop`, `staging`, `release`)
- `--all` — include Equal files in output (by default, Equal files are excluded)
- `--checksum` — force content comparison for all files (bypass mtime/size quick check). Useful when timestamps are unreliable.

### 2. Inspect diffs

Diff supports single files, multiple files, directories, or all files (no path = root).

```bash
# Single file
remote-merge diff src/main.rs --left local --right develop --format json --max-lines 200

# Multiple files
remote-merge diff src/main.rs src/lib.rs --left local --right develop --format json

# Directory (all files under src/)
remote-merge diff src/ --left local --right develop --format json

# All files (no path argument = project root)
remote-merge diff --left local --right develop --format json
```

`--max-files 100` is the default limit. Use `--max-files 0` for unlimited.

The output is a `MultiDiffOutput` containing a `files` array, `summary`, `truncated` flag, and `changed_files_total`. See [references/json-schemas.md](references/json-schemas.md) for the full schema.

**Note on glob:** Shell glob expansion is used (the CLI does not implement glob internally). Remote path glob is not supported — always specify explicit paths or directories.

### 3. Merge

Merge requires at least one path (no default to root, for safety).

```bash
# Preview first (dry-run)
remote-merge merge src/main.rs --left local --right develop --dry-run

# Single file
remote-merge merge src/main.rs --left local --right develop

# Multiple files
remote-merge merge src/main.rs src/lib.rs --left local --right develop

# Directory
remote-merge merge src/ --left local --right develop
```

Sensitive files (`.env`, `*.pem`) auto-skipped; use `--force` to override. Backups created automatically. Optimistic locking checks mtime before writing.

### 4. Rollback

Undo a merge by restoring files from backup sessions.

```bash
# List backup sessions
remote-merge rollback --list --target develop --format json

# Preview what would be restored (dry-run)
remote-merge rollback --target develop --dry-run --format json

# Restore latest session
remote-merge rollback --target develop --force --format json

# Restore specific session
remote-merge rollback --target develop --session 20260311-140000 --force --format json
```

Options:
- `--target <side>` — restore target (required except for `--list`)
- `--list` — list backup sessions without restoring
- `--session <id>` — specific session to restore (default: latest non-expired)
- `--dry-run` — preview without executing
- `--force` — skip confirmation, allow expired/sensitive files
- `--format text|json` — output format (default: text)

Exit codes: 0 = success, 2 = error (partial or total failure).

Backup structure: `.remote-merge-backup/{session_id}/{relative_path}` (session directory per merge operation).

### 5. Verify

```bash
remote-merge status --format json
```

## TUI Monitoring

### CLI commands (preferred)

```bash
# Logs — debug.log is JSONL; text output is the default, use --format json for machine parsing
remote-merge logs --format json                  # all logs (JSONL)
remote-merge logs --format json --level error    # errors only
remote-merge logs --format json --since 5m       # last 5 minutes
remote-merge logs --format json --tail 50        # last 50 entries

# Events — always JSONL output
remote-merge events                              # all events
remote-merge events --type error                 # error events only
remote-merge events --type key_press --since 5m  # recent key presses
remote-merge events --tail 100                   # last 100 events
```

Duration shorthand for `--since`: `30s`, `5m`, `1h`, `2d`.

### Dump files (alternative)

Read directly at `~/.cache/remote-merge/`:

| File | Content | Command |
|------|---------|---------|
| `state.json` | App state snapshot | `cat ~/.cache/remote-merge/state.json` |
| `screen.txt` | Plain text screen | `cat ~/.cache/remote-merge/screen.txt` |
| `events.jsonl` | Event stream (JSONL) | `remote-merge events --type error` |
| `debug.log` | Application logs (JSONL) | `remote-merge logs --format json --level error` |

Event types: `key_press`, `render_slow`, `error`, `dialog`, `state_change`.

## Exit Codes

0 = success (no diff), 1 = success (diffs found), 2 = error.

## Error Recovery

- **SSH connection failed** -> check `.remote-merge.toml`
- **Sensitive file skipped** -> add `--force`
- **Optimistic lock failed** -> retry (file changed during merge)
- **TUI unresponsive** -> check `state.json` for `is_connected`, inspect `debug.log`

## JSON Schemas

See [references/json-schemas.md](references/json-schemas.md) for complete JSON output structures.
