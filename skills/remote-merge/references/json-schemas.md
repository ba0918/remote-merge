# JSON Output Schemas

## status

```json
{
  "left":  { "label": "local", "root": "/home/user/app" },
  "right": { "label": "develop", "root": "dev:/var/www/app" },
  "ref":   { "label": "staging", "root": "stg:/var/www/app" },
  "agent": "connected",
  "files": [
    { "path": "src/config.ts", "status": "modified", "sensitive": false },
    { "path": ".env", "status": "modified", "sensitive": true, "ref_badge": "ref_differs" }
  ],
  "summary": {
    "modified": 2,
    "left_only": 0,
    "right_only": 1,
    "equal": 10,
    "ref_differs": 1,
    "ref_only": 0,
    "ref_missing": 0
  }
}
```

File status values: `modified`, `left_only`, `right_only`, `equal`.

Optional fields (omitted when null/not applicable):
- `ref` — reference server info (present only with `--ref`)
- `agent` — Agent connection status: `"connected"` or `"fallback"` (present only for remote servers)
- `files[].ref_badge` — reference badge (present only with `--ref`)
- `summary.ref_differs`, `summary.ref_only`, `summary.ref_missing` — reference comparison counts (present only with `--ref`)

With `--summary`, `files` is omitted.

With `--checksum`, all files are compared by content regardless of mtime/size. Files that appear `equal` by metadata may become `modified`.

## diff

Diff always returns a `MultiDiffOutput` wrapper, even for a single file.

```json
{
  "files": [
    {
      "path": "src/config.ts",
      "left":  { "label": "local", "root": "/home/user/app" },
      "right": { "label": "develop", "root": "dev:/var/www/app" },
      "sensitive": false,
      "truncated": false,
      "hunks": [
        {
          "index": 0,
          "left_start": 10,
          "right_start": 10,
          "lines": [
            { "type": "context", "content": "  function hello() {" },
            { "type": "removed", "content": "  old line" },
            { "type": "added",   "content": "  new line" }
          ]
        }
      ]
    }
  ],
  "summary": {
    "scanned_files": 5,
    "files_with_changes": 1
  },
  "truncated": true,
  "changed_files_total": 3
}
```

- `files`: array of per-file diff outputs
- `summary.scanned_files`: total number of files scanned (including equal/unchanged files)
- `summary.files_with_changes`: number of files that have at least one hunk
- `truncated`: true when `--max-files` limit was reached (default: 100; use `--max-files 0` for unlimited). Omitted when false.
- `changed_files_total`: total number of changed files before truncation. Present only when `truncated` is true.
- Line types within hunks: `context`, `added`, `removed`. Per-file `truncated` is true when `--max-lines` was hit.

### Sensitive file (without --force)

```json
{
  "path": ".env",
  "sensitive": true,
  "truncated": false,
  "hunks": [],
  "note": "Content hidden (sensitive file). Use --force to show."
}
```

### Binary file

```json
{
  "path": "assets.bin",
  "sensitive": false,
  "binary": true,
  "truncated": false,
  "hunks": [],
  "left_hash": "149d4736...",
  "right_hash": "d6d73d23..."
}
```

### Symlink

```json
{
  "path": "readme_link",
  "sensitive": false,
  "symlink": true,
  "truncated": false,
  "hunks": [],
  "left_symlink_target": "../README.md",
  "right_symlink_target": "../README.md"
}
```

## merge

```json
{
  "merged": [
    { "path": "src/config.ts", "status": "ok", "backup": "20260311-140000/src/config.ts" }
  ],
  "skipped": [
    { "path": ".env", "reason": "sensitive file" }
  ],
  "deleted": [
    { "path": "old-file.ts", "status": "ok", "backup": "20260311-140000/old-file.ts" }
  ],
  "failed": [
    { "path": "broken.ts", "error": "optimistic lock failed" }
  ]
}
```

- `merged[].backup`: backup path in format `{session_id}/{relative_path}` (inside `.remote-merge-backup/` directory)
- `merged[].status`: `"ok"` on success, `"would merge"` in dry-run mode
- `merged[].ref_badge`: optional reference badge (present only with `--ref`)
- `skipped`: files skipped (sensitive files without `--force`, remote-to-remote without `--force`)
- `deleted`: files deleted by `--delete` flag. Omitted when empty. Each entry has `path`, `status` (`"ok"` or `"failed"`), and optional `backup` path.
- `failed`: files that failed to merge or delete with error details
- `ref`: optional reference server info (present only with `--ref`)

## sync

