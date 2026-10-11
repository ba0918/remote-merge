# 名前とリンク先を、どの経路でも読んだとおりに一覧に載せる

## Context

品質の台帳の ao-2 と、その後の単位で見つかった sm-1・sm-2・sm-3、symlink-merge-check の R2・R6 を直す単位である。

- ao-2（[agent-operations A16](./2026-10-11-agent-operations.md#A16)、[FLAG-paths-001](../../ir/paths/FLAGS.md)・[FLAG-scan-009](../../ir/scan/FLAGS.md)）: SSH の経路の走査と子の一覧は find の出力を TAB と改行で区切って読むので、名前に TAB か改行を含む項目を読み違える。
- symlink-merge-check の R6: 同じ理由で、リンク先に TAB か末尾の改行を含む symlink は、SSH の走査がリンク先を切るので、変わっていなくても書き込み先の確かめで SSH だけ failed になる。R2: 名前に改行を含む子があるだけで、SSH の子の一覧が失敗し、その親の symlink の置き換えが SSH だけ failed になる。
- sm-1: エージェントの走査は項目の名前の "\" を "/" に替える。
- sm-2: エージェントの走査は symlink のリンク先を読めないとき、その失敗を捨てて空のリンク先にする（[REQ-scan-015](../../ir/scan/listing.md#REQ-scan-015) に反する）。
- sm-3: [REQ-scan-006](../../ir/scan/listing.md#REQ-scan-006) は「リンク先の文字列とともに載せる」とだけ書き、子の一覧の [REQ-paths-017](../../ir/paths/operations.md#REQ-paths-017) のように「読んだ文字列そのまま」とは書いていない。

利用者は 2026-10-11 に、品質の台帳の単位を順に自走して進めてよいと指示した。ここに書く決定は、どれも A16 と承認済みの要件から導いたもので、新しい方針は足していない。

Position: 決定は A1〜A2 で出揃った。

## Agreements

- A1 走査と子の一覧は、ローカル・SSH・エージェントのどの経路でも、項目の名前と symlink のリンク先を、TAB・改行・"\" を含めて読んだとおりに載せる。[REQ-scan-006](../../ir/scan/listing.md#REQ-scan-006) にリンク先を読んだ文字列そのままで載せることを書き足し、名前とリンク先に TAB と改行を含む項目の例を足す。[REQ-paths-013](../../ir/paths/operations.md#REQ-paths-013) の SSH の例外の文を消し、[FLAG-paths-001](../../ir/paths/FLAGS.md) と [FLAG-scan-009](../../ir/scan/FLAGS.md) を閉じて消す。UTF-8 でない名前の扱い（[REQ-scan-012](../../ir/scan/listing.md#REQ-scan-012)）は変えない。
  - why: A16 は「SSH の走査と一覧の出力の読み方をまとめて直す」と決めた。REQ-paths-013・REQ-paths-017 は子の一覧がどの経路でも同じ値を返すと決め、REQ-scan-006 は走査がどの経路でも symlink をリンク先とともに載せると決めている。経路によって名前やリンク先が変わると、同じファイルを経路ごとに別のものとして比べ、書き込み先の確かめも経路ごとに違う結果になる。
  - decided_by: 利用者（[agent-operations A16](./2026-10-11-agent-operations.md#A16) から導いた。2026-10-11 の自走の指示により、この単位で書き下ろした）
- A2 エージェントの走査も、symlink のリンク先を読めないときは、ローカルと SSH と同じく空のリンク先で埋めずにエラーで止まる。IR は変えない（REQ-scan-015 が既に決めている）。
  - why: REQ-scan-015 は「symlink のリンク先の文字列を読めないときも、既定の値で埋めずにエラーで止まる」と決めている。
  - decided_by: 利用者（REQ-scan-015 の当てはめ。2026-10-11 の自走の指示により、この単位で書き下ろした）

## Revisions

- 2026-10-12: [FLAG-paths-001](../../ir/paths/FLAGS.md) と [FLAG-scan-009](../../ir/scan/FLAGS.md) を A1 で閉じ、IR から消した。[adopt-scan-listing A12](./2026-09-29-adopt-scan-listing.md#A12)（FLAG として残す）を A1 で置き換えた。
