# ツリー走査の上限

多数のファイルを持つ書き込み先で、一覧が途中までしか取得できない場合の扱いと、パスを指定したときの走査の範囲。

## Requirements

### REQ-scan-003: 件数上限を変更できる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A16
- verification: unit

ツリー走査にファイル件数の上限を設け、利用者はその上限を変更できる。

### REQ-scan-004: 不完全な走査を知らせる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A16
- verification: unit

ツリー走査が件数上限を超えた場合は、取得した一部だけを完全な一覧として返さず、上限超過を報告する。

### REQ-scan-005: 不完全な一覧から書き込まない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A45
- verification: unit

元または先のディレクトリ一覧が件数上限やリンクの循環で不完全なとき、そのディレクトリの merge・sync は削除指定の有無によらず書き込みを開始せず、理由を報告する。

### REQ-scan-008: 上限の超過を案内付きで知らせる
- kind: event_driven
- source: docs/decision/records/2026-09-29-adopt-scan-limits.md#A1
- verification: unit

status・diff・merge・sync の走査が件数の上限を超えたとき、"Tree scan truncated at 上限 entries." で始まり、--max-entries で上限を上げること、設定の max_scan_entries を設定すること、全体を走査せずにファイルのパスを指定することを案内するエラーを出す。

### REQ-scan-009: 指定したパスで走査の範囲を選ぶ
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-scan-limits.md#A2, docs/decision/records/2026-09-29-adopt-scan-limits.md#A8
- verification: unit
- definition: TBL-scan-001

status は常に root_dir 全体を走査し、diff はパスを指定しないとき root_dir 全体を走査し、merge・sync は指定したパスによって TBL-scan-001 に従って走査の範囲を選ぶ。

## Decision tables

### TBL-scan-001: merge・sync の走査の範囲
- source: docs/decision/records/2026-09-29-adopt-scan-limits.md#A2, docs/decision/records/2026-09-29-adopt-scan-limits.md#A8

| 指定したパス | 走査の範囲 |
|---|---|
| "."・"./"・空の値のいずれかがある | root_dir 全体 |
| glob 文字（"*"・"?"・"["）を含むものがある | root_dir 全体 |
| 21 個以上 | root_dir 全体 |
| 末尾が "/" のものとそうでないものが混ざる | root_dir 全体 |
| merge・sync に --delete を付けた | root_dir 全体 |
| 全て末尾が "/" | それぞれのディレクトリの下だけで、件数の上限はディレクトリごとの走査に当てる |
| 全て末尾が "/" でない | 各パスの親ディレクトリの下だけ。root_dir の直下のファイルが含まれれば root_dir 全体 |

## Examples

```gherkin
@id=EX-scan-006 @about=REQ-scan-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A16
Scenario: 明示した走査上限で比較する
Given 書き込み先に多数のファイルがある
When 利用者が件数上限を変更して一覧を取得する
Then 変更後の上限が適用される

@id=EX-scan-007 @about=REQ-scan-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A16
Scenario: 上限を指定しない
Given 利用者が件数上限を指定しない
When ツリーを走査する
Then 既定の件数上限が適用される

@id=EX-scan-008 @about=REQ-scan-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A16
Scenario: 上限を超えるディレクトリ
Given 上限を超えるファイルがある
When ツリーを走査する
Then 上限超過が報告され完全な一覧としては返されない

@id=EX-scan-009 @about=REQ-scan-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A16
Scenario: 上限内のディレクトリ
Given ファイル数が上限以下で循環や読み取り失敗もない
When ツリーを走査する
Then 件数上限を理由に一覧が途中で打ち切られることはない

@id=EX-scan-010 @about=REQ-scan-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A45
Scenario: 元の走査が上限で止まる
Given 同期元ディレクトリの走査が件数上限で途中までしか取得できない
When --delete を指定して同期する
Then 対象ディレクトリへの書き込みも削除も始まらず上限超過が報告される

@id=EX-scan-011 @about=REQ-scan-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A45
Scenario: 先のリンクが循環する
Given 同期先ディレクトリの symlink が祖先へ戻り一覧が不完全である
When --delete なしで同期する
Then 対象ディレクトリへの書き込みは始まらず循環が報告される
```
