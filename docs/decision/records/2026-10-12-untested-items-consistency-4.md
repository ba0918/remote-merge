# テストのない要件と例にテストを付けた単位の、四度目の IR とコードの突き合わせで決めたこと

## Context

テストのない要件と例に契約テストを付けた単位（計画 untested-items）に、main の種類の違うパスとその配下を一件のスキップにする扱い（[REQ-merge-001](../../ir/merge/symlink.md#REQ-merge-001)）を取り込んだ（マージコミット e965d6f）。
この単位は、左右のどちらにもないパスを failed に入れる扱い（[REQ-merge-035](../../ir/merge/basic.md#REQ-merge-035)）を merge と sync に入れていた。
取り込みでは、merge と sync のパスを絞る部分で、どちらにもないパスを分ける処理（`split_missing_paths`）と種類の違う対象とその配下を外す処理（`split_different_kind_subtrees`）を、この順に並べた。
この記録は、二つの要件が同じパスに当たる場合の IR の食い違いをそろえた決定と、二つの処理の順の根拠を残す。

Position: 決定は A1・A2 で出揃い、IR とテストに入れた。利用者の判断を待つものはない。

## Agreements

- A1 左右のどちらにもないパスは、その祖先が REQ-merge-001 で種類の違いによりスキップする対象でも、REQ-merge-035 によって failed に入れ（すべてないときはエラーで止まり）、そのパスのために祖先を skipped に出さない。REQ-merge-035 にこの一文を足す。コードは変えない。
  - why: 読み込み元の "x" が通常ファイル、書き込み先の "x" がディレクトリのとき、`--delete x/nope.txt`（"x/nope.txt" は左右のどちらにもない）には、REQ-merge-001 の最後の段落（配下のパスを直接指定し、走査の範囲が祖先を含むときは祖先を skipped に一件出す）と、REQ-merge-035（すべてないときはエラーで止まる）の両方が当たる。止まれば結果は出ないので、二つは同時に成り立たない（IR の中の食い違い）。REQ-merge-001 の段落は「そのパスを消さず」「書き込み先にだけあるパスは REQ-merge-024 による」と、パスがどちらかにあることを前提に書かれている。その目的（[skip-subtree の A1〜A3](./2026-10-12-skip-subtree.md#A1)：スキップしたパスの配下を書かず消さず、一件だけ示す）は、どちらにもないパスでは書くものも消すものもないので、それだけで満たされる。REQ-merge-035 の目的（[quality-merge の A3](./2026-10-05-quality-merge.md#A3)：ないパスを黙って成功にせず、警告と failed で示す）は、このパスにもそのまま当たる。一つだけ指定したときは REQ-merge-035 によるしかないので、ほかのパスと一緒に指定したときも同じ扱いにし、一緒に指定したパスでそのパスの扱いが変わらないようにした。
  - 測ったこと（使い捨てのテストで、コミットしていない。出力は `.agents/artifacts/implement-scratch/run4-probe-head.txt`）: e965d6f のローカルの経路で、merge と sync、--dry-run と --delete のそれぞれの有無で、`x/nope.txt` だけは "specified path(s) not found on either side" のエラーで止まり、`x/nope.txt other.txt` は failed に "x/nope.txt"、merged に "other.txt"、skipped は空、終了コード 2（sync の status は "partial"）だった。どの回も書き込み先の "x/only.txt" は元のままだった。契約テスト `merge_missing_paths::req_merge_035_a_path_on_neither_side_under_a_different_kind_is_failed_not_skipped`（ae4528e）は、試験 SSH サーバに対して実行ファイルを起動して同じことを確かめ、通った。
  - rejected: どちらにもないパスでも祖先を skipped に出す（REQ-merge-001 の段落を優先する）。一つだけ指定したときに REQ-merge-035 のエラーで止まる扱いと両立せず、一緒に指定したパスの有無で同じパスの結果が変わるため。
  - rejected: 一文を REQ-merge-001 の段落に足す。どちらにもないパスの扱いを決めるのは REQ-merge-035 なので、そちらに足した。REQ-merge-001 の段落は種類の違いの扱いを決めた別の単位（skip-subtree）のもので、この単位からは書き換えない。
  - decided_by: 一貫性の確認（consistency phase）
- A2 merge と sync は、左右のどちらにもないパスを分けてから、残りのパスから種類の違う対象とその配下を外す。この順は入れ替えられない。IR は変えない（A1 の一文がこの順の結果を決めている）。
  - why: 取り込みのときは「どちらにもないパスは種類の比べようがないので、どちらの順でも結果は同じ」と推測していたが、成り立たない。種類の違いを先に外すと、`split_different_kind_subtrees` は "x/nope.txt" の祖先 "x" を種類の違う対象として skipped に入れ、"x/nope.txt" を配下として外すので、どちらにもないパスが残らず、REQ-merge-035 の警告も failed も出ない。測ったこと（使い捨てで二つの呼び出しの順を crates/remote-merge/src/cli/merge.rs と sync.rs で入れ替え、すぐ戻した）: 上の契約テストは、merge の --delete の回で、終了コード 0、skipped に "x"、failed は空の出力で失敗した。戻した順では通る。
  - decided_by: 一貫性の確認（consistency phase）
