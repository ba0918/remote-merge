# CLI diff の未決事項のうち利用者の判断がいらないものを片付ける判断

## Context

CLI diff の取り込みで FLAG として残した未決事項のうち、承認済みの要件か実装の事実だけで決まり、利用者の判断を要しないものを片付ける。
2026-10-04 に利用者が「いちいち僕が判断したら終わらないから、仕分けして判断がいらないものを全て対処して」と指示し、読み取りだけの仕分けで各 FLAG を、承認済みの要件か実装で既に決まっているもの、現状を仕様にすればよいもの、承認済みの要件に実装が反していて直せばよいものに分けた。
この記録はそのうち CLI diff の出力・symlink・--ref に関わるものを扱う。
仕分けの前提が確かめると成り立たなかったものは片付けず、FLAG のまま残す。

## Agreements

- A1 FLAG-cli-028 を閉じる。diff の JSON は files の配列と summary を持ち、left・right は label と root を持つ形とし、旧総合仕様の一つのファイルのオブジェクトの例には合わせない。
  - why: この形は承認済みの [REQ-cli-053](../../ir/cli/diff-output.md#REQ-cli-053)（[A2 (diff の出力の取り込み)](./2026-09-29-adopt-diff-output.md#A2)）で既に決まっており、印の付いた契約テストもある。食い違っていたのは移行元の旧資料だけで、決め直す点が残っていない。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A2 FLAG-cli-029 を閉じる。リモートのバイナリのハッシュを計算する場所は仕様にせず、IR を変えない。
  - why: [REQ-cli-009](../../ir/cli/binary.md#REQ-cli-009) と [REQ-cli-061](../../ir/cli/binary.md#REQ-cli-061) は SHA-256 のハッシュを出すことだけを定め、計算する場所を定めていない。実装の diff は中身を読んでからローカルで計算するが、利用者に見える出力は計算する場所によって変わらない。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A3 FLAG-cli-052 を閉じる。diff は symlink のリンク文字列を link_targets だけで示し、hunks にはリンク文字列の削除と追加の行を入れない。機密として隠したとき、root_dir の外で内容を比べなかったとき、参照先のディレクトリが読めないとき、循環したときも hunks を空にする。
  - why: 承認済みの [REQ-cli-024](../../ir/cli/symlink-diff.md#REQ-cli-024) はリンク文字列を link_targets に置き、内容差を hunks で示すと定め、[REQ-cli-059](../../ir/cli/diff-output.md#REQ-cli-059) は --force のない機密ファイルの hunks を空にすると定める。リンク文字列が hunks に残る実装はこれに反しており、リンク文字列の違いは link_targets から差分ありと数えるため、hunks から外しても差分件数と終了コードは変わらない。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A4 FLAG-cli-053 を閉じる。diff は symlink の参照先の内容差を --max-lines で打ち切ったとき、通常のファイルと同じくそのファイルの truncated を true にし、テキストでは差分の行の後に "... (output truncated)" を出す。
  - why: 承認済みの [REQ-cli-054](../../ir/cli/diff-output.md#REQ-cli-054) は --max-lines で打ち切ったファイルの truncated を true にし、テキストで "... (output truncated)" を出すと定め、symlink の参照先の内容差を例外にしていない。実装は参照先の差分から hunks だけを取り出して打ち切りの印を捨てており、要件に反していた。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A5 FLAG-cli-065 を閉じ、現行の挙動を [REQ-cli-067](../../ir/cli/reference.md#REQ-cli-067) として仕様にする。--ref を指定した diff は、左右に差があり出力に含めるファイルについてだけ参照先との差を出し、左右に差のないファイルは参照先だけが違っても出力に加えない。
  - why: diff は左右に差のあるファイルだけを出力に含め、参照先はその中のファイルについてだけ読む（"crates/remote-merge/src/cli/diff.rs" の出力の組み立て）。参照先だけが違うファイルの数は status の --ref が既に示しており、現行の挙動を書き留めるだけで利用者の判断を要しない。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A6 FLAG-cli-066 を閉じ、現行の挙動を [REQ-cli-068](../../ir/cli/reference.md#REQ-cli-068) として仕様にする。diff は参照先を比べる相手として加えるだけで、参照先にだけあるファイルを files にも summary の数にも含めない。
  - why: diff が比べるファイルは左右のツリーか左右の読み取りだけで決まり、参照先のツリーは走査しない。参照先は [REQ-cli-062](../../ir/cli/reference.md#REQ-cli-062) の三者比較の相手として加わるだけで、比べる対象を広げる定めはどこにもなく、現行の挙動を書き留めるだけで利用者の判断を要しない。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A7 FLAG-cli-068 を閉じ、現行の挙動を [REQ-cli-069](../../ir/cli/reference.md#REQ-cli-069) として仕様にする。--ref の参照先に接続できないとき、diff は比較の結果を出さずにエラーで終了コード 2 を返す。
  - why: diff は左右を走査した後に参照先へ接続し、失敗すると main が "Error: …" を出して終了コード 2 で終える。status と merge も参照先への接続の失敗を同じくエラーにしており、diff だけを別の扱いにする定めはない。終了コード 2 をエラーとする [REQ-cli-056](../../ir/cli/diff-output.md#REQ-cli-056) とも一致する。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A8 FLAG-cli-055 を閉じる。利用者向けの手引き "skills/remote-merge/references/json-schemas.md" の Symlink の例を、リンク文字列を link_targets の left と right で出す形に直し、symlink の項目の hunks は参照先の内容差だけを持つと書き添える。IR は変えない。
  - why: 承認済みの [REQ-cli-024](../../ir/cli/symlink-diff.md#REQ-cli-024) と実装はリンク文字列を link_targets の left と right で出し、手引きの "left_symlink_target" と "right_symlink_target" はどちらにもない欄名だった。手引きを要件と実装に合わせるだけで、決め直す点がない。hunks の書き添えは [A3](#A3) の修正後の挙動に合わせる。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
