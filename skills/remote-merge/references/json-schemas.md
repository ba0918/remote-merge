# JSON Output Schemas

## status

```json
{
  "left":  { "label": "local", "root": "/home/user/app" },
  "right": { "label": "develop", "root": "dev:/var/www/app" },
  "ref":   { "label": "staging", "root": "stg:/var/www/app" },
  "agent": "connected",
  "files": [
    { "path": "src/config.ts", "status": "modified", "sensitive": false, "hunks": null },
    { "path": ".env", "status": "modified", "sensitive": true, "hunks": null }
  ],
  "summary": { "modified": 2, "left_only": 0, "right_only": 1, "equal": 10, "ref_differs": 1, "ref_only": 0, "ref_missing": 0 }
}
```

File status values: `modified`, `left_only`, `right_only`, `equal`.

Optional fields (omitted when null/not applicable):
- `ref` — reference server info (present only with `--ref`)
- `agent` — Agent connection status: `"connected"` or `"fallback"` (present only for remote servers)
- `summary.ref_differs`, `summary.ref_only`, `summary.ref_missing` — reference comparison counts (present only with `--ref`)

With `--summary`, `files` is omitted.

With `--all`, Equal files are included in `files`. By default, Equal files are excluded.

With `--checksum`, all files are compared by content regardless of mtime/size. Files that appear `equal` by metadata may become `modified`.

## diff

Diff always returns a `MultiDiffOutput` wrapper, even for a single file.

```json
{
  "files": [
    {
      "path": "src/config.ts",
      "left":  { "label": "local", "root": "." },
      "right": { "label": "develop", "root": "/var/www" },
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
    "total_files": 1,
    "files_with_changes": 1
  },
  "truncated": false,
  "total_files": 1
}
```

- `files`: array of per-file diff outputs
- `summary.total_files`: number of files included in output
- `summary.files_with_changes`: number of files that have at least one hunk
- `truncated`: true when `--max-files` limit was reached (default: 100; use `--max-files 0` for unlimited)
- `total_files`: total number of matching files before truncation (present only when truncated)
- Line types within hunks: `context`, `added`, `removed`. Per-file `truncated` is true when `--max-lines` was hit.

## merge

```json
{
  "merged": [
    { "path": "src/config.ts", "status": "ok", "backup": "src/config.ts.20260307.bak" }
  ],
  "skipped": [
    { "path": ".env", "reason": "sensitive file" }
  ],
  "failed": [
    { "path": "broken.ts", "error": "optimistic lock failed" }
  ]
}
```

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
