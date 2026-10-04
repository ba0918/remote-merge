# ディレクトリ symlink の表示と走査

共有ディレクトリを指すリンクの配下を、CLI のファイル一覧にどう表示するか。

## Requirements

### REQ-scan-002: CLI の一覧にはリンク先の配下を含める
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A7
- verification: unit

CLI status はディレクトリを指す symlink の配下も走査件数の上限内で再帰列挙し、循環または件数超過が起きた場合は不完全な一覧であることを報告する。

## Examples

```gherkin
@id=EX-scan-003 @about=REQ-scan-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A7
Scenario: リンク先に複数のファイルがある
Given ディレクトリを指す symlink の先に複数のファイルがある
When CLI status を実行する
Then リンク先の配下のファイルも一覧に含まれる

@id=EX-scan-004 @about=REQ-scan-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A7
Scenario: リンクが祖先へ戻る
Given ディレクトリを指す symlink が走査済みの祖先へ戻る
When CLI status を実行する
Then 走査は停止し不完全な一覧であることが報告される

@id=EX-scan-005 @about=REQ-scan-002 @source=docs/decision/records/2026-09-25-spec-migration.md#A7
Scenario: 上限に達する
Given リンク先を含むファイル数が走査上限を超える
When CLI status を実行する
Then 上限超過が報告され一部の一覧を完全な結果として返さない
```
