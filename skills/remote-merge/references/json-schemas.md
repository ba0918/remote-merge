# JSON Output Schemas

## Error Response

When `--format json` is specified and an error occurs, all subcommands (status, diff, merge, sync, rollback, logs) return a JSON error object on stdout:

```json
{"error": "error message here"}
```

Exit code is `2` on error. Config-not-found errors display both searched paths (project `.remote-merge.toml` and global `~/.config/remote-merge/config.toml`).

## status

```json
{
  "left":  { "label": "local", "root": "/home/user/app" },
  "right": { "label": "develop", "root": "dev:/var/www/app" },
  "ref":   { "label": "staging", "root": "stg:/var/www/app" },
  "files": [
    { "path": "src/config.ts", "status": "modified" },
    { "path": ".env", "status": "modified", "ref_badge": "differs" }
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
- `agent` — Agent connection status: `"connected"` when the right side is a remote server reached through the agent. Only included with `--verbose` (`-v`).
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
- `summary.files_with_changes`: number of files that differ, exist on one side only, or differ in kind, including binary files and symlinks (the same files that make the exit code 1). The text output's "file(s) with changes" counts the same files
- `truncated`: true when `--max-files` limit was reached (default: 100; use `--max-files 0` for unlimited). Omitted when false.
- `changed_files_total`: total number of changed files before truncation. Present only when `truncated` is true.
- Line types within hunks: `context`, `added`, `removed`. Per-file `truncated` is true when `--max-lines` was hit.

### Binary file

```json
{
  "path": "assets.bin",
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
  "symlink": true,
  "truncated": false,
  "hunks": [],
  "link_targets": {
    "left": "../README.md",
    "right": "../docs/README.md"
  }
}
```

- `link_targets.left` / `link_targets.right`: the link string of the symlink on each side; `null` on a side where the path is a regular file or does not exist.
- `hunks` holds only the diff of the resolved target content. A difference in the link strings appears in `link_targets`, never in `hunks`.

### With `--ref`

- `ref`: the reference server (`label` and `root`). Omitted when the reference side was not compared for that path (for example, the path resolves outside `root_dir` without `--follow-external-links`).
- `ref_hunks`: the diff from the left side to the reference, for text files. An empty array when the left side equals the reference. Omitted when the reference file does not exist, cannot be read (the path and reason are then in `errors`), or is of a different kind.
- `ref_hash`: the SHA-256 of the reference file, for binary files (instead of `ref_hunks`).
- `link_targets.ref`: the link string of the reference, for symlinks (`null` when the reference is not a symlink).
- `conflict_count` / `conflict_regions`: conflicts where both sides changed the same lines differently. A whole-file conflict (binary files, a file missing in the reference that differs on both sides, or a file removed on one side and changed on the other) is one region whose `ref_range` is `{"whole_file": true}` with no line fields. A line conflict's element holds exactly `ref_range` (`{"start": N, "end": M}`, 0-based, end exclusive), `left_lines` and `right_lines`.
- `--max-lines` also applies to `ref_hunks`; the file's `truncated` is true when either diff was cut.

## merge

```json
{
  "merged": [
    { "path": "src/config.ts", "status": "ok", "backup": "20260311-140000/src/config.ts" }
  ],
  "skipped": [
    { "path": "current", "reason": "source and destination have different file types" }
  ],
  "deleted": [
    { "path": "old-file.ts", "status": "ok", "backup": "20260311-140000/old-file.ts" }
  ],
  "failed": [
    { "path": "broken.ts", "error": "destination changed since it was compared" }
  ]
}
```

- `merged[].backup`: backup reference in format `{session_id}/{relative_path}`. It does not expose the absolute path of the local backup store. Omitted when the destination file was newly created or backups are disabled.
- `merged[].status`: `"ok"` on success, `"would merge"` in dry-run mode
- `merged[].ref_badge`: optional reference badge (present only with `--ref`)
- `skipped`: files skipped with the reason (for example, the source and destination have different file types)
- `deleted`: files deleted by `--delete` flag. Always present (empty array `[]` when no files are deleted). Each entry has `path`, `status` (`"ok"`, or `"would delete"` for a planned deletion in dry-run mode), and optional `backup` path. Files that could not be deleted are in `failed`.
- `failed`: files that failed to merge or delete with error details. A destination that changed after the comparison (content, kind, or link target) is not written or deleted and is reported here.
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
        { "path": "current", "reason": "destination is a symlink; deletion skipped" }
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
      "deleted": [],
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
- `targets[].deleted`: files deleted by `--delete`. Always present (empty array `[]` when no files are deleted).
- `targets[].deleted[].status`: `"ok"` or `"failed"`
- `targets[].deleted[].backup`: backup path (omitted when backup is disabled or not applicable)
- `targets[].merged[].backup`: backup reference in format `{session_id}/{relative_path}` (omitted when the destination file was newly created or backups are disabled)
- `summary`: aggregate counts across all servers
- Connection failures appear as targets with empty `merged` and error details in `failed`

## rollback --list

```json
{
  "target": { "label": "develop", "root": "dev:/var/www/app" },
  "sessions": [
    {
      "session_id": "20260311-140000",
      "file_count": 2,
      "files": [
        { "path": "src/config.ts", "size": 1234 },
        { "path": "current", "link_target": "releases/A" }
      ]
    }
  ]
}
```

- `target`: SourceInfo object with `label` and `root`
- `sessions`: array of backup sessions, sorted newest first
- `sessions[].file_count`: number of files in the session
- `sessions[].expired`: boolean, present when true (the UTC timestamp in the session ID is at least `retention_days * 24 hours` old). Omitted when false.
- `sessions[].files`: backed-up entries. Regular-file entries contain `path` and `size`. An entry for a replaced symlink contains `path` and `link_target`, and omits `size`.
- Sessions whose records are missing are omitted; a record that cannot be parsed stops the command with the path and the reason (exit code 2). Operations that had nothing to back up do not leave empty sessions.

## rollback (restore)

```json
{
  "target": { "label": "develop", "root": "dev:/var/www/app" },
  "session_id": "20260311-140000",
  "restored": [
    { "path": "src/config.ts", "pre_rollback_backup": "20260311-150000" }
  ],
  "skipped": [
    { "path": "current", "reason": "symlink restore not supported" }
  ],
  "failed": [
    { "path": "broken.ts", "error": "cannot resolve path: permission denied" }
  ]
}
```

- `target`: SourceInfo object with `label` and `root`
- `restored[].pre_rollback_backup`: session ID of the safety backup created before restoring, so the restore itself can be undone. Omitted when the destination did not exist before restoration.
- `skipped`: files skipped. Omitted when empty. Symlink-safe rollback may report `"symlink restore not supported"`, `"path now resolves to a different location"`, or `"parent directory no longer exists"`; these skips make the command exit with code 2 even with `--force`.
- `failed`: files that failed to restore. Omitted when empty. A path that cannot be resolved is reported as `"cannot resolve path: <cause>"`, and then every file of the session is reported with that cause. A destination that is no longer a regular file (a directory, FIFO, socket, or device) is reported as `"not a regular file: target"`. A file whose current content cannot be saved first is reported as `"backup failed: <cause>"`.

## debug.log (JSONL)

Each line is one JSON object (structured tracing log):

```json
{"timestamp":"2026-03-07T21:00:00.123Z","level":"INFO","target":"ssh::client","message":"connected to develop","fields":{}}
{"timestamp":"2026-03-07T21:00:01.456Z","level":"ERROR","target":"ssh::client","message":"connection timeout","fields":{"elapsed_ms":30000}}
```

Fields: `timestamp` (ISO 8601), `level` (TRACE/DEBUG/INFO/WARN/ERROR), `target` (module path), `message`, `fields` (extra key-value data).
