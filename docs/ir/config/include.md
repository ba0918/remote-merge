# include の値の扱い

include の値の要素の正規化と、値が走査する側にないとき、root_dir の外を指すとき、ファイルや symlink を指すとき、すべて無効なときの走査の起点の決め方。値の書き方の基本は config/filters.md の REQ-config-023 と REQ-config-024 による。

## Requirements

### REQ-config-032: 走査する側にない include の値は飛ばす
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A1, docs/decision/records/2026-10-07-include-values-consistency.md#A2, docs/decision/records/2026-10-07-include-values-consistency.md#A3, docs/decision/records/2026-10-12-foundation-2.md#C1, docs/decision/records/2026-10-12-foundation-2.md#C8
- verification: unit

include の値が走査する側に存在しないとき、走査は "Include path does not exist, skipping: 値" の警告を標準エラーに出してその値を飛ばし、残りの値を走査する。
値が存在するかは、走査する側での値のパスの確認（paths/resolution.md の REQ-paths-001 と REQ-paths-002）の結果で決める。確認がエラーになった値（途中の祖先がディレクトリでない値を含む）は飛ばさず、そのエラーで止まる。
その側の値がすべて飛ばされたときは、その側を走査せずに空の一覧とし、include を指定がないものとして全体に広げない。飛ばした値の配下は、その値がある側にだけある項目として出る。

### REQ-config-033: root_dir の外へ解決される include の値は使わない
- kind: prohibition
- source: docs/decision/records/2026-10-05-quality-config.md#A1, docs/decision/records/2026-10-07-include-values-consistency.md#A6, docs/decision/records/2026-10-12-foundation-2.md#C1, docs/decision/records/2026-10-12-foundation-2.md#C8
- verification: unit

include の値が symlink を辿ると root_dir の外へ解決されるとき、走査は警告を標準エラーに出してその値を使わず、root_dir の外を走査しない。
警告の文言は IR で決めず、実装者が値を含めて英語で書き、テストはその文言を固定しない。

### REQ-config-034: ファイルを指す include の値はそのファイルを対象にする
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A1, docs/decision/records/2026-10-07-include-values-consistency.md#A4, docs/decision/records/2026-10-07-include-values-consistency.md#A9, docs/decision/records/2026-10-12-foundation-2.md#C1
- verification: unit

include の値がファイルを指すとき、走査はそのファイル 1 件を一覧に載せる。
include の値がディレクトリでないものを指す symlink（ファイルへのリンクとリンク切れ）のときは、リンク先のファイルではなく、scan/listing.md の REQ-scan-006 のとおり symlink の項目 1 件を載せる。
ただし、リンク切れの symlink でも、paths/resolution.md の REQ-paths-002 が末尾の symlink に与える実パスが root_dir の外にあるときは、REQ-config-033 の root_dir の外へ解決される値として扱い、載せない。

### REQ-config-035: include の値を要素ごとに正規化する
- kind: ubiquitous
- source: docs/decision/records/2026-10-05-quality-config.md#A1, docs/decision/records/2026-10-05-quality-config.md#A9, docs/decision/records/2026-10-07-include-values-consistency.md#A1, docs/decision/records/2026-10-07-include-values-consistency.md#A12, docs/decision/records/2026-10-12-foundation-2.md#C1
- verification: unit

include の値は、使う前に "." の要素を除き、続いた "/" を一つにまとめ、末尾の "/" を除いてから使う（"a/./b"、"a//b" と "a/b/" は "a/b" として扱う）。
正規化で空になる値（"." だけの値）は root_dir 全体を指し、include の制限がないものとして扱い、REQ-config-037 の無効な値に数えない。
この正規化は config/filters.md の REQ-config-024 で先頭の "./" を取り除いた後に行い、そこで空になる値（"./" だけの値）は REQ-config-024 の空の値として無視し、root_dir 全体を指すものとはしない。
REQ-config-024 の空の値と警告して無視する値の判定は、この正規化の前の、先頭の "./" だけを取り除いた値で行い、この正規化は判定を通った値にだけ行う。"/" や ".//" のように、先頭の "./" を取り除いて "/" で始まる値は、REQ-config-024 の絶対パスの値であって空の値ではない。

