# ファイル内容の文字コードの選択

接続先と root_dir からの相対パスごとの文字コード指定、グローバル設定とプロジェクト設定の合成、CLI の上書き指定。
選んだ指定による内容の解釈は content/interpretation.md に置く。

## Requirements

### REQ-config-046: 内容設定を接続設定から分離して読む
- kind: ubiquitous
- source: docs/decision/records/2026-10-16-content-encoding.md#A8, docs/decision/records/2026-10-16-content-encoding.md#A10, docs/decision/records/2026-10-16-content-encoding.md#D2
- verification: unit

内容設定は "[content.defaults]" の接続先名ごとの文字コードと、"[[content.rules]]" の "side"、"path"、"encoding" で読み、"side" は "local" または設定にあるサーバ名とする。
文字コード名は大文字小文字を区別せず "utf-8"、"euc-jp"、"shift_jis"、"cp932"、"windows-31j"、"binary" を受け付け、他の名前と設定にない接続先は設定エラーで止める。
"shift_jis"、"cp932"、"windows-31j" は同じ CP932 相当のマッピングを指定する。

### REQ-config-047: 内容設定を二層で合成する
- kind: ubiquitous
- source: docs/decision/records/2026-10-16-content-encoding.md#A12
- verification: unit

内容設定の接続先ごとの既定値は、プロジェクト設定にあるキーだけをグローバル設定の値より優先する。
パスルールはグローバル設定の配列の後にプロジェクト設定の配列を連結し、一致する最後のルールを使う。
プロジェクト設定の既定値は、グローバル設定から継承したパスルールを消さない。

### REQ-config-048: 側とパスごとに文字コードを選ぶ
- kind: invariant
- source: docs/decision/records/2026-10-16-content-encoding.md#A3, docs/decision/records/2026-10-16-content-encoding.md#A4, docs/decision/records/2026-10-16-content-encoding.md#A12, docs/decision/records/2026-10-16-content-encoding.md#D2
- verification: unit

文字コードは、CLI のその側の指定、一致した最後のパスルール、その接続先の既定値、未設定時の解釈の順に選び、パスルールはスコープによらず既定値より優先する。
パスルールは接続先の名前と root_dir からの相対パスに当て、パターンの照合は filters.md の REQ-config-021 と REQ-config-022 と同じ方法にする。
同じ側とパスの選択は、ローカルと Agent の経路、名前の指定とディレクトリの展開と全体走査、左右と参照先の役割によらない。

### REQ-config-049: diff と merge で側ごとに上書きする
- kind: ubiquitous
- source: docs/decision/records/2026-10-16-content-encoding.md#A4, docs/decision/records/2026-10-16-content-encoding.md#A13, docs/decision/records/2026-10-16-content-encoding.md#D2, docs/decision/records/2026-10-16-content-encoding.md#D3
- verification: unit

diff と merge は "--left-encoding"、"--right-encoding"、"--ref-encoding" でその側の文字コードを上書きできる。
指定できる文字コード名は REQ-config-046 と同じとし、設定と CLI のどちらにも指定がないことを明示的な "utf-8" 指定と区別する。

## Examples

```gherkin
@id=EX-config-073 @about=REQ-config-046 @source=docs/decision/records/2026-10-16-content-encoding.md#D2
Scenario: 接続設定とは別に内容設定を読む
Given "legacy" というサーバがあり内容設定のその接続先の既定値は "EUC-JP" である
When 設定を読む
Then "legacy" の内容の文字コードは EUC-JP になり SSH の設定は変わらない

@id=EX-config-074 @about=REQ-config-046 @source=docs/decision/records/2026-10-16-content-encoding.md#D2
Scenario: 未対応の文字コード名を拒否する
Given 内容設定の文字コードに "utf-16" が指定されている
When 設定を読む
Then 設定エラーで止まる

@id=EX-config-075 @about=REQ-config-047 @source=docs/decision/records/2026-10-16-content-encoding.md#A12
Scenario: 共通のパスルールをプロジェクトの既定値より優先する
Given グローバル設定は "legacy" の "*.txt" を CP932 としプロジェクト設定は "legacy" の既定値だけを EUC-JP とする
When "legacy" の "a.txt" と "a.php" の文字コードを選ぶ
Then "a.txt" は CP932 になり "a.php" は EUC-JP になる

@id=EX-config-076 @about=REQ-config-047 @source=docs/decision/records/2026-10-16-content-encoding.md#A12
Scenario: プロジェクトの例外ルールで共通ルールを上書きする
Given グローバル設定は "legacy" の "*.txt" を CP932 としプロジェクト設定は "legacy" の "new/a.txt" を UTF-8 とする
When "legacy" の "new/a.txt" と "old/a.txt" の文字コードを選ぶ
Then "new/a.txt" は UTF-8 になり "old/a.txt" は CP932 になる

@id=EX-config-077 @about=REQ-config-048 @source=docs/decision/records/2026-10-16-content-encoding.md#A3,docs/decision/records/2026-10-16-content-encoding.md#A4
Scenario: 一つのサーバの中に異なる文字コードがある
Given "legacy" の "old/**" を EUC-JP とし "new/**" を UTF-8 とするルールがある
When 名前の指定とディレクトリの展開と全体走査でそれぞれ diff する
Then どの指定でも "old/a.php" は EUC-JP として "new/a.php" は UTF-8 として読まれる

@id=EX-config-078 @about=REQ-config-049 @source=docs/decision/records/2026-10-16-content-encoding.md#A4,docs/decision/records/2026-10-16-content-encoding.md#D3
Scenario: 参照先の指定だけを上書きする
Given 左右と参照先の文字コードにそれぞれ設定の指定がある
When "--ref-encoding euc-jp" を指定して diff する
Then 参照先だけが EUC-JP で解釈され左右には元の指定が使われる
```
