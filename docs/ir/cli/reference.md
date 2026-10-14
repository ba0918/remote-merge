# 三者比較の参照先

追加の参照サーバを指定した場合に、書き込み対象と参照対象を区別する規則と、diff での参照先との差の示し方。

## Requirements

### REQ-cli-011: 参照先を書き換えない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A25
- verification: unit

--ref で追加した参照先は比較にだけ使い、merge と sync は明示した書き込み先以外を変更しない。

### REQ-cli-062: diff に参照先を加える
- kind: state_driven
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A1
- verification: unit

diff の --ref に設定のサーバ名か "local" を指定すると、diff は参照先を加えた三者比較をする。
設定にないサーバ名を指定すると、"Server '<名前>' not found in config" のエラーで終了コード 2 を返す。

### REQ-cli-063: diff の JSON に参照先との差を出す
- kind: state_driven
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A2, docs/decision/records/2026-10-05-size-limit.md#A33, docs/decision/records/2026-10-12-foundation-4.md#E28
- verification: unit

--ref を指定した diff の JSON は、各ファイルに "ref"（参照先の "label" と "root"）と、左から参照先への差 "ref_hunks" を出し、左と参照先が同じなら "ref_hunks" を空の配列にする。
参照先のファイルを読めないときと、参照先の同じパスが左右のファイルと種類の違うものであるときは "ref" だけを出す。--ref がなければ "ref" も "ref_hunks" も出さない。
root_dir の外へ解決される項目を --follow-external-links なしで比べるときと、--max-files の枠を超えて出さないファイルには "ref" を付けない。

### REQ-cli-100: symlink とバイナリの参照先との差
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E28, docs/decision/records/2026-10-12-foundation-5.md#F84
- verification: unit

--ref を指定した diff の JSON は、symlink の項目では "link_targets" に参照先のリンク文字列 "ref" を加え（参照先が symlink でなければ null）、参照先とのリンク文字列の差を示し、"ref_hunks" を出さない。
左右か参照先のどれかがバイナリのファイル（左右がテキストで参照先だけがバイナリのものを含む）では、参照先のハッシュを "ref_hash" に出し、"ref_hunks" を出さない。

### REQ-cli-101: 参照先との差の打ち切りもファイルの打ち切りとして示す
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E28
- verification: unit

--ref を指定した diff は、--max-lines を "ref_hunks" にも当て、"ref_hunks" を打ち切ったときもそのファイルの "truncated" を true にする（REQ-cli-054）。

### REQ-cli-064: diff のテキストに参照先との差を出す
- kind: state_driven
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A3
- verification: unit

--ref を指定した diff のテキストは、参照先との差が空でないファイルについて、左右の差の後に "--- ref:<参照先>:<パス> (reference diff vs left)" の見出しと、左から参照先への差の hunk を出す。

### REQ-cli-067: 参照先との差は左右に差のあるファイルにだけ出す
- kind: state_driven
- source: docs/decision/records/2026-10-04-cli-diff-flag-resolution.md#A5
- verification: unit

--ref を指定した diff は、左右に差があり出力に含めるファイルについてだけ参照先との差を出す。
左右に差のないファイルは、参照先だけが違っても --ref によって出力に加えない。

### REQ-cli-068: 参照先にだけあるファイルを diff の出力に含めない
- kind: prohibition
- source: docs/decision/records/2026-10-04-cli-diff-flag-resolution.md#A6
- verification: unit

diff は参照先を比べる相手として加えるだけで、参照先にだけあるファイルを files にも summary の数にも含めない。

### REQ-cli-069: 参照先に接続できない diff はエラーで終える
- kind: event_driven
- source: docs/decision/records/2026-10-04-cli-diff-flag-resolution.md#A7
- verification: unit

--ref の参照先に接続できないとき、diff は比較の結果を出さずにエラーで終了コード 2 を返す。

### REQ-cli-066: 左右と同じ参照先では diff の三者比較をしない
- kind: state_driven
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A5
- verification: unit

diff の --ref の参照先が左と同じとき、diff は "Warning: --ref server is the same as left side; ref comparison skipped." を標準エラーに出し、参照先なしで比較を続ける。
参照先が右と同じときは "Warning: --ref server is the same as right side; ref comparison skipped." を出し、同じく参照先なしで続ける。

## Examples

```gherkin
@id=EX-cli-109 @about=REQ-cli-100 @source=docs/decision/records/2026-10-12-foundation-4.md#E28
Scenario: symlink の参照先との差
Given 左右と参照先の "link" は symlink で、参照先だけが違うリンク文字列を持ち、左右のリンク文字列も違う
When --ref を指定して diff --format json を実行する
Then "link" の "link_targets" の "ref" に参照先のリンク文字列が出る

@id=EX-cli-110 @about=REQ-cli-100 @source=docs/decision/records/2026-10-12-foundation-4.md#E28
Scenario: バイナリの参照先との差
Given 左右と参照先の "img.bin" はバイナリで中身がそれぞれ違う
When --ref を指定して diff --format json を実行する
Then "img.bin" に参照先の SHA-256 が "ref_hash" で出て、"ref_hunks" は出ない

@id=EX-cli-114 @about=REQ-cli-100 @source=docs/decision/records/2026-10-12-foundation-5.md#F84
Scenario: 参照先だけがバイナリ
Given 左右の "a.txt" は中身の違うテキストで、参照先の "a.txt" はバイナリである
When --ref を指定して diff --format json を実行する
Then "a.txt" に左右の差の "hunks" と参照先の SHA-256 の "ref_hash" が出て、"ref_hunks" は出ない

@id=EX-cli-111 @about=REQ-cli-101 @source=docs/decision/records/2026-10-12-foundation-4.md#E28
Scenario: 参照先との差だけが打ち切られる
Given "a.txt" は左右の差が 1 行で、左と参照先の差が 10 行ある
When --ref と --max-lines 3 を指定して diff --format json を実行する
Then "a.txt" の "truncated" は true になる

@id=EX-cli-021 @about=REQ-cli-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A25,docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A7
Scenario: 三者で差分を見ながらマージする
Given 書き込み先は参照先と同じで読み込み元だけが参照先から変わったファイルがある
When --ref を指定して左から右へマージする
Then 右だけが更新され参照先のファイルは変わらない

```
