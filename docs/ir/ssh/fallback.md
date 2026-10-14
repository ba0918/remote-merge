# エージェントを使えないサーバ

リモートのファイルの操作はエージェントだけで行うことと、エージェントを利用できないサーバでは読み書きの前に止めること。

## Requirements

### REQ-ssh-005: エージェントを使えなければ読み書きの前に止める
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation.md#A12, docs/decision/records/2026-10-11-agent-operations.md#A32, docs/decision/records/2026-10-12-foundation-3.md#D3, docs/decision/records/2026-10-12-foundation-5.md#F23
- verification: unit

エージェントを利用できない（[agent] enabled が false、配置先が REQ-ssh-014 か REQ-ssh-017 に当たる、配置できないか起動できない、REQ-ssh-021 の通信の版が違う。TBL-ssh-001）サーバでは、sudo=true を指定したかどうかによらず、ファイルを読み書きする前に、エージェントを使えない理由（安全でない配置先ならそのパス）を示す接続のエラーで止め、終了コード 2 を返す。
エージェントの配布は Linux の x86_64（本番の CentOS 5 のカーネル 2.6.18 を含む）と macOS のサーバに配置でき、配布にないサーバ（ARM の Linux を含む）は配置できないサーバとして止める。

### REQ-ssh-010: リモートのすべての操作をエージェントで行う
- kind: state_driven
- source: docs/decision/records/2026-10-11-agent-operations.md#A1, docs/decision/records/2026-10-05-quality-ssh.md#A1, docs/decision/records/2026-10-12-foundation-3.md#D4, docs/decision/records/2026-10-12-foundation.md#A12
- verification: unit

リモートのサーバへのすべての操作（走査・パスの確認・読み込み・書き込み・symlink の作成・削除・権限の変更・子の一覧・ハッシュ）は、sudo=true を指定したかどうかによらずエージェントで行う。
操作が失敗したとき（`接続断` を含む）は、`接続断` は reconnect.md の REQ-ssh-003 と REQ-ssh-022 により、それ以外の失敗はその操作の失敗として扱う。

### REQ-ssh-024: エージェントを使えるかはサーバにつなぐときに一度だけ決める
- kind: algorithm
- source: docs/decision/records/2026-10-12-foundation-3.md#D3, docs/decision/records/2026-10-12-foundation-3.md#D4, docs/decision/records/2026-10-12-foundation.md#A12
- verification: unit
- definition: TBL-ssh-001

### REQ-ssh-021: 通信の版が手元と違うエージェントは使わない
- kind: state_driven
- source: docs/decision/records/2026-10-11-agent-operations.md#A2, docs/decision/records/2026-10-11-agent-operations.md#A27, docs/decision/records/2026-10-11-agent-operations.md#A34, docs/decision/records/2026-10-12-foundation.md#A12
- verification: unit

接続したエージェントが答えた通信の版が手元の通信の版と同じでないとき、そのエージェントは使わず、REQ-ssh-005 のとおり止める。

## Decision tables

### TBL-ssh-001: エージェントを使うか止めるか
- source: docs/decision/records/2026-10-12-foundation-3.md#D3, docs/decision/records/2026-10-05-harden-security.md#A14, docs/decision/records/2026-10-12-foundation.md#A12

| エージェントの状態 | 扱い |
|---|---|
| [agent] enabled が true で、配置先が安全で、起動でき、通信の版が手元と同じ | エージェントで操作する |
| [agent] enabled が false、配置先が REQ-ssh-014 か REQ-ssh-017 に当たる、配置できないか起動できない、通信の版が手元と違う | ファイルを読み書きする前に、理由を示す接続のエラーで止め、終了コード 2（REQ-ssh-005） |

## Examples

```gherkin
@id=EX-ssh-010 @about=REQ-ssh-005 @source=docs/decision/records/2026-09-25-spec-migration.md#A31
Scenario: エージェントを利用できる
Given SSH とエージェントの両方が利用可能である
When 差分を確認してファイルをマージする
Then 差分が確認でき書き込み先が更新される

@id=EX-ssh-036 @about=REQ-ssh-021 @source=docs/decision/records/2026-10-11-agent-operations.md#A2,docs/decision/records/2026-10-12-foundation.md#A12
Scenario: 通信の版が違うエージェント
Given 接続のときに手元と違う通信の版を答えるエージェントが起動する
When merge を実行する
Then ファイルを読み書きする前に、通信の版が違うことを示す接続のエラーで止まり、終了コード 2 を返す

@id=EX-ssh-049 @about=REQ-ssh-005 @source=docs/decision/records/2026-10-11-agent-operations.md#A29,docs/decision/records/2026-10-12-foundation.md#A12
Scenario: エージェントを起動できない
Given sudo=true を指定していないサーバで、リモート側のエージェントが起動できない
When merge を実行する
Then ファイルを読み書きする前に、起動できないことを示す接続のエラーで止まり、終了コード 2 を返す

@id=EX-ssh-055 @about=REQ-ssh-010 @source=docs/decision/records/2026-10-12-foundation-3.md#D4
Scenario: エージェントの読み込みが権限がないで失敗する
Given エージェントを利用でき、書き込み先の "a.txt" と "b.txt" のうち "a.txt" は読む権限がない
When "a.txt" と "b.txt" を merge する
Then "a.txt" は読めなかった失敗として failed に出て、"b.txt" はエージェントで書き込まれ、終了コード 2 を返す

@id=EX-ssh-056 @about=REQ-ssh-024 @source=docs/decision/records/2026-10-12-foundation-3.md#D3,docs/decision/records/2026-10-12-foundation.md#A12
Scenario: エージェントを無効にしている
Given サーバの [agent] enabled が false である
When そのサーバとの差分を確認する
Then ファイルを読む前に接続のエラーで止まり、終了コード 2 を返す
```
