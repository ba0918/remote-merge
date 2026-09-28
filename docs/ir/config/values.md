# 設定の値の読み方と検査

設定に書いた値をどう読み、どの値で止まるか。サーバの値、パーミッション、走査の上限、ホスト鍵の確認、パスワードと鍵。

## Requirements

### REQ-config-014: 不正なサーバの値で止める
- kind: event_driven
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A1
- verification: unit
- definition: TBL-config-002

サーバの値が TBL-config-002 のいずれかに当たるとき、"Invalid config value: servers.サーバ名.キー - 理由" のエラーで止まり、終了コード 2 を返す。

### REQ-config-015: パーミッションを 8 進数として読む
- kind: event_driven
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A2
- verification: unit

サーバごとと [defaults] の file_permissions・dir_permissions は "0o" で始まる形か数字だけの形を 8 進数として読み、8 進数の数字以外を含むときは理由 "invalid permissions string: '値' (must contain only octal digits 0-7)"、数字がないときは理由 "invalid permissions string: '値' (empty octal digits)"、0o777 を超えるときは理由 "invalid permissions value: '値' (must be <= 0o777, got 0o8進数)" で、"Invalid config value: キー名 - 理由" のエラーで止まり、終了コード 2 を返す。

### REQ-config-016: 新しく作るものの権限を選ぶ
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A3
- verification: unit

新しく作るファイルとディレクトリの権限には、サーバの file_permissions・dir_permissions があればそれを、なければ [defaults] の値を、それもなければ既定値を使う。

### REQ-config-017: 走査の上限の範囲外で止める
- kind: event_driven
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A4
- verification: unit
- definition: TBL-config-003

走査の上限の値が TBL-config-003 の範囲の外のとき、その行のエラーで止まり、終了コード 2 を返す。

### REQ-config-018: ホスト鍵の確認の値を読む
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A5
- verification: unit

strict_host_key_checking は大文字と小文字を区別せずに "ask" を ask、"yes" と "true" を yes、"no" と "false" を no として読み、それ以外の値は "Unknown strict_host_key_checking value: '値', falling back to 'ask'" の警告を出して ask として扱う。

### REQ-config-019: 環境変数のパスワードを優先する
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A6
- verification: unit

auth が "password" のサーバのパスワードには、サーバ名を大文字にした環境変数 REMOTE_MERGE_PASSWORD_<サーバ名> が空でなく設定されていればそれを設定の password より優先して使い、空の環境変数は設定されていないものとして扱う。

### REQ-config-020: 鍵のパスの既定値と展開
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A7, docs/decision/records/2026-09-28-adopt-config-values.md#A12
- verification: unit

auth が "key" のサーバで key を省くと "~/.ssh/id_rsa" を使い、key が "~/" で始まるときはその部分を利用者のホームディレクトリに置き換える。

鍵ファイルを読めないときは "Failed to load SSH private key: パス" のエラーで止まり、パスには key を省いたときは "~/.ssh/id_rsa" を、key を書いたときは置き換えた後のパスを示す。

## Decision tables

### TBL-config-002: 止めるサーバの値
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A1

| キー | 止める値 | 理由 |
|---|---|---|
| port | 0 | "port must be >= 1" |
| auth | "key" でも "password" でもない値 | "invalid auth value: '値' (expected 'key' or 'password')" |
| root_dir | 空の文字列 | "root_dir must not be empty" |

### TBL-config-003: 走査の上限の範囲
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A4

| 値 | 範囲 | 範囲外のときのエラー |
|---|---|---|
| 設定の max_scan_entries | 1 から 1,000,000 | "Invalid config value: max_scan_entries - max_scan_entries must be between 1 and 1,000,000, got 値" |
| 設定の badge_scan_max_files | 1 から 10,000 | "Invalid config value: badge_scan_max_files - badge_scan_max_files must be between 1 and 10,000, got 値" |
| status・diff・merge・sync の --max-entries | 1 から 1,000,000 | "max_scan_entries must be between 1 and 1,000,000, got 値" |
