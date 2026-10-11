# ファイルの対象フィルター

ツリー表示と比較・書き込みの対象からパスを選ぶ規則。

## Requirements

### REQ-config-003: 除外は表示と操作の両方に効く
- kind: invariant
- source: docs/decision/records/2026-09-25-spec-migration.md#A28
- verification: unit

除外パターンに一致するファイルは、ツリー・status・merge・sync の走査対象から外す。

### REQ-config-004: include は対象を限定する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A28
- verification: unit

include 指定があるとき、ツリー・status・merge・sync は一致するパスだけを対象にする。

### REQ-config-021: 名前のパターンは各要素に当てる
- kind: ubiquitous
- source: docs/decision/records/2026-09-29-adopt-config-filters.md#A1, docs/decision/records/2026-10-11-scan-route-parity-consistency-3.md#A1, docs/decision/records/2026-10-11-scan-route-parity-consistency-4.md#A1
- verification: unit

"/" を含まない exclude のパターンは、root_dir からの相対パスのいずれかの要素（ファイル名かディレクトリ名）に glob で当たるときそのパスを除外し、ディレクトリ名に当たればその下のパスも全て除外する。ローカル・SSH・エージェントのどの経路の走査も同じに除外し、除外したディレクトリの下にある循環する symlink や読めないディレクトリでは走査を止めず、不完全な一覧とも報告しない（SSH の経路で走査を止める場合は FLAG-config-016）。

### REQ-config-022: パスのパターンはパス全体に当てる
- kind: ubiquitous
- source: docs/decision/records/2026-09-29-adopt-config-filters.md#A2, docs/decision/records/2026-10-05-quality-config.md#A7, docs/decision/records/2026-10-11-scan-route-parity-consistency-3.md#A1, docs/decision/records/2026-10-11-scan-route-parity-consistency-4.md#A1
- verification: unit

"/" を含む exclude のパターンは、root_dir からの相対パス全体に glob で当たるときそのパスを除外し、当たったパスがディレクトリならその下のパスも全て除外する。ローカル・SSH・エージェントのどの経路の走査も同じに除外し、除外したディレクトリの下にある循環する symlink や読めないディレクトリでは走査を止めず、不完全な一覧とも報告しない（SSH の経路で走査を止める場合は FLAG-config-016）。

### REQ-config-038: "../" を含む exclude のパターンは読み込み時に捨てる
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A8
- verification: unit

"../" を含む exclude のパターンは、設定の読み込み時に "Skipping suspicious exclude pattern containing '../': パターン" の警告をパターンごとに一度だけ標準エラーに出して捨て、走査のときには当てず警告も出さない。

### REQ-config-023: include は区切りの単位の前方一致
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-config-filters.md#A3
- verification: unit

include に書いた root_dir からの相対パスそのものと、その下の "/" の区切りの単位で続くパスだけを対象にし、"src" に対する "srcx" のように区切りの途中で続くパスは対象にしない。

### REQ-config-024: include の書き方を整える
- kind: ubiquitous
- source: docs/decision/records/2026-09-29-adopt-config-filters.md#A4, docs/decision/records/2026-09-29-adopt-config-filters.md#A10
- verification: unit

include の値は先頭の "./" を取り除いて使い、空の値を無視し、絶対パスの値は "Absolute path is not allowed in include filter: 値"、".." を含む値は "Path traversal is not allowed in include filter: 値"、"*"・"?"・"[" を含む値は "Glob patterns are not supported in include filter: 値" の警告を出して無視する。

### REQ-config-025: include と exclude の両方を満たすものだけ
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-config-filters.md#A5
- verification: unit

include と exclude を併せて指定したとき、include の対象のうち exclude に当たらないパスだけを対象にする。

## Examples

```gherkin
@id=EX-config-005 @about=REQ-config-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A28
Scenario: 除外したファイルがある
Given 除外パターンに一致するファイルがある
When status と merge を実行する
Then そのファイルは一覧にも書き込み対象にも含まれない

@id=EX-config-006 @about=REQ-config-003 @source=docs/decision/records/2026-09-25-spec-migration.md#A28
Scenario: 除外しないファイルがある
Given 除外パターンに一致しない変更済みファイルがあり include は指定されていない
When status を実行する
Then そのファイルは対象になる

@id=EX-config-007 @about=REQ-config-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A28
Scenario: include で対象を絞る
Given include に一致するファイルと一致しないファイルがある
When status と sync を実行する
Then 一致するファイルだけが一覧と同期の対象になる

@id=EX-config-008 @about=REQ-config-004 @source=docs/decision/records/2026-09-25-spec-migration.md#A28
Scenario: include を指定しない
Given include に指定がない
When ファイル一覧を取得する
Then include による絞り込みは行われない

@id=EX-config-009 @about=REQ-config-022 @source=docs/decision/records/2026-10-05-quality-config.md#A7
Scenario: パスのパターンがディレクトリに当たる
Given exclude に "a/b" があり、ディレクトリ "a/b" の下に "a/b/c.txt" がある
When SSH の側を含む status を実行する
Then "a/b/c.txt" はどの経路でも一覧に含まれない

@id=EX-config-023 @about=REQ-config-003 @source=docs/decision/records/2026-10-11-drop-compat.md#A5
Scenario: 以前の版のバックアップの置き場が書き込み先に残っている
Given 書き込み先の root_dir に ".remote-merge-backup/old.txt" があり、exclude に ".remote-merge-backup" がない
When status を実行する
Then ".remote-merge-backup/old.txt" はほかのファイルと同じく一覧に出る
```
