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
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A2
- verification: unit

--ref を指定した diff の JSON は、各ファイルに "ref"（参照先の "label" と "root"）と、左から参照先への差 "ref_hunks" を出し、左と参照先が同じなら "ref_hunks" を空の配列にする。
参照先のファイルを読めないときは "ref" だけを出す。--ref がなければ "ref" も "ref_hunks" も出さない。

### REQ-cli-064: diff のテキストに参照先との差を出す
- kind: state_driven
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A3
- verification: unit

--ref を指定した diff のテキストは、参照先との差が空でないファイルについて、左右の差の後に "--- ref:<参照先>:<パス> (reference diff vs left)" の見出しと、左から参照先への差の hunk を出す。

### REQ-cli-066: 左右と同じ参照先では diff の三者比較をしない
- kind: state_driven
- source: docs/decision/records/2026-09-30-adopt-diff-ref.md#A5
- verification: unit

diff の --ref の参照先が左と同じとき、diff は "Warning: --ref server is the same as left side; ref comparison skipped." を標準エラーに出し、参照先なしで比較を続ける。
参照先が右と同じときは "Warning: --ref server is the same as right side; ref comparison skipped." を出し、同じく参照先なしで続ける。

## Examples

```gherkin
@id=EX-cli-021 @about=REQ-cli-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A25,docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A7
Scenario: 三者で差分を見ながらマージする
Given 書き込み先は参照先と同じで読み込み元だけが参照先から変わったファイルがある
When --ref を指定して左から右へマージする
Then 右だけが更新され参照先のファイルは変わらない

```
