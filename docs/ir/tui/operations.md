# TUI の主要な操作

ファイルを選び、差分を確認し、書き込み前の確認からマージする対話操作。

## Requirements

### REQ-tui-001: ツリーを辿ってファイルを選べる
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A22
- verification: unit

TUI でディレクトリを展開すると配下のファイルが選択可能になり、ファイルを選ぶと左右の差分を確認できる。

### REQ-tui-002: マージ前に確認できる
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A22
- verification: unit

TUI でマージを開始したときは書き込み前に確認を求め、キャンセルした場合は対象を変更しない。

## Examples

```gherkin
@id=EX-tui-001 @about=REQ-tui-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A22
Scenario: ディレクトリからファイルを選ぶ
Given TUI のツリーに子ファイルがあるディレクトリが表示されている
When 利用者がディレクトリを展開してファイルを選ぶ
Then そのファイルの左右の差分が見られる

@id=EX-tui-002 @about=REQ-tui-001 @source=docs/decision/records/2026-09-25-spec-migration.md#A22
Scenario: 別のファイルに選択を移す
Given ツリーに差分のあるファイルが二つある
When 利用者が二つ目のファイルを選ぶ
Then 二つ目のファイルの差分が見られる

@id=EX-tui-003 @about=REQ-tui-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A22
Scenario: マージをキャンセルする
Given 書き込み元と先に異なる内容のファイルがある
When TUI でマージの確認を拒否する
Then 書き込み先のファイルは変わらない

@id=EX-tui-004 @about=REQ-tui-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A22
Scenario: マージを確認する
Given 書き込み元と先に異なる内容のファイルがある
When TUI でマージの確認を受け入れる
Then 書き込み先のファイルが更新される
```