```json
{
  "left": { "label": "local", "root": "/home/user/app" },
  "targets": [
    {
      "target": { "label": "server1", "root": "srv1:/var/www/app" },
      "merged": [
        { "path": "src/config.ts", "status": "ok", "backup": "20260311-140000/src/config.ts" }
      ],
      "skipped": [
        { "path": ".env", "reason": "sensitive file (use --force to include)" }
      ],
      "deleted": [
        { "path": "old-file.ts", "status": "ok", "backup": "20260311-140000/old-file.ts" }
      ],
      "failed": [
        { "path": "broken.ts", "error": "permission denied" }
      ],
      "status": "success"
    },
    {
      "target": { "label": "server2", "root": "srv2:/var/www/app" },
      "merged": [],
      "skipped": [],
      "failed": [
        { "path": "src/config.ts", "error": "connection lost" }
      ],
      "status": "failed"
    }
  ],
  "summary": {
    "total_servers": 2,
    "successful_servers": 1,
    "total_files_merged": 1,
    "total_files_deleted": 1,
    "total_files_failed": 1
  }
}
```

- `targets[]`: per-server sync results
- `targets[].status`: `"success"` (all files OK or no changes), `"partial"` (some merged, some failed), `"failed"` (all failed)
- `targets[].deleted`: files deleted by `--delete`. Omitted when empty.
- `targets[].deleted[].status`: `"ok"` or `"failed"`
- `targets[].deleted[].backup`: backup path (omitted when backup is disabled or not applicable)
- `summary`: aggregate counts across all servers
- Connection failures appear as targets with empty `merged` and error details in `failed`

## rollback --list

```json
{
  "target": { "label": "develop", "root": "dev:/var/www/app" },
  "sessions": [
    {
      "session_id": "20260311-140000",
      "files": [
        { "path": "src/config.ts", "size": 1234 }
      ]
    }
  ]
}
```

- `target`: SourceInfo object with `label` and `root`
- `sessions`: array of backup sessions, sorted newest first
- `sessions[].expired`: boolean, present when true (session older than retention period). Omitted when false.
- `sessions[].files`: list of backed-up files with their sizes. May be empty if the merge created a new file on the target (no original to back up).

## rollback (restore)

```json
{
  "target": { "label": "develop", "root": "dev:/var/www/app" },
  "session_id": "20260311-140000",
  "restored": [
    { "path": "src/config.ts", "pre_rollback_backup": "20260311-150000" }
  ],
  "skipped": [
    { "path": ".env", "reason": "sensitive file (use --force to override)" }
  ],
  "failed": [
    { "path": "broken.ts", "error": "file not found in backup" }
  ]
}
```

- `target`: SourceInfo object with `label` and `root`
- `restored[].pre_rollback_backup`: session ID of the safety backup created before restoring (so the restore itself can be undone)
- `skipped`: files skipped. Omitted when empty.
- `failed`: files that failed to restore. Omitted when empty.

## state.json (TUI dump)

```json
{
  "focus": "file_tree",
  "left_source": "local",
  "right_source": "develop",
  "is_connected": true,
  "status_message": "local <-> develop | Tab: switch focus | q: quit",
  "has_dialog": false,
  "dialog_kind": null,
  "selected_path": "src/config.ts",
  "tree_cursor": 3,
  "diff_scroll": 0,
  "diff_cursor": 0,
  "hunk_cursor": 0,
  "diff_mode": "unified",
  "scan_state": "idle",
  "merge_scan_state": "idle",
  "diff_filter_mode": false,
  "tree_files": [
    { "path": "src/config.ts", "name": "config.ts", "is_dir": false, "badge": "[M]" }
  ],
  "file_counts": {
    "modified": 5, "equal": 20, "left_only": 1, "right_only": 2,
    "unchecked": 0, "error": 0
  }
}
```

Badges: `[M]` modified, `[=]` equal, `[+]` left only, `[-]` right only, `[?]` unchecked, `[!]` error.

## debug.log (JSONL)

Each line is one JSON object (structured tracing log):

```json
{"timestamp":"2026-03-07T21:00:00.123Z","level":"INFO","target":"ssh::client","message":"connected to develop","fields":{}}
{"timestamp":"2026-03-07T21:00:01.456Z","level":"ERROR","target":"ssh::client","message":"connection timeout","fields":{"elapsed_ms":30000}}
```

Fields: `timestamp` (ISO 8601), `level` (TRACE/DEBUG/INFO/WARN/ERROR), `target` (module path), `message`, `fields` (extra key-value data).

## events.jsonl

Each line is one JSON object:

```json
{"ts":"2026-03-07T15:30:01.123Z","event":"key_press","key":"Char('j')","result":"FileTree"}
{"ts":"2026-03-07T15:30:01.500Z","event":"render_slow","frame":142,"duration_ms":150}
{"ts":"2026-03-07T15:30:02.500Z","event":"error","kind":"connection_lost","target":"ssh","message":"timeout"}
{"ts":"2026-03-07T15:30:05.000Z","event":"dialog","action":"open","dialog_kind":"confirm"}
```
