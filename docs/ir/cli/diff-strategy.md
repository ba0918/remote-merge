# パスの指定の仕方によらない diff の比べ方

CLI diff が、パスを名前で指定したか・ディレクトリの展開か・root_dir 全体か・指定したパスの数によらず、ファイル・フィルタ・symlink・件数の上限・ディレクトリとファイルの組を compare/flow.md の同じ比べる流れで比べること。

## Requirements

### REQ-cli-080: diff はどのファイルも中身で比べる
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A4, docs/decision/records/2026-10-12-foundation-4.md#E4
- verification: unit

diff は、名前で指定したファイルも、ディレクトリの展開と root_dir 全体の下のファイルも、指定したパスの数によらず、compare/judgement.md の TBL-compare-003 の中身の比べ方で、サイズと更新時刻が同じでも中身で比べる。

### REQ-cli-081: フィルタはディレクトリの展開と全体の走査に当てる
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A4, docs/decision/records/2026-10-12-foundation-4.md#E17
- verification: unit

diff は、指定したパスの数によらず、名前で指定したファイルには include と exclude を当てずに比べ、指定したディレクトリの配下とパスを指定しないときの root_dir 全体には include と exclude を当てる。

### REQ-cli-082: 子がすべて除外されるディレクトリは差分なしとする
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A4, docs/decision/records/2026-10-05-ssh-directory-read.md#A1, docs/decision/records/2026-10-12-foundation-4.md#E17
- verification: unit

配下がすべて exclude に当たるディレクトリが左右の両方にあり、そのディレクトリを CLI diff に指定したとき、REQ-cli-077 と同じく、指定したパスの数によらず比べるファイルがないものとして差分なしを報告し（テキストでは "0 file(s) with changes"）、終了コード 0 を返す。

### REQ-cli-083: 同じ symlink は項目に出さない
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A4, docs/decision/records/2026-10-12-foundation-4.md#E17
- verification: unit

diff は、パスの指定の仕方によらず symlink を REQ-cli-020 と REQ-cli-021 で比べ、リンク文字列も参照先の中身も左右で同じ symlink を files にもテキストにも項目として出さない。

### REQ-cli-084: 件数の上限を超えても読めた差分を残す
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A4, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A20, docs/decision/records/2026-09-29-adopt-diff-output.md#A5, docs/decision/records/2026-10-05-quality-cli.md#A15, docs/decision/records/2026-09-29-adopt-scan-limits.md#A1, docs/decision/records/2026-10-12-untested-items-consistency.md#A1, docs/decision/records/2026-10-12-foundation-4.md#E17
- verification: unit

diff の走査が件数の上限を超えたとき、パスの数と指定の仕方によらず、読めた分の差分を files に残し、上限の超過をトップレベルの errors に載せて終了コード 2 を返す。
errors に載せる上限の超過の reason は、REQ-scan-008 の "Tree scan truncated at 上限 entries." で始まる案内の文とする。
名前で指定したディレクトリを展開して比べるときも、件数の上限に当てる件数は scan/limits.md の REQ-scan-010 と同じくし、ディレクトリを数えない。

### REQ-cli-085: 通常のディレクトリと通常ファイルの組は両方を示す
- kind: state_driven
- source: docs/decision/records/2026-10-05-quality-cli.md#A4, docs/decision/records/2026-10-12-untested-items-consistency-5.md#A2
- verification: unit

片側が通常のディレクトリで他方が通常ファイルのパスを diff で比べるとき、EX-cli-061 のディレクトリ symlink と通常ファイルの組と同じく、種類の差を示し、ディレクトリ側の子ファイルとファイル側の本文をそれぞれ比較結果として示す。
ファイルはファイル側にだけあるので、REQ-cli-086 の片側にだけある 0 バイトのファイルと同じく、ファイル側の本文が空でもそのパスを差分ありと数える。

## Examples

```gherkin
@id=EX-cli-072 @about=REQ-cli-080 @source=docs/decision/records/2026-10-05-quality-cli.md#A4
Scenario: 21 個のパスの中にメタデータが同じで中身の違うファイルがある
Given "a.txt" は左右でサイズと更新時刻が同じで中身が違う
When "a.txt" を含む 21 個のファイルのパスを指定して diff を実行する
Then "a.txt" の中身の差分が出て終了コード 1 を返す

@id=EX-cli-073 @about=REQ-cli-081 @source=docs/decision/records/2026-10-05-quality-cli.md#A4
Scenario: exclude に当たるファイルを名前で指定する
Given exclude に "*.log" があり "app.log" は左右で中身が違う
When "app.log" を 1 個だけ指定した diff と、"app.log" を含む 21 個のパスを指定した diff を実行する
Then どちらも "app.log" の差分が出る

@id=EX-cli-074 @about=REQ-cli-081 @source=docs/decision/records/2026-10-05-quality-cli.md#A4
Scenario: exclude に当たるファイルをディレクトリの展開で見る
Given exclude に "*.log" があり "logs/app.log" は左右で中身が違う
When "logs" を指定して diff を実行する
Then "logs/app.log" は比べられず差分に出ない

@id=EX-cli-075 @about=REQ-cli-082 @source=docs/decision/records/2026-10-05-quality-cli.md#A4,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A2,docs/decision/records/2026-10-05-ssh-directory-read.md#A1,docs/decision/records/2026-09-29-adopt-diff-output.md#A7
Scenario: 配下がすべて除外されるディレクトリを指定する
Given exclude に "target" があり、左右にある "target" の配下のファイルは中身が違う
When CLI diff で "target" と "target/" をそれぞれ指定する
Then どちらも "0 file(s) with changes" を出し終了コード 0 を返す

@id=EX-cli-076 @about=REQ-cli-083 @source=docs/decision/records/2026-10-05-quality-cli.md#A4
Scenario: リンク文字列も参照先も同じ symlink
Given "link" は左右で同じリンク文字列を持ち参照先の中身も同じである
When "link" を指定した diff と、パスを指定しない diff を実行する
Then どちらも "link" の項目を出さない

@id=EX-cli-077 @about=REQ-cli-084 @source=docs/decision/records/2026-10-05-quality-cli.md#A4,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A20,docs/decision/records/2026-09-29-adopt-diff-output.md#A5
Scenario: パスを指定しない diff で件数の上限を超える
Given 左右の root_dir に 10 件のファイルがあり --max-entries に 5 を指定する
When パスを指定しない diff と、そのディレクトリを指定した diff を実行する
Then どちらも読めた分の差分を files に残し、上限の超過を errors に載せて終了コード 2 を返す

@id=EX-cli-078 @about=REQ-cli-085 @source=docs/decision/records/2026-10-05-quality-cli.md#A4
Scenario: 通常のディレクトリと通常ファイルの組
Given 左の "x" は子ファイルを持つ通常のディレクトリで、右の "x" は通常ファイルである
When "x" を指定して diff を実行する
Then 種類の差に加えて左の子ファイルと右の本文がそれぞれ比較結果に示される
```
