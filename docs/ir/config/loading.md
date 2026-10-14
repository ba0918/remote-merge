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
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A3, docs/decision/records/2026-10-06-path-resolution.md#A64
- verification: unit

--config の指定先が存在しないときは "Config file not found: 絶対パス"、通常のファイルでないときは "Config path is not a regular file: 絶対パス" のエラーで止まり、終了コード 2 を返す。

### REQ-config-008: 設定がどこにもなければ探した場所を示す
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A4
- verification: unit

`グローバル設定` も `プロジェクト設定` も存在しないときは "Config file not found." と探した二つのパスを示すエラーで止まり、終了コード 2 を返す。

### REQ-config-009: TOML として読めない設定で止める
- kind: event_driven
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A5, docs/decision/records/2026-10-11-drop-compat-consistency-2.md#A1
- verification: unit

設定ファイルが TOML として読めないときは "Failed to parse config file: 絶対パス: " に続けて読めなかった理由を示すエラーで止まり、終了コード 2 を返す。絶対パスは読めなかった設定ファイルのものである。

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
- source: docs/decision/records/2026-10-05-quality-config.md#A3, docs/decision/records/2026-10-05-quality-config.md#A10, docs/decision/records/2026-10-11-drop-compat.md#A1, docs/decision/records/2026-10-11-drop-compat.md#A2, docs/decision/records/2026-10-11-drop-compat.md#A3, docs/decision/records/2026-10-11-drop-compat-consistency.md#A1
- verification: unit
- definition: TBL-config-004

`グローバル設定` か `プロジェクト設定` に製品の知らないキーかセクションがあるときはエラーで止まり、終了コード 2 を返す。製品の知っているセクションとキーは TBL-config-004 のものだけである。以前の版が読んでいた [filter] の sensitive、badge_scan_max_files、[agent] の timeout_secs・tree_chunk_size・max_file_chunk_bytes も知らないキーとして扱う。
このエラーの文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。

### REQ-config-043: 設定のエラーに書かれた中身を出さない
- kind: prohibition
- source: docs/decision/records/2026-10-11-drop-compat.md#A7
- verification: unit

`グローバル設定` か `プロジェクト設定` を解釈できないとき（TOML の書き方の誤り、値の型の違い、知らないキーかセクション）のエラーは、標準エラーにも --format json の error にも、設定ファイルの行の抜き出しと、設定ファイルに書かれた値を含めない。設定ファイルのパス、行と列、キーの名前、解釈できない理由は含めてよい。

### REQ-config-040: 相対の root_dir は設定ファイルの場所を基準にする
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A5, docs/decision/records/2026-10-06-path-resolution.md#A65, docs/decision/records/2026-10-06-path-resolution.md#A66
- verification: unit

[local] の root_dir が相対パスのとき、カレントディレクトリではなく、その root_dir を書いた設定ファイルのあるディレクトリを基準に解決する。設定ファイルのディレクトリは、利用者が書いた設定ファイルのパスを OS に解決させた絶対パス（symlink と ".." を OS が辿った結果）の親とし、OS が解決できないときは "Failed to resolve the directory of config file: 絶対パス" のエラーで止まり、終了コード 2 を返す。"~/" を展開した後の root_dir が絶対パスのときは、設定ファイルのディレクトリを求めない。

### REQ-config-041: 空のローカルの root_dir で止める
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A5, docs/decision/records/2026-10-06-path-resolution.md#A66
- verification: unit

[local] の root_dir が空の文字列のときは、理由 "root_dir must not be empty" を示すエラーで止まり、終了コード 2 を返す。

## Decision tables

### TBL-config-004: 製品の知っているセクションとキー
- source: docs/decision/records/2026-10-11-drop-compat-consistency.md#A1

| セクション | キー |
|---|---|
| (トップレベル) | max_scan_entries |
| [servers.名前] | host、port、user、auth、password、key、root_dir、sudo、file_permissions、dir_permissions |
| [servers.名前.ssh_options] | kex_algorithms、host_key_algorithms、ciphers、mac_algorithms |
| [local] | root_dir |
| [filter] | exclude、include |
| [ssh] | timeout_sec、strict_host_key_checking |
| [backup] | enabled、retention_days |
| [agent] | enabled、deploy_dir |
| [defaults] | file_permissions、dir_permissions |

## Examples

```gherkin
@id=EX-config-015 @about=REQ-config-039 @source=docs/decision/records/2026-10-05-quality-config.md#A3
Scenario: キー名を書き間違える
Given `プロジェクト設定` の [filter] に include の書き間違い "inclde" がある
When status を実行する
Then 比較を始めずにエラーで止まり終了コード 2 を返す

@id=EX-config-021 @about=REQ-config-039 @source=docs/decision/records/2026-10-11-drop-compat.md#A1,docs/decision/records/2026-10-11-drop-compat.md#A3
Scenario: 以前の版が読んでいたキーを残した設定
Given `グローバル設定` の [filter] に sensitive = ["*.pem"] がある
When status を実行する
Then 比較を始めずにエラーで止まり終了コード 2 を返す

@id=EX-config-022 @about=REQ-config-039 @source=docs/decision/records/2026-10-11-drop-compat.md#A2,docs/decision/records/2026-10-11-drop-compat.md#A3
Scenario: 使われなくなった agent のキーを残した設定
Given `プロジェクト設定` の [agent] に timeout_secs = 30 がある
When status を実行する
Then 比較を始めずにエラーで止まり終了コード 2 を返す

@id=EX-config-016 @about=REQ-config-040 @source=docs/decision/records/2026-10-05-quality-config.md#A5
Scenario: 別のディレクトリの設定を --config で指定する
Given "proj/.remote-merge.toml" の [local] の root_dir が "." である
When "proj" の外のディレクトリから --config "proj/.remote-merge.toml" を指定して status を実行する
Then ローカルの root_dir は "proj" のディレクトリになる

@id=EX-config-024 @about=REQ-config-043 @source=docs/decision/records/2026-10-11-drop-compat.md#A7
Scenario: パスワードと同じ行に書き間違いのキーがある
Given `プロジェクト設定` の [servers] に、password = "SENTINEL-PW" と知らないキー bogus を一行に書いたサーバがある
When status を --format json を付けたときと付けないときで実行する
Then 標準エラーにも JSON の error にも、設定を解釈できないエラーが出て、そこに "SENTINEL-PW" は含まれない

@id=EX-config-025 @about=REQ-config-043 @source=docs/decision/records/2026-10-11-drop-compat.md#A7
Scenario: パスワードと同じ行の別のキーの型が違う
Given `プロジェクト設定` の [servers] に、password = "SENTINEL-PW" と user = 1 を一行に書いたサーバがある
When status を実行する
Then 標準エラーに設定を解釈できないエラーが出て、そこに "SENTINEL-PW" は含まれない
```
