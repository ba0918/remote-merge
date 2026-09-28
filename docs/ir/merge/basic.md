# 通常ファイルのマージ

安全条件を満たす通常ファイルを、指定した書き込み先だけに反映する規則。

## Requirements

### REQ-merge-018: 指定先だけを更新する
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A53
- verification: unit

左右と書き込み先を明示し、対象の読み取り・必要な確認・バックアップが成功し、競合と外部更新がないとき、通常ファイルの merge は指定した書き込み先だけを読み込み元の内容に更新する。

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
```
