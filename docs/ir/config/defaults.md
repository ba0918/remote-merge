# 設定の既定値

設定ファイルで省いたキーに使う値と、使われないキーの扱い。

## Requirements

### REQ-config-013: 省いたキーには既定値を使う
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A8, docs/decision/records/2026-10-04-config-flag-resolution.md#A3
- verification: unit
- definition: TBL-config-001

設定ファイルで省いたキーには TBL-config-001 の既定値を使う。

### REQ-config-042: 使われない agent のキーは警告して無視する
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A6, docs/decision/records/2026-10-04-tui-disposition.md#A13, docs/decision/records/2026-10-05-quality-config.md#A10
- verification: unit

`グローバル設定` か `プロジェクト設定` の [agent] に timeout_secs・tree_chunk_size・max_file_chunk_bytes の指定があるときは、そのキーが使われず無視されることを示す警告を標準エラーに出し、その指定を使わずに続ける。
この警告の文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。

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
