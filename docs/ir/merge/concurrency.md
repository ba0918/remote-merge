# 書き込み直前の更新確認

差分表示後に別の操作が書き込み先を変えたときのマージ結果と、そのときの書き込み・symlink の作成・削除が書き込み先の外や種類の違う対象を変えずに失敗する規則。

## Requirements

### REQ-merge-011: 他者の更新を上書きしない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A35, docs/decision/records/2026-09-25-spec-migration.md#A38
- verification: unit

差分確認からマージまでに書き込み先の中身が変わったとき、サイズと更新時刻が元と同じでも、古い状態を前提としたマージは書き込みを止め、変更を報告する。

### REQ-merge-033: 更新を検知したファイルだけを止める
- kind: event_driven
- source: docs/decision/records/2026-10-04-merge-backup-flag-resolution.md#A9
- verification: unit

merge と sync は、比べたときから書き込み先の中身が変わったファイルと、比べたときになかった書き込み先が現れたファイルを書かずに failed に出し、他のファイルの処理は続ける。

### REQ-merge-038: 最後の要素が symlink のパスへ通常ファイルを書かない
- kind: prohibition
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A1, docs/decision/records/2026-10-05-ssh-strict-exec.md#A3, docs/decision/records/2026-10-05-harden-security.md#A6
- verification: review
- how_to_verify: ローカルは "crates/remote-merge-engine/src/merge/executor.rs"、SSH は "crates/remote-merge-ssh/src/tree_parser.rs"（生成したコマンドを sh で実行する）、エージェントは "crates/remote-merge-agent/src/agent/file_io.rs" の単体テストを読み、最後の要素が symlink の書き込み先への書き込みが失敗しリンクとリンク先が変わらないこと、途中の symlink の先へは書けることを人が確かめる。比べた後に書き込み先が symlink に替わる競合でだけ起きるため、契約テストでは作らない。

ローカル・SSH・エージェントのどの経路でも、通常ファイルの中身の書き込みは、書き込み先のパスの最後の要素が既存の symlink ならそのパスへ書かずに失敗し、リンクもリンク先も変えない。
書き込み先の途中の symlink は REQ-merge-004 のとおり辿る。

### REQ-merge-039: SSH の書き込みは一時ファイルを書き込み先の名前に付け替える
- kind: ubiquitous
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A8
- verification: review
- how_to_verify: "crates/remote-merge-ssh/src/tree_parser.rs" の単体テストを読み、生成した書き込みのコマンドを sh で実行して、書き込みの途中で書き込み先を symlink に替えてもリンク先が変わらないこと、既存の書き込み先のモードが残ること、失敗したときに一時ファイルが残らないことを人が確かめる。

SSH の経路の通常ファイルの書き込みは、書き込み先と同じディレクトリに既存のものを上書きせずに作った一時ファイルに中身を書き、書き込み先の名前に付け替えて置き換え、既存の書き込み先のモードを引き継ぐ。
書き込みの途中で書き込み先が symlink に替わっても、リンク先は変えない。
書き込み先が symlink でない既存のディレクトリなら書かずに失敗し、失敗したときは一時ファイルを残さない。

### REQ-merge-040: symlink の作成先にある項目を置き換えない
- kind: prohibition
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A7, docs/decision/records/2026-10-05-ssh-strict-exec.md#A10
- verification: review
- how_to_verify: ローカルは "crates/remote-merge-engine/src/local_io.rs"、SSH は "crates/remote-merge-ssh/src/tree_parser.rs"（生成したコマンドを sh で実行する）、エージェントは "crates/remote-merge-agent/src/agent/file_io.rs" の単体テストを読み、作成先に通常ファイル・ディレクトリ・symlink があると作成が失敗しそれらが変わらないことを人が確かめる。比べた後に作成先に項目が現れる競合でだけ起きるため、契約テストでは作らない。

ローカル・SSH・エージェントのどの経路でも、symlink の作成は、作成先に通常ファイル・ディレクトリ・symlink のどれかが既にあれば置き換えずに失敗し、何も変えない。
merge・sync・rollback の symlink の更新は、比べたときにあった書き込み先を消してから作るので、REQ-merge-003 のとおりリンク先を更新する。

### REQ-merge-041: 比べた後に消えた削除の対象は失敗にする
- kind: event_driven
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A4, docs/decision/records/2026-10-05-ssh-strict-exec.md#A9
- verification: review
- how_to_verify: "crates/remote-merge-ssh/src/tree_parser.rs" の単体テストを読み、生成した削除のコマンドがないパスで失敗することと、ローカルの削除がないパスで失敗することを人が確かめる。比べた後に書き込み先が消える競合でだけ起きるため、契約テストでは作らない。

--delete の merge と sync は、消そうとした書き込み先が比べた後に消えていたとき、どの経路でもそのファイルを failed に出し、他のファイルの処理は続ける。
リモートの削除を使う symlink の更新でも、消えていた書き込み先は failed に出す。

## Examples

```gherkin
@id=EX-merge-021 @about=REQ-merge-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A35
Scenario: 差分確認後に書き込み先が変わる
Given 差分を確認した後で別の操作が書き込み先を更新した
When 元の差分を使ってマージする
Then 後から行われた更新は上書きされず変更が報告される

@id=EX-merge-022 @about=REQ-merge-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A35
Scenario: 書き込み先は変わらない
Given 差分確認後も書き込み先は同じである
When 元の差分を使ってマージする
Then 書き込みが行われる

@id=EX-merge-023 @about=REQ-merge-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A38
Scenario: 更新時刻を戻した外部変更
Given 差分確認後に書き込み先の中身が変わりサイズと更新時刻は以前と同じである
When 元の差分を使ってマージする
Then 書き込み先は上書きされず内容の変更が報告される

@id=EX-merge-045 @about=REQ-merge-038 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A1
Scenario: 比べた後に書き込み先が symlink に替わる
Given 比べたときに通常ファイルだった書き込み先が、書き込みの前に別のファイルを指す symlink に替わった
When そのパスをマージする
Then そのファイルは失敗し、リンクとリンク先のファイルは変わらない

@id=EX-merge-046 @about=REQ-merge-039 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A8
Scenario: SSH の書き込みの途中で書き込み先が symlink に替わる
Given SSH の経路で書き込み先への中身の送信の途中に、書き込み先が別のファイルを指す symlink に替わった
When 送信が終わり書き込みが完了する
Then リンク先のファイルは変わらない

@id=EX-merge-047 @about=REQ-merge-040 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A7
Scenario: 比べた後に symlink の作成先にファイルが現れる
Given 比べたときになかった symlink の作成先に、作成の前に通常ファイルが現れた
When そのパスをマージする
Then そのファイルは失敗し、現れた通常ファイルは変わらない

@id=EX-merge-048 @about=REQ-merge-041 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A4,docs/decision/records/2026-10-05-ssh-strict-exec.md#A9
Scenario: 比べた後に削除の対象が消える
Given --delete で消す書き込み先だけのファイルが、削除の前に別の操作で消された
When --delete でマージする
Then そのファイルは failed に出て、他のファイルの処理は続く
```
