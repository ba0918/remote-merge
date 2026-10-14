# 名前とリンク先を読んだとおりに載せる単位で、走査の項目の情報の扱いを IR とそろえたこと

## Context

[2026-10-12-ssh-output-parsing.md](./2026-10-12-ssh-output-parsing.md) の A2 は、エージェントの走査がリンク先を読めないときに止まると決めた。実装の後に変えたコードを [REQ-scan-015](../../ir/scan/listing.md#REQ-scan-015) と突き合わせると、同じエージェントの走査（`crates/remote-merge-agent/src/agent/tree_scan.rs` の `process_entry`）に、項目の情報を既定の値で埋めるか飛ばす箇所が二つ残っていた。

- 更新時刻を UNIX 紀元からの経過時間に直せないとき (0, 0) で埋める。1970 年より前の更新時刻がこれに当たる。
- 項目の lstat に失敗すると警告して項目を落とす。読めるが検索の権限のないディレクトリ（0o444）の中の項目がこれに当たる。

Position: 決定は A1〜A4 で出揃い、A1・A2 はコードに入れた。IR は変えていない。

## Agreements

- A1 エージェントの走査は、1970 年より前の更新時刻を 0 で埋めずに、読んだ値（ナノ秒まで、秒は負の値に切り下げる）で載せる。読み方は子の一覧と同じ `remote_merge_core::listing::modified` を使い、OS が更新時刻を返さないときは不完全な走査として止める。IR は変えない。
  - why: REQ-scan-015 は更新時刻を既定の値で埋めないと決めている。1970 年より前の更新時刻は読める値で、子の一覧（[REQ-paths-017](../../ir/paths/operations.md#REQ-paths-017)）も既存のテスト `an_entry_modified_before_1970_is_listed_with_the_same_time_on_all_three_routes` で読んだ値として扱っている。だから「読めない」ではなく「読んだとおりに載せる」が IR に合う。(0, 0) で埋めると、サイズが同じでも更新時刻の違う二つのファイルが同じ更新時刻とみなされ、ディレクトリ指定の merge と status が中身を比べずに同じと判定しうる（[REQ-merge-020](../../ir/merge/comparison.md#REQ-merge-020) の判定の入力が変わる）。測ったこととして、使い捨てのテストで更新時刻を `UNIX_EPOCH - 1_000_000 秒` にしたファイルをエージェントの走査にかけると `("top.txt", 0, 0)` が返り、修正の前のテスト `a_modification_time_before_1970_is_listed_as_read_instead_of_a_default` は `left: (0, 0) right: (-1000001, 750000000)` で失敗した。
  - decided_by: 一貫性の確認（consistency phase）
- A2 エージェントの走査は、項目の lstat に失敗したとき、その項目を飛ばさずに不完全な走査として止める。exclude に当たる項目は、情報を読む前に除く。
  - why: REQ-scan-015 は「root_dir や項目の情報を読めなければ走査を止める」とし、更新時刻を読めないときも止まると決めている。lstat に失敗した項目は更新時刻も読めない。同じ状態で、ほかの経路はすでに止まる。exclude を先に当てるのは、除いた項目を読めないだけで走査を止めないためで、読めないディレクトリの扱い（[REQ-scan-014](../../ir/scan/limits.md#REQ-scan-014)）の exclude の扱いと同じ向きにした。測ったこと（どれも使い捨てで、コミットしていない）として、中に a.txt を置いた 0o444 のディレクトリを、エージェントの走査は `OK [("top.txt", ...)]` で `s/a.txt` を落として返した。同じ状態のローカルの `scan_local_tree_recursive_with_include` は `Err("Permission denied (os error 13)")` で止まり、`LC_ALL=C find -L <dir> -mindepth 1 ! -xtype l` は `.../s/a.txt: Permission denied` を出して終了コード 1 で終わった。
  - decided_by: 一貫性の確認（consistency phase）
- A3 実装者の決定 [2026-10-11-ssh-output-parsing-implement.md の A1](./2026-10-11-ssh-output-parsing-implement.md#A1)（ローカルの再帰の走査もリンク先を読めなければ止める）は、REQ-scan-015 の「どの経路も」の当てはめで、IR を変える必要はない。
  - why: REQ-scan-015 は経路を限らずに「symlink のリンク先の文字列か更新時刻を読めないときも、既定の値で埋めずにエラーで止まる」と書いている。
  - decided_by: 一貫性の確認（consistency phase）
- A4 ローカルの `scan_dir_with_limit` が読めないリンク先を "???" で埋めることと、SSH の `list_dir`（strict でない方）が find の終了コードを見ないことは、この単位では直さない。どちらも IR の項目にならない。
  - why: どちらも、`runtime/side_io.rs` の `fetch_tree`・`fetch_children` からしか呼ばれない。この二つを呼ぶ製品のコマンドはなく、呼ぶのはテストだけ（`rg "fetch_children|fetch_tree" crates/remote-merge/src` で確かめた）。利用者がこの違いを観測できる道がないので、[REQ-paths-017](../../ir/paths/operations.md#REQ-paths-017) の子の一覧（`list_children`）にも REQ-scan-015 の走査にも当たらない。
  - decided_by: 一貫性の確認（consistency phase）
