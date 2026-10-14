# 設定の既定値

設定ファイルで省いたキーに使う値。

## Requirements

### REQ-config-013: 省いたキーには既定値を使う
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A8, docs/decision/records/2026-10-04-config-flag-resolution.md#A3, docs/decision/records/2026-10-15-flag-resolution.md#A7
- verification: unit
- definition: TBL-config-001

設定ファイルで省いたキーには TBL-config-001 の既定値を使う。
ただし [local] の root_dir には既定値がなく、`グローバル設定` にも `プロジェクト設定` にも [local] の root_dir がない設定はエラーで止まり、終了コード 2 を返す。

## Decision tables

### TBL-config-001: 省いたキーの既定値
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A8, docs/decision/records/2026-10-04-config-flag-resolution.md#A3, docs/decision/records/2026-10-04-tui-disposition.md#A5, docs/decision/records/2026-10-05-quality-config.md#A6

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
| [defaults] | file_permissions | 0o664 |
| [defaults] | dir_permissions | 0o775 |
| (トップレベル) | max_scan_entries | 50000 |
