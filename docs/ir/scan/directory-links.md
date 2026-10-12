# ディレクトリ symlink の表示と走査

走査が、root_dir より下のディレクトリを指す symlink の配下をどう一覧に載せるか、リンク先を辿れない symlink でどう止まるか。

## Requirements

### REQ-scan-002: 走査はディレクトリ symlink の配下を辿る
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A7, docs/decision/records/2026-10-05-quality-scan.md#A2, docs/decision/records/2026-10-12-foundation-2.md#C4
- verification: unit

status・diff・merge・sync の走査は、途中のディレクトリを指す symlink の配下も走査件数の上限内で再帰列挙し、循環をデバイス番号と inode 番号で検出し、循環または件数超過が起きた場合は不完全な一覧であることを報告する。

### REQ-scan-018: リンク先を辿れない symlink で走査を止める
- kind: event_driven
- source: docs/decision/records/2026-10-12-ssh-output-parsing-consistency-2.md#A1, docs/decision/records/2026-10-12-foundation-2.md#C1
- verification: unit

リンク先がないのではなく、ループしているか途中にディレクトリでないものを含むためにリンク先を辿れない symlink があるとき、走査はそれがディレクトリへの symlink かを決められないので、それを載せずにエラーで止まる。
リンク先がない symlink は REQ-scan-006 のとおり載せる。

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
