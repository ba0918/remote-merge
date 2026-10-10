# エージェントと SSH の経路の選び方

リモート側のエージェントを使うか SSH の経路で行うかの選び方と、エージェントを利用できない場合の比較・マージの結果。

## Requirements

### REQ-ssh-005: エージェントの不在で操作を失わない
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A31, docs/decision/records/2026-10-11-agent-operations.md#A29, docs/decision/records/2026-10-11-agent-operations.md#A32
- verification: unit

sudo=true を指定していないサーバでは、リモート側のエージェントを利用できないときも、SSH の経路でファイルの比較とマージを行える。
sudo=true を指定したサーバでエージェントを利用できない（起動できない、REQ-ssh-021 の通信の版が違う）ときは、ファイルを読み書きする前に接続のエラーで止め、終了コード 2 を返し、SSH の経路では行わない。

### REQ-ssh-010: 削除・権限の変更・子の一覧もエージェントで行う
- kind: state_driven
- source: docs/decision/records/2026-10-11-agent-operations.md#A1, docs/decision/records/2026-10-05-quality-ssh.md#A1
- verification: unit

リモート側のエージェントを利用できるとき、そのサーバへの削除・権限の変更・子の一覧は、sudo=true を指定したかどうかによらず、読み込みや書き込みと同じくエージェントで行い、SSH の経路では行わない。

### REQ-ssh-021: 通信の版が手元と違うエージェントは使わない
- kind: state_driven
- source: docs/decision/records/2026-10-11-agent-operations.md#A2, docs/decision/records/2026-10-11-agent-operations.md#A27, docs/decision/records/2026-10-11-agent-operations.md#A34
- verification: unit

接続したエージェントが答えた通信の版が手元の通信の版と同じでないとき、そのエージェントは使わない。sudo=true を指定したサーバでは REQ-ssh-005 のとおり接続のエラーで止める。sudo=true を指定していないサーバでは、エージェントを使わないことと通信の版が違うことを英語の警告で標準エラーに出し、REQ-ssh-005 のとおり SSH の経路で行う。警告の文言は IR で決めず、テストは警告が出てエージェントを使わない理由が読めることだけを確かめ、文言を固定しない。

## Examples

```gherkin
@id=EX-ssh-009 @about=REQ-ssh-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A31
Scenario: リモートでエージェントを使えない
Given sudo=true を指定していないサーバで、SSH 接続は可能だがリモート側のエージェントは利用できない
When 差分を確認してファイルをマージする
Then SSH 経路で差分が確認でき書き込み先が更新される

@id=EX-ssh-010 @about=REQ-ssh-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A31
Scenario: エージェントを利用できる
Given SSH とエージェントの両方が利用可能である
When 差分を確認してファイルをマージする
Then 差分が確認でき書き込み先が更新される

@id=EX-ssh-034 @about=REQ-ssh-010 @source=docs/decision/records/2026-10-11-agent-operations.md#A1
Scenario: sudo のサーバで書き込み先だけのファイルを削除する
Given サーバに sudo=true を指定し、エージェントを利用でき、書き込み先にだけ root の持つ "old.txt" がある
When --delete で同期する
Then "old.txt" はエージェントで削除され、SSH の経路では rm を実行しない

@id=EX-ssh-035 @about=REQ-ssh-010 @source=docs/decision/records/2026-10-11-agent-operations.md#A1
Scenario: sudo を指定しないサーバでも三つの操作はエージェントで行う
Given サーバに sudo=true を指定せず、エージェントを利用できる
When 新しいファイルを merge し、--delete で同期し、ディレクトリを指定して diff する
Then 権限の変更・削除・子の一覧はエージェントで行われ、SSH の経路では chmod・rm・find を実行しない

@id=EX-ssh-036 @about=REQ-ssh-021 @source=docs/decision/records/2026-10-11-agent-operations.md#A2,docs/decision/records/2026-10-11-agent-operations.md#A32
Scenario: sudo のサーバで通信の版が違うエージェント
Given サーバに sudo=true を指定し、接続のときに手元と違う通信の版を答えるエージェントが起動する
When merge を実行する
Then ファイルを読み書きする前に接続のエラーで止まり、終了コード 2 を返し、SSH の経路で読み書きしない

@id=EX-ssh-037 @about=REQ-ssh-021 @source=docs/decision/records/2026-10-11-agent-operations.md#A2,docs/decision/records/2026-10-11-agent-operations.md#A27
Scenario: sudo を指定しないサーバで通信の版が違うエージェント
Given サーバに sudo=true を指定せず、接続のときに手元と違う通信の版を答えるエージェントが起動する
When 差分を確認してファイルをマージする
Then 通信の版が違う警告が標準エラーに出て、そのエージェントは使われず、SSH の経路で差分が確認でき書き込み先が更新される

@id=EX-ssh-049 @about=REQ-ssh-005 @source=docs/decision/records/2026-10-11-agent-operations.md#A29,docs/decision/records/2026-10-11-agent-operations.md#A32
Scenario: sudo のサーバでエージェントを起動できない
Given サーバに sudo=true を指定し、リモート側のエージェントが起動できない
When merge を実行する
Then ファイルを読み書きする前に接続のエラーで止まり、終了コード 2 を返し、SSH の経路で読み書きしない
```
