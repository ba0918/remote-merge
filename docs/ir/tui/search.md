# ファイル検索とサーバ切替

ツリー内の目的のファイルを探し、比較先を変えても作業位置を保つ操作。

## Requirements

### REQ-tui-003: ファイル名から見つける
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A41
- verification: unit

TUI でファイル名を検索すると該当するファイルへ移動できる。

### REQ-tui-004: サーバ切替後に作業位置を保つ
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A41
- verification: unit

TUI の比較先サーバを切り替えたとき、同じパスが引き続き存在するなら選択とツリーの展開を保つ。

## Examples

```gherkin
@id=EX-tui-005 @about=REQ-tui-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A41
Scenario: ファイル名を検索する
Given ツリーに複数のファイルがある
When 一つのファイル名を検索する
Then 該当するファイルへ移動できる

@id=EX-tui-006 @about=REQ-tui-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A41
Scenario: 一致する名前がない
Given ツリーに検索語と一致するファイルがない
When その名前を検索する
Then 一致がないことが分かる

@id=EX-tui-007 @about=REQ-tui-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A41
Scenario: 両サーバに同じパスがある
Given ツリーでファイルを選びその親ディレクトリを展開し、切替先にも同じパスがある
When 比較先のサーバを切り替える
Then 選択と親ディレクトリの展開は保たれる

@id=EX-tui-008 @about=REQ-tui-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A41
Scenario: 切替先にそのパスがない
Given 選択中のパスが切替先サーバにはない
When 比較先のサーバを切り替える
Then 存在しないパスを誤って選択中として示さない
```
