# 経路ごとのパスの確認

パスの確認の結果が経路によらないこと、リモートの相対の root_dir の扱い、確認の結果を使う側の扱い。

## Requirements

### REQ-paths-008: どの経路も同じ結果を返す
- kind: invariant
- source: docs/decision/records/2026-10-06-path-resolution.md#A1, docs/decision/records/2026-10-06-path-resolution.md#A8, docs/decision/records/2026-10-06-path-resolution.md#A14, docs/decision/records/2026-10-06-path-resolution.md#A34, docs/decision/records/2026-10-06-path-resolution.md#A35, docs/decision/records/2026-10-06-path-resolution.md#A44, docs/decision/records/2026-10-12-foundation-3.md#D5, docs/decision/records/2026-10-12-foundation-3.md#D10, docs/decision/records/2026-10-12-foundation.md#A12, docs/decision/records/2026-10-12-foundation-5.md#F23
- verification: property

同じファイルシステムの状態と同じパスを同じ利用者・同じ権限で確認するとき、ローカルとエージェントのどちらの経路でも、パスの確認は同じ種類と同じバイト列の実パスの結果か、同じ "cannot resolve path: " の文言のエラーを返す。
存在しないこと、"cannot resolve path: permission denied"、"cannot resolve path: cannot read" は、どの経路でも同じ状態で同じものになる。
経路で違いうるのは、FLAG-paths-004 の OS のパスの長さの上限による違いだけである。

### REQ-paths-011: 祖先がディレクトリでなければ merge・sync とバックアップの作成はエラーにする
- kind: state_driven
- source: docs/decision/records/2026-10-06-path-resolution.md#A16, docs/decision/records/2026-10-12-skip-subtree-consistency-2.md#A1, docs/decision/records/2026-10-12-skip-subtree-consistency-3.md#A1, docs/decision/records/2026-10-12-foundation-5.md#F56
- verification: unit

パスの確認の結果が途中の祖先がディレクトリでないとき、merge・sync とバックアップの作成は、そのパスを "cannot resolve path: not a directory" のエラーとして扱う。
merge と sync の --dry-run も、書き込む予定のパスについて実行時と同じパスの確認をし、確認がエラーになるパス（途中の祖先がディレクトリでないパスを含む）は書き込む予定に出さず、実行時と同じエラーとして failed に出す。

### REQ-paths-012: リモートの相対の root_dir はログインした利用者の作業ディレクトリを基準にする
- kind: state_driven
- source: docs/decision/records/2026-10-06-path-resolution.md#A53, docs/decision/records/2026-10-12-foundation-3.md#D12, docs/decision/records/2026-10-12-foundation.md#A12
- verification: unit

リモートの root_dir が相対パスのとき、パスの確認はリモートでログインした利用者の作業ディレクトリの実パスを基準に root_dir を絶対パスにしてから辿る。

## Examples
```gherkin
@id=EX-paths-019 @about=REQ-paths-008 @source=docs/decision/records/2026-10-06-path-resolution.md#A14
Scenario: 二つの経路で同じ結果
Given 同じ一時ディレクトリに、連なった symlink、"."・"//"・".." を含むリンク先、4096 バイト前後のパスを置く
When 同じ利用者がローカルとエージェントの経路で同じパスを確認する
Then 二つの結果は種類も実パスのバイト列もエラーの文言も一致する

@id=EX-paths-043 @about=REQ-paths-008 @source=docs/decision/records/2026-10-12-foundation-3.md#D10
Scenario: 検索できないディレクトリの下を二つの経路で確認する
Given root_dir の下の "locked" に、確認する root でない利用者の検索の権限がない
When 同じ利用者がローカルとエージェントの経路で "locked/a.txt" と "locked" を確認する
Then どの経路でも "locked/a.txt" は "cannot resolve path: permission denied" で止まり、"locked" は存在するで同じ実パスになる


@id=EX-paths-022 @about=REQ-paths-011 @source=docs/decision/records/2026-10-06-path-resolution.md#A16
Scenario: merge の書き込み先の祖先が通常ファイル
Given 書き込み先の "dir" が通常ファイルになっている
When "dir/a.txt" を merge する
Then そのファイルは "cannot resolve path: not a directory" のエラーになる

@id=EX-paths-038 @about=REQ-paths-011,REQ-cli-091 @source=docs/decision/records/2026-10-12-skip-subtree-consistency-2.md#A1
Scenario: --dry-run の merge の書き込み先の祖先が通常ファイル
Given 書き込み先の "dir" が通常ファイルになっている
When --dry-run を付けて "dir/a.txt" を merge する
Then "dir/a.txt" は merged に出ず、failed に "cannot resolve path: not a directory" のエラーで出て、終了コード 2 を返し、書き込み先の "dir" は変わらない

@id=EX-paths-039 @about=REQ-paths-011,REQ-paths-003,REQ-cli-091 @source=docs/decision/records/2026-10-12-skip-subtree-consistency-3.md#A1
Scenario: --dry-run の merge の書き込み先の実パスが長すぎる
Given 書き込み先の "deep" が実パス 4085 バイトのディレクトリへの symlink で、比べたときの書き込み先のツリーに載っており、読み込み元に "deep/a" と "deep/longname.txt" がある
When --dry-run を付けて root_dir 全体を merge する
Then "deep/longname.txt" は merged に出ず、failed に "cannot resolve path: path too long" のエラーで出て、"deep/a" は merged に status "would merge" で出て、終了コード 2 を返す

@id=EX-paths-025 @about=REQ-paths-012 @source=docs/decision/records/2026-10-06-path-resolution.md#A53
Scenario: リモートの相対の root_dir
Given リモートでログインした利用者の作業ディレクトリが symlink を含まない実パス "/home/u" で、root_dir が "app" で、"/home/u/app/a.txt" が通常ファイルである
When エージェントの経路で "a.txt" を確認する
Then 結果は存在するで、実パスは "/home/u/app/a.txt" になる
```