### REQ-config-036: ディレクトリ symlink の include の値は設定の名前で載せる
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A1, docs/decision/records/2026-10-07-include-values-consistency.md#A5, docs/decision/records/2026-10-12-foundation-2.md#C1
- verification: unit

include の値が root_dir の中のディレクトリを指す symlink のとき、走査はリンク先の配下を走査し、その配下を include に書いた名前（リンクの名前）の下に載せる。
リンク自体も scan/listing.md の REQ-scan-006 のとおり symlink の項目として載せる。同じディレクトリを指す別の名前の値（リンクの名前とリンク先の名前）を両方書いたときは、どちらの名前の下にも配下を載せる。

### REQ-config-037: 全ての値が無効な include で止める
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-config.md#A4, docs/decision/records/2026-09-29-adopt-config-filters.md#A4, docs/decision/records/2026-10-07-include-values-consistency.md#A6
- verification: unit

include に空でない値を一つ以上書き、そのすべてが REQ-config-024 の警告を出して無視する値のとき、include を指定がないものとして全体に広げず、エラーで止まる。
エラーの文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。

## Examples

```gherkin
@id=EX-config-010 @about=REQ-config-032 @source=docs/decision/records/2026-10-05-quality-config.md#A1,docs/decision/records/2026-10-07-include-values-consistency.md#A3,docs/decision/records/2026-10-12-foundation-2.md#C1
Scenario: 片側にない include の値
Given include に "src" と "docs" があり、右の側に "docs" がない
When status を実行する
Then "Include path does not exist, skipping: docs" の警告が出て "src" の配下は比べられ、左の "docs" の配下は左にだけある項目として出る

@id=EX-config-011 @about=REQ-config-034 @source=docs/decision/records/2026-10-05-quality-config.md#A1
Scenario: include にファイルを書く
Given include に "config/app.php" があり、そのファイルは左右で中身が違う
When status を実行する
Then "config/app.php" が "modified" として一覧に出る

@id=EX-config-012 @about=REQ-config-036 @source=docs/decision/records/2026-10-05-quality-config.md#A1,docs/decision/records/2026-10-12-foundation-2.md#C1
Scenario: include に root_dir の中を指すディレクトリ symlink を書く
Given include に "current" があり、"current" は root_dir の中の "releases/v2" を指す symlink である
When status を実行する
Then 両側とも "current" の下に配下のファイルが載り、片側だけのファイルとは判定されない

@id=EX-config-013 @about=REQ-config-033 @source=docs/decision/records/2026-10-05-quality-config.md#A1,docs/decision/records/2026-10-12-foundation-2.md#C1
Scenario: include の symlink が root_dir の外を指す
Given include に root_dir の外のディレクトリを指す symlink の名前がある
When status を実行する
Then 警告が出てその値は使われず root_dir の外のファイルは一覧に載らない

@id=EX-config-014 @about=REQ-config-037 @source=docs/decision/records/2026-10-05-quality-config.md#A4,docs/decision/records/2026-09-29-adopt-config-filters.md#A4
Scenario: include の値がすべて絶対パス
Given include に "/etc" だけがある
When status を実行する
Then 警告の後にエラーで止まり、全体を走査しない

@id=EX-config-017 @about=REQ-config-035 @source=docs/decision/records/2026-10-05-quality-config.md#A9
Scenario: include に "." だけを書く
Given include に "." だけがあり、root_dir の直下と "src" の下に左右で中身の違うファイルがある
When status を実行する
Then エラーで止まらず、root_dir 全体のファイルが比べられる
```
