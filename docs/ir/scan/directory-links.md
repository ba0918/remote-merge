# ディレクトリ symlink の表示と走査

走査が、root_dir より下のディレクトリを指す symlink の配下をどう一覧に載せるか、リンク先を辿れない symlink をどう扱うか。

## Requirements

### REQ-scan-002: 走査はディレクトリ symlink の配下を辿る
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A7, docs/decision/records/2026-10-05-quality-scan.md#A2, docs/decision/records/2026-10-12-foundation-2.md#C4
- verification: unit

status・diff・merge・sync の走査は、途中のディレクトリを指す symlink の配下も走査件数の上限内で再帰列挙し、循環をデバイス番号と inode 番号で検出し、循環または件数超過が起きた場合は不完全な一覧であることを報告する。

### REQ-scan-018: リンク先を辿れない symlink
- kind: event_driven
- source: docs/decision/records/2026-10-12-ssh-output-parsing-consistency-2.md#A1, docs/decision/records/2026-10-12-foundation-2.md#C1, docs/decision/records/2026-10-12-foundation-5.md#F57, docs/decision/records/2026-10-12-foundation-5.md#F82
- verification: unit

リンク先がないのではなく、ループしているか途中にディレクトリでないものを含むためにリンク先を辿れない symlink があるとき、走査はそれをリンク先を辿れない symlink として一覧に載せ、その下を辿らない。名前が UTF-8 として読めない symlink は listing.md の REQ-scan-012 のとおり読めない名前として報告し、リンク先を辿れないことも同じに扱う（走査を止めない）。
読むだけのコマンド（status・diff）は止まらずに、その symlink を compare/judgement.md の TBL-compare-001 のとおり判定できないとしてトップレベルの errors にそのパスと理由を出し、ほかのパスの結果を出して終了コード 2 を返す。
merge・sync・rollback は、比べる範囲にリンク先を辿れない symlink があれば、書き込みを始める前にエラーで止める。
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

@id=EX-scan-018 @about=REQ-scan-018 @source=docs/decision/records/2026-10-12-foundation-5.md#F57
Scenario: 読むだけのコマンドがループする symlink に当たる
Given root_dir の下に互いを指す二つの symlink "a" と "b" があり、ほかに左右で中身の違う "c.txt" がある
When パスを指定せずに diff --format json を実行する
Then "c.txt" の差分は files に出て、"a" と "b" はそのパスと理由が errors に出て、終了コード 2 を返す

@id=EX-scan-019 @about=REQ-scan-018 @source=docs/decision/records/2026-10-12-foundation-5.md#F57
Scenario: merge がループする symlink に当たる
Given 書き込み先の root_dir の下に互いを指す二つの symlink があり、読み込み元にだけ "new.txt" がある
When "." を指定して root_dir 全体を merge する
Then 何も書かずにエラーで止まり、終了コード 2 を返す
```
