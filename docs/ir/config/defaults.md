# 設定の既定値

設定ファイルで省いたキーに使う値。

## Requirements

### REQ-config-013: 省いたキーには既定値を使う
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A8
- verification: unit
- definition: TBL-config-001

設定ファイルで省いたキーには TBL-config-001 の既定値を使う。

## Decision tables

### TBL-config-001: 省いたキーの既定値
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A8

| セクション | キー | 既定値 |
|---|---|---|
| [servers.名前] | port | 22 |
| [servers.名前] | auth | "key" |
| [servers.名前] | sudo | false |
| [ssh] | timeout_sec | 300 |
| [ssh] | strict_host_key_checking | "ask" |
| [backup] | enabled | true |
| [backup] | retention_days | 7 |
| [agent] | enabled | true |
| [agent] | deploy_dir | "/var/tmp" |
| [agent] | timeout_secs | 30 |
| [agent] | tree_chunk_size | 1000 |
| [agent] | max_file_chunk_bytes | 4194304 |
| [defaults] | file_permissions | 0o664 |
| [defaults] | dir_permissions | 0o775 |
