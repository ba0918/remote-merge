# 設定ファイルの場所と読み込み

どの設定ファイルを読むか、--config による指定、読み込めないときの止まり方、ローカルの root_dir の展開。

## Requirements

### REQ-config-005: プロジェクト設定はカレントディレクトリから読む
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A1
- verification: unit

--config がないとき、カレントディレクトリの ".remote-merge.toml" だけを `プロジェクト設定` として読み、上の階層のディレクトリは探さない。

### REQ-config-006: --config でプロジェクト設定を指定する
- kind: event_driven
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A2
- verification: unit

--config でファイルを指定したとき、そのファイルを `プロジェクト設定` として読み、カレントディレクトリの ".remote-merge.toml" は読まず、`グローバル設定` は合成し、相対パスはカレントディレクトリから解決する。

### REQ-config-007: --config の指定先がなければ止める
- kind: event_driven
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A3
- verification: unit

--config の指定先が存在しないときは "Config file not found: 絶対パス"、通常のファイルでないときは "Config path is not a regular file: 絶対パス" のエラーで止まり、終了コード 2 を返す。

### REQ-config-008: 設定がどこにもなければ探した場所を示す
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A4
- verification: unit

`グローバル設定` も `プロジェクト設定` も存在しないときは "Config file not found." と探した二つのパスを示すエラーで止まり、終了コード 2 を返す。

### REQ-config-009: TOML として読めない設定で止める
- kind: event_driven
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A5
- verification: unit

設定ファイルが TOML として読めないときは "Failed to parse config file: " に続けて読めなかった理由を示すエラーで止まり、終了コード 2 を返す。

### REQ-config-010: ローカルの root_dir のホームを展開する
- kind: event_driven
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A9
- verification: unit

[local] の root_dir が "~/" で始まるとき、その部分を利用者のホームディレクトリに置き換えて使う。

### REQ-config-028: 設定を使わないサブコマンドは --config を無視する
- kind: event_driven
- source: docs/decision/records/2026-10-04-config-flag-resolution.md#A4
- verification: unit

init・logs・events に --config を指定したときは "Warning: --config is ignored for the 'サブコマンド名' subcommand" を標準エラーに出し、設定を読まずに続ける。
