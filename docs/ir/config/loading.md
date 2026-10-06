# 設定ファイルの場所と読み込み

どの設定ファイルを読むか、--config による指定、読み込めないときと知らないキーがあるときの止まり方、ローカルの root_dir の展開と基準。

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
- source: docs/decision/records/2026-10-04-config-flag-resolution.md#A4, docs/decision/records/2026-10-04-tui-disposition.md#A4
- verification: unit

init・logs に --config を指定したときは "Warning: --config is ignored for the 'サブコマンド名' subcommand" を標準エラーに出し、設定を読まずに続ける。

### REQ-config-039: 知らないキーとセクションで止める
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A3, docs/decision/records/2026-10-05-quality-config.md#A10
- verification: unit

`グローバル設定` か `プロジェクト設定` に製品の知らないキーかセクションがあるときはエラーで止まり、終了コード 2 を返す。[filter] の sensitive と badge_scan_max_files はこのエラーにせず、REQ-config-030 と REQ-config-031 の警告を出して続ける。
このエラーの文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。

### REQ-config-040: 相対の root_dir は設定ファイルの場所を基準にする
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A5, docs/decision/records/2026-10-06-path-resolution.md#A56, docs/decision/records/2026-10-06-path-resolution.md#A59
- verification: unit

[local] の root_dir が相対パスのとき、カレントディレクトリではなく、その root_dir を書いた設定ファイルのあるディレクトリを基準に解決する。設定ファイルのディレクトリは、設定ファイルの絶対パスから "." の要素を除き、".." の要素は OS と同じく物理的に解決し（その直前までの部分に symlink があれば実パスに直してから一つ上へ移る）、それ以外の symlink は辿らずに字面のまま残したものの親とする。

### REQ-config-041: 空のローカルの root_dir で止める
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A5
- verification: unit

[local] の root_dir が空の文字列のときは、理由 "root_dir must not be empty" を示すエラーで止まる。

## Examples

```gherkin
@id=EX-config-015 @about=REQ-config-039 @source=docs/decision/records/2026-10-05-quality-config.md#A3
Scenario: キー名を書き間違える
Given `プロジェクト設定` の [filter] に include の書き間違い "inclde" がある
When status を実行する
Then 比較を始めずにエラーで止まり終了コード 2 を返す

@id=EX-config-016 @about=REQ-config-040 @source=docs/decision/records/2026-10-05-quality-config.md#A5
Scenario: 別のディレクトリの設定を --config で指定する
Given "proj/.remote-merge.toml" の [local] の root_dir が "." である
When "proj" の外のディレクトリから --config "proj/.remote-merge.toml" を指定して status を実行する
Then ローカルの root_dir は "proj" のディレクトリになる
```
