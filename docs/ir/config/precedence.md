# 設定の優先順位とフィルター

グローバル設定とプロジェクト設定の両方がある場合に利用者へ適用される値。

## Requirements

### REQ-config-001: プロジェクト固有の値を優先する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A14
- verification: unit

同名サーバ・ローカル root・SSH 設定がグローバルとプロジェクトの両方にあるとき、プロジェクト側の値を採用する。

### REQ-config-002: 両階層のフィルターを適用する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A14, docs/decision/records/2026-10-04-drop-sensitive-file-handling.md#A2
- verification: unit

除外と対象のフィルターをグローバル設定とプロジェクト設定の両方に指定したとき、両方の指定を結合して適用する。

### REQ-config-011: ssh・backup・agent はセクションごと置き換える
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A6
- verification: unit

[ssh]・[backup]・[agent] が `プロジェクト設定` にあるときはセクション全体をプロジェクト側に置き換えて省いたキーには既定値を使い、`プロジェクト設定` にないときは `グローバル設定` のセクションを使う。

### REQ-config-012: defaults はキーごとに合成する
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A7
- verification: unit

[defaults] の file_permissions と dir_permissions は、`プロジェクト設定` にあるキーはプロジェクト側、`プロジェクト設定` になく `グローバル設定` にあるキーはグローバル側、どちらにもないキーは既定値を使う。

### REQ-config-027: 走査の上限はキーごとに選ぶ
- kind: state_driven
- source: docs/decision/records/2026-10-04-config-flag-resolution.md#A3
- verification: unit

max_scan_entries と badge_scan_max_files は、`プロジェクト設定` にあればプロジェクト側、`プロジェクト設定` になく `グローバル設定` にあればグローバル側、どちらにもなければ既定値を使う。

## Examples

```gherkin
@id=EX-config-001 @about=REQ-config-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A14
Scenario: 同名サーバを両方に設定する
Given グローバルとプロジェクトに同名サーバの異なる接続先がある
When 設定を読み込む
Then プロジェクト側の接続先が使われる

@id=EX-config-002 @about=REQ-config-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A14
Scenario: プロジェクトにそのサーバがない
Given グローバルだけに設定されたサーバがある
When 設定を読み込む
Then グローバル側の接続先が使われる

@id=EX-config-003 @about=REQ-config-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A14
Scenario: 異なる除外パターンがある
Given 両方の設定に異なる除外パターンがある
When ファイル一覧を取得する
Then どちらのパターンに一致するファイルも除外される
```
