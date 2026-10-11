# 通常ファイルのマージ

安全条件を満たす通常ファイルを、指定した書き込み先だけに反映する規則と、指定したパスが両側にないときの扱い。

## Requirements

### REQ-merge-018: 指定先だけを更新する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A53, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A8
- verification: unit

左右と書き込み先を明示し、対象の読み取り・必要な確認・バックアップが成功し、競合と外部更新がなく、参照先を使うときは書き込み先が参照先から変わっていないとき、通常ファイルの merge は指定した書き込み先だけを読み込み元の内容に更新する。

### REQ-merge-021: ディレクトリ指定は差分のあるファイルを書く
- kind: state_driven
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A3
- verification: unit

ディレクトリを指定した merge は、配下で中身の違うファイルを上書きし、読み込み元にだけあるファイルを書き込み先に作り、中身が同じファイルは書かない。

### REQ-merge-022: 指定したパスをそれぞれ一度だけ書く
- kind: ubiquitous
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A4
- verification: unit

複数のパスを指定した merge は指定したそれぞれのパスを書き込み、同じパスを重ねて指定しても一度だけ書き込む。

### REQ-merge-035: 両側にないパスを failed に入れる
- kind: event_driven
- source: docs/decision/records/2026-10-05-quality-merge.md#A3, docs/decision/records/2026-10-05-quality-merge.md#A5, docs/decision/records/2026-10-12-untested-items-consistency-4.md#A1, docs/decision/records/2026-10-12-untested-items-consistency-5.md#A1
- verification: unit

merge と sync は、指定したパスが左右のどちらにもないとき、--dry-run かどうかによらず、パスごとに警告を出してそのパスを failed に入れ、指定したパスがすべてないときは書き込まずにエラーで止まる。
左右のどちらにもないパスは、その祖先が REQ-merge-001 で種類の違いによりスキップする対象でも、この扱いによる（そのパスのために祖先を skipped に出すことはない）。
書き込み先が複数の sync では、左右を読み込み元とそれぞれの書き込み先とし、どちらにもないパスはその書き込み先の failed に入れ、エラーで止まるのは、すべての書き込み先で指定したパスがすべてないときだけとする（REQ-merge-015）。接続かツリーの取得に失敗した書き込み先があるときは、その書き込み先でのパスの有無が分からないので止まらず、その失敗とほかの書き込み先の failed を報告する。
両側にないパスの警告と failed の error の文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。

## Examples

```gherkin
@id=EX-merge-036 @about=REQ-merge-018 @source=docs/decision/records/2026-09-25-spec-migration.md#A53
Scenario: 通常ファイルを指定先にマージする
Given 読み込み元と先の通常ファイルが異なり読み取り・確認・バックアップが成功し競合や外部更新がない
When 左右と書き込み先を明示してマージする
Then 書き込み先だけが元の内容に更新される

@id=EX-merge-037 @about=REQ-merge-018 @source=docs/decision/records/2026-09-25-spec-migration.md#A53,docs/decision/records/2026-09-25-spec-migration.md#A51
Scenario: 読み込みに失敗する
Given 読み込み元の通常ファイルを読めない
When 左右と書き込み先を明示してマージする
Then 書き込み先は変更されない

@id=EX-merge-042 @about=REQ-merge-035 @source=docs/decision/records/2026-10-05-quality-merge.md#A3
Scenario: dry-run で両側にないパスを指定する
Given "a.txt" は左にあり "missing.txt" は左右のどちらにもない
When "a.txt" と "missing.txt" を --dry-run でマージする
Then "missing.txt" は警告が出て failed に入り "Would merge" とは出ない
```
