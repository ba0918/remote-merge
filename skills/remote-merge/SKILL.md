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

Options: `--summary` (counts only, saves tokens), `--server <name>`, `--left <side> --right <side>`.

### 2. Inspect individual files

```bash
remote-merge diff <path> --format json --max-lines 200
```

Process one file at a time to manage context window.

### 3. Merge

```bash
remote-merge merge <path> --dry-run   # preview first
remote-merge merge <path>             # execute (local -> remote)
```

Sensitive files (`.env`, `*.pem`) auto-skipped; use `--force` to override. Backups created automatically. Optimistic locking checks mtime before writing.

### 4. Verify

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
