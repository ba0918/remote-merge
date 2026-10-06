# 経路ごとのパスの確認

ローカル・SSH・エージェントの経路がパスの確認を resolution.md の規則で同じ結果にすること、SSH とエージェントに固有の扱い、確認の結果を使う側の扱い。

## Requirements

### REQ-paths-008: どの経路も同じ結果を返す
- kind: invariant
- source: docs/decision/records/2026-10-06-path-resolution.md#A1, docs/decision/records/2026-10-06-path-resolution.md#A8, docs/decision/records/2026-10-06-path-resolution.md#A14, docs/decision/records/2026-10-06-path-resolution.md#A34, docs/decision/records/2026-10-06-path-resolution.md#A35, docs/decision/records/2026-10-06-path-resolution.md#A44, docs/decision/records/2026-10-06-path-resolution.md#A47, docs/decision/records/2026-10-06-path-resolution.md#A52
- verification: property

同じファイルシステムの状態と同じパスを同じ利用者・同じ権限で確認するとき、ローカル・SSH・エージェントのパスの確認は、同じ種類と同じバイト列の実パスの結果か、同じ "cannot resolve path: " の文言のエラーを返す。ただし SSH の経路は "cannot read" に当たる OS の失敗を存在しないことと見分けない。ローカルの経路が macOS のときは、1024 バイト以上 4096 バイト未満の実パスで OS が先に失敗し "cannot resolve path: cannot read" になる。SSH の経路では、権限以外の理由で入れないディレクトリも "cannot resolve path: permission denied" になる。

### REQ-paths-009: SSH で readlink がなければ止める
- kind: state_driven
- source: docs/decision/records/2026-10-06-path-resolution.md#A20, docs/decision/records/2026-10-06-path-resolution.md#A35, docs/decision/records/2026-10-06-path-resolution.md#A36
- verification: unit

SSH の経路でパスの確認が symlink に出会い、リモートに readlink がないとき、確認は "cannot resolve path: readlink not available" で止める。symlink に出会わない確認は readlink がなくても続ける。

### REQ-paths-010: 古いエージェントでは SSH で確認する
- kind: state_driven
- source: docs/decision/records/2026-10-06-path-resolution.md#A17, docs/decision/records/2026-10-06-path-resolution.md#A45
- verification: unit

エージェントとの通信の版が 5 未満のとき、パスの確認はエージェントを使わず SSH の経路で行い、sudo を使うサーバではエージェントの置き直しが要ることを示すエラーで止める。

### REQ-paths-011: 祖先がディレクトリでなければ rollback 以外はエラーにする
- kind: state_driven
- source: docs/decision/records/2026-10-06-path-resolution.md#A16
- verification: unit

パスの確認の結果が途中の祖先がディレクトリでないとき、diff・merge・sync とバックアップの作成は、そのパスを "cannot resolve path: not a directory" のエラーとして扱う。

### REQ-paths-012: リモートの相対の root_dir はログインした利用者の作業ディレクトリを基準にする
- kind: state_driven
- source: docs/decision/records/2026-10-06-path-resolution.md#A53
- verification: unit

SSH かエージェントの経路で root_dir が相対パスのとき、パスの確認はリモートでログインした利用者の作業ディレクトリの実パスを基準に root_dir を絶対パスにしてから辿る。

## Examples

```gherkin
@id=EX-paths-019 @about=REQ-paths-008 @source=docs/decision/records/2026-10-06-path-resolution.md#A14
Scenario: 三つの経路で同じ結果
Given 同じ一時ディレクトリに、連なった symlink、"."・"//"・".." を含むリンク先、4096 バイト前後のパスを置く
When 同じ利用者がローカル・SSH・エージェントの経路で同じパスを確認する
Then 三つの結果は種類も実パスのバイト列もエラーの文言も一致する

@id=EX-paths-020 @about=REQ-paths-009 @source=docs/decision/records/2026-10-06-path-resolution.md#A36,docs/decision/records/2026-10-06-path-resolution.md#A20,docs/decision/records/2026-10-06-path-resolution.md#A35
Scenario: readlink のないリモート
Given リモートに readlink がなく、root_dir の下に通常ファイル "a.txt" と symlink "l" がある
When SSH の経路で "a.txt" と "l" を確認する
Then "a.txt" は存在するで、"l" の確認は "cannot resolve path: readlink not available" で止まる

@id=EX-paths-021 @about=REQ-paths-010 @source=docs/decision/records/2026-10-06-path-resolution.md#A17
Scenario: 版 4 のエージェント
Given 接続したエージェントとの通信の版が 4 である
When パスを確認する
Then 確認は SSH の経路で行われる

@id=EX-paths-022 @about=REQ-paths-011 @source=docs/decision/records/2026-10-06-path-resolution.md#A16
Scenario: merge の書き込み先の祖先が通常ファイル
Given 書き込み先の "dir" が通常ファイルになっている
When "dir/a.txt" を merge する
Then そのファイルは "cannot resolve path: not a directory" のエラーになる

@id=EX-paths-024 @about=REQ-paths-010 @source=docs/decision/records/2026-10-06-path-resolution.md#A45
Scenario: sudo のサーバの版 4 のエージェント
Given sudo を使うサーバで、接続したエージェントとの通信の版が 4 である
When パスを確認する
Then 確認は SSH の経路で行われず、エージェントの置き直しが要ることを示すエラーになる

@id=EX-paths-025 @about=REQ-paths-012 @source=docs/decision/records/2026-10-06-path-resolution.md#A53
Scenario: リモートの相対の root_dir
Given リモートでログインした利用者の作業ディレクトリが symlink を含まない実パス "/home/u" で、root_dir が "app" で、"/home/u/app/a.txt" が通常ファイルである
When SSH とエージェントの経路で "a.txt" を確認する
Then どちらも結果は存在するで、実パスは "/home/u/app/a.txt" になる
```
