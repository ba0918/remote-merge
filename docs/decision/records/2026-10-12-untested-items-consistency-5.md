# テストのない要件と例にテストを付けた単位の、五度目の IR とコードの突き合わせで決めたこと

## Context

テストのない要件と例に契約テストを付けた単位（計画 untested-items）で、品質の確認の後の修正が二つの振る舞いを変えた。
一つは、書き込み先が複数の sync で、左右のどちらにもないパスの扱い（[REQ-merge-035](../../ir/merge/basic.md#REQ-merge-035)）を書き込み先ごとにしたこと（コミット dad3e33）。
もう一つは、diff で中身の空なファイルと通常のディレクトリの組を差分ありと数えるようにしたこと（コミット fb88371、[REQ-cli-085](../../ir/cli/diff-strategy.md#REQ-cli-085)）。
どちらも IR に書かれていない具体化だったので、IR に書き足し、その根拠を残す。

Position: 決定は A1・A2 で出揃い、IR とテストに入れた。利用者の判断を待つものはない。

## Agreements

- A1 書き込み先が複数の sync では、REQ-merge-035 の「左右」を読み込み元とそれぞれの書き込み先とし、どちらにもないパスはその書き込み先の failed に入れる。エラーで止まるのは、すべての書き込み先で指定したパスがすべてないときだけとする。接続かツリーの取得に失敗した書き込み先があるときは止まらず、その失敗とほかの書き込み先の failed を報告する。REQ-merge-035 にこの一文を足す。コードは変えない（dad3e33 の扱いのまま）。
  - why: REQ-merge-035 は「指定したパスがすべてないときは書き込まずにエラーで止まる」とだけ書き、書き込み先が複数のときにどの組の「すべて」かを決めていなかった。dad3e33 の前のコードは、一つの書き込み先でパスがすべてないだけで sync 全体を止めていた（修正の報告の RED: 書き込み先 a にだけ "x.txt" がなく b にはあるとき、`["a", "b"]: specified path(s) not found on either side`）。これは [REQ-merge-015](../../ir/merge/multi-target.md#REQ-merge-015)（一つの書き込み先が失敗しても他を処理し、書き込み先ごとの結果を報告する）と食い違う。書き込み先ごとに failed に入れ、どの書き込み先でもないときだけ止まる読みは、REQ-merge-035 の目的（[quality-merge の A3](./2026-10-05-quality-merge.md#A3)：ないパスを黙って成功にせず、警告と failed で示す）と REQ-merge-015 の両方を満たす。書き込み先が一つのときは今までと同じになる。接続や走査に失敗した書き込み先については、その書き込み先ではパスの有無が分からないので「すべての書き込み先でない」とは言えない。止めるとその失敗も、ほかの書き込み先の failed も出ず、REQ-merge-015 の書き込み先ごとの報告が失われる。測ったこと: 契約テスト `merge_missing_paths::req_merge_035_sync_does_not_stop_when_a_target_could_not_be_scanned`（書き込み先 a は "x.txt" が左右のどちらにもなく、b は root_dir を消してツリーを取れない。`[a,b]` と `[b,a]` の両順）は、a の failed に "x.txt"、b の status が Failed、終了コード 0 以外で通る。使い捨てで crates/remote-merge/src/cli/sync.rs の `connection_failures.is_empty() &&` の条件を外すと、このテストは Err（パスがないエラーで止まる）で失敗した（すぐ戻した）。書き込み先ごとの failed と、すべてないときに止まることは dad3e33 の契約テスト `req_merge_035_sync_fails_a_path_only_for_the_target_where_it_is_on_neither_side` と `req_merge_035_sync_stops_when_the_path_is_on_neither_side_for_every_target` で確かめている。三つともローカルの書き込み先で測った。止まるかの判断は走査済みのツリーと status だけを読む純粋関数（`partition_existing_files`、`every_path_missing`）なので、SSH とエージェントの経路でも同じになると推測している（測っていない）。
  - rejected: 接続できた書き込み先のすべてでパスがすべてないなら、接続に失敗した書き込み先があっても止まる。接続の失敗を報告できず、REQ-merge-015 の書き込み先ごとの報告と食い違うため。
  - rejected: 一つの書き込み先でパスがすべてないなら sync 全体を止める（dad3e33 の前の扱い）。REQ-merge-015 と食い違うため。
  - decided_by: 一貫性の確認（consistency phase）
- A2 diff で片側が通常のディレクトリ、他方が通常ファイルのパスは、ファイル側の本文が空でもそのパスを差分ありと数える。REQ-cli-085 にこの一文を足す。コードは変えない（fb88371 の扱いのまま）。
  - why: REQ-cli-085 は種類の差を示すとだけ書き、本文が空のときに差分ありと数えるかを決めていなかった。fb88371 の前のコードは、空のファイルと空のディレクトリの組を、種類の差の note を出しながら files_with_changes 0・終了コード 0 としていた（修正の報告の RED: `left: Number(0)  right: 1`）。ファイルはファイル側にだけあるので、[REQ-cli-086](../../ir/cli/diff-output.md#REQ-cli-086)（片側にだけある 0 バイトのファイルは両側を空として差分なしとせず差分ありと数える）と同じ扱いにした。REQ-cli-086 そのものは「片側にだけある」ファイルを前提にしており、もう一方にディレクトリがあるこの組には直接当たらないので、REQ-cli-085 に書いた。測ったこと（使い捨てのテストで、コミットしていない。`.agents/artifacts/implement-scratch/zz_run5_probe_kinds.rs`）: cb7874e のローカルの経路で、左の "x" が空のディレクトリ、右の "x" が空のファイルのとき、"x" を指定しても指定しなくても files_with_changes 1、終了コード 1 だった。契約テスト `diff_directory_against_file::req_cli_085_an_empty_file_against_an_empty_directory_is_counted_as_a_change`（fb88371）が同じことを左右の向きと走査方式ごとに確かめている。summary の files_with_changes が数えるもの全般の食い違い（[FLAG-cli-034](../../ir/cli/FLAGS.md#FLAG-cli-034)）は開いたままで、この一文はそれを決めない。
  - decided_by: 一貫性の確認（consistency phase）
