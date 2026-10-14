# 名前とリンク先を読んだとおりに載せる単位で、走査の更新時刻と exclude に当たる辿れない symlink を IR とそろえたこと

## Context

この単位の修正で、ローカルの走査は更新時刻をナノ秒まで読み、1970 年より前の更新時刻も読んだ値で載せるようになり、SSH の走査は大きさ・更新時刻・権限を読めない項目と、終了コード 0 で出た find の診断で止まるようになった。修正後のコードを [REQ-scan-015](../../ir/scan/listing.md#REQ-scan-015) と突き合わせると、二つの点で IR とコードがそろっていなかった。

- 三つの経路の走査が更新時刻をナノ秒まで同じ値で載せることを契約テストが確かめているが、走査の IR にはその規則がない（子の一覧では [REQ-paths-017](../../ir/paths/operations.md#REQ-paths-017) が決めている）。
- リンク先を辿れない symlink が exclude に当たるとき、経路によって走査が止まるかどうかが分かれていた。

Position: 決定は A1〜A2 で出揃い、IR とコードに入れた。

## Agreements

- A1 ツリーの走査は、どの経路でも項目の更新時刻を、1970 年より前のものも含めて読んだ値のままナノ秒まで載せ、SSH の経路は find が出した桁までとする。これを [REQ-scan-016](../../ir/scan/listing.md#REQ-scan-016) として足し、秒しか出さない find の差は [FLAG-scan-015](../../ir/scan/FLAGS.md) に残す。[FLAG-paths-003](../../ir/paths/FLAGS.md) の「未確認」は確かめた事実に直す。
  - why: 利用者は [quality-cli A3](./2026-10-05-quality-cli.md#A3) で「各側が出せる最も細かい精度で比べる」と決め、IR の例（[EX-cli-089](../../ir/cli/status.md#EX-cli-089) と [merge の比べ方の例](../../ir/merge/comparison.md#REQ-merge-020)）も「ローカルではナノ秒」を前提にしている。子の一覧は REQ-paths-017 で同じ規則を持つ。走査の更新時刻は status の判定（[TBL-cli-001](../../ir/cli/status.md#TBL-cli-001)）の入力なので、利用者が観測できる。既存の IR と矛盾しない具体化で、契約テスト `req_scan_016_a_modification_time_before_1970_is_listed_as_read_on_all_three_routes` の印を REQ-scan-015 から REQ-scan-016 に移した。測ったこと（使い捨てで、コミットしていない）: `docker run centos:5` で `find /r -mindepth 1 -printf '%f %T@\n'` は `GNU find version 4.2.27`・`old.txt -1000001`・`new.txt 1791738526` を出し、同じスクリプトを `ubuntu:26.04` で動かすと `find (GNU findutils) 4.10.0`・`old.txt -1000001.7500000000`・`new.txt 1791738526.8710376270` を出した（old.txt は `touch -d "1969-12-20 10:13:19.75 UTC"`）。4.2.27 の %T@ は秒しか出さないので、パーサでは差をなくせない。
  - decided_by: 一貫性の確認（consistency phase）
- A2 exclude に当たる symlink と、exclude に当たるディレクトリの下の symlink は、ループしているか途中にディレクトリでないものを含むためにリンク先を辿れなくても、どの経路の走査も止めずに一覧に載せない。これを REQ-scan-015 に書き足す。ローカルの走査は、walkdir がリンク先を辿れずに返したエラーのパスが exclude に当たるならそれを飛ばす。SSH の走査は、find の C ロケールの診断 `find: 'P': Not a directory` と `find: 'P': Too many levels of symbolic links` も、P が exclude に当たるかその下なら除いたパスの失敗として見分ける。4.2.27 の引用しない診断は見分けず、[FLAG-config-016](../../ir/config/FLAGS.md) に足す。
  - why: [REQ-config-021](../../ir/config/filters.md#REQ-config-021) と [REQ-config-022](../../ir/config/filters.md#REQ-config-022) は、どの経路の走査も同じに除外し、除外したディレクトリの下の循環する symlink や読めないディレクトリでは止めないと決めている。[2026-10-12-ssh-output-parsing-consistency.md の A2](./2026-10-12-ssh-output-parsing-consistency.md#A2) は、除いた項目を読めないだけで走査を止めないために exclude を情報を読む前に当てると決めた。辿れない symlink だけを例外にする理由は IR にない。測ったこと（使い捨てのテストで、コミットしていない）: "top.txt" と、"n" -> "top.txt/x" か "d/n" -> "../top.txt/x" を置き、`status --format json` を三つの経路で動かした。exclude "n" と "[n]" ではローカルが `Directory scan incomplete: IO error for operation on …/n: Not a directory (os error 20)` で、SSH が find の終了コード 1 で止まり（終了コード 2）、エージェントは成功した。exclude "[d]" ではローカルとエージェントが成功し、SSH だけが止まった。exclude "d" はどの経路も成功した。find の診断は `docker run ubuntu:26.04`（findutils 4.10.0）で、`'(' -name 'n' ')' -prune -o` があっても `find: '/r/n': Not a directory` を、互いを指す "a"・"b" では `find: '/r/a': Too many levels of symbolic links` を出し、終了コード 1 で終えた。`docker run centos:5`（4.2.27）は、"-name n" で刈り込むと診断を出さず終了コード 0、"a"・"b" を刈り込んでも `find: /r/a: Too many levels of symbolic links` を出して終了コード 1、"[d]" の下では引用しない `find: /r/d/n: Not a directory` を出した。修正の前、契約テスト `req_scan_015_the_local_scan_skips_an_excluded_symlink_whose_target_cannot_be_followed` と同じ SSH のテストは失敗し、エージェントのテストは成功した。
  - decided_by: 一貫性の確認（consistency phase）
