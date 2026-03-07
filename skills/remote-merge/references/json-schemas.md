# JSON Output Schemas

## status

```json
{
  "left":  { "label": "local", "root": "/home/user/app" },
  "right": { "label": "develop", "root": "dev:/var/www/app" },
  "files": [
    { "path": "src/config.ts", "status": "modified", "sensitive": false, "hunks": null },
    { "path": ".env", "status": "modified", "sensitive": true, "hunks": null }
  ],
  "summary": { "modified": 2, "left_only": 0, "right_only": 1, "equal": 10 }
}
```

File status values: `modified`, `left_only`, `right_only`, `equal`.

With `--summary`, `files` is omitted.

## diff

```json
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
```

Line types: `context`, `added`, `removed`. When `truncated` is true, output was cut by `--max-lines`.

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

## events.jsonl

Each line is one JSON object:

```json
{"ts":"2026-03-07T15:30:01.123Z","event":"key_press","key":"Char('j')","result":"FileTree"}
{"ts":"2026-03-07T15:30:01.500Z","event":"render_slow","frame":142,"duration_ms":150}
{"ts":"2026-03-07T15:30:02.500Z","event":"error","kind":"connection_lost","target":"ssh","message":"timeout"}
{"ts":"2026-03-07T15:30:05.000Z","event":"dialog","action":"open","dialog_kind":"confirm"}
```
