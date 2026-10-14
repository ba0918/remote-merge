# テストのない要件と例にテストを付けた単位で、diff の比べ方を IR とそろえたこと

## Context

テストのない要件と例に契約テストを付け、IR と違う動きを IR のとおりに直した単位（計画 untested-items）の後、変わったコードを IR と突き合わせた。
diff は名前で指定したパスを数によらず一つずつ比べ、ディレクトリを指定したときは走査せずに子の一覧で展開するようになった。
その展開と全体の走査とで、件数の上限の数え方、ディレクトリとファイルの組の示し方、名前で指定したファイルへのフィルタの当て方がそろっていなかった。
この記録は、それを IR とコードでそろえた決定と、その根拠の測定を残す。

Position: 決定は A1〜A4 で出揃った。A1・A2・A4 は IR とコードに入れ、A3 は FLAG-scan-016 に残して利用者の判断を待つ。

## Agreements

- A1 名前で指定したディレクトリを diff が展開して比べるときも、件数の上限に当てる件数にはファイルと symlink だけを数え、ディレクトリを数えない。これを [REQ-cli-084](../../ir/cli/diff-strategy.md#REQ-cli-084) に書き足し、展開する子のうち、どちらかの側でディレクトリでないものだけを数える。
  - why: [REQ-scan-010](../../ir/scan/limits.md#REQ-scan-010) は、どの経路の走査も件数にディレクトリを数えないと決め、REQ-cli-084 は diff の上限の超過を走査方式によらず同じに扱うと決めている。展開でだけディレクトリを数える理由は IR にない。測ったこと（使い捨てのテストで、コミットしていない）: 左右に "d/a.txt" と "d/sub/b.txt" を置き（大きさが左右で違う）、`--max-entries 2` で diff を動かすと、パスを指定しないときは上限を超えずに二つの差分を出し、"d" を指定したときは "d/sub" に `Tree scan truncated at 2 entries.` を出して終了コード 2 を返し、"d/sub/b.txt" を比べなかった。
  - decided_by: 一貫性の確認（consistency phase）
- A2 diff は、"."・"./"・空の値・"/" や glob 文字を含むパスと一緒に名前で指定したパスも、一つずつ比べてフィルタを当てない。root_dir 全体の走査の結果と合わせるとき、名前で指定したパスには一つずつ比べた結果だけを使い、比べるパスは全体の走査と同じくパスの順に並べる。どちらか一方で指定したパスが見つからなくても、もう一方の結果があれば使う。IR は変えない。
  - why: [REQ-cli-081](../../ir/cli/diff-strategy.md#REQ-cli-081) は、指定したパスの数と走査方式によらず名前で指定したファイルに include と exclude を当てないと決めている。[REQ-cli-052](../../ir/cli/diff-output.md#REQ-cli-052) は "." があるときに root_dir 全体を比べると決めており、名前で指定したファイルを足しても全体の比べ方は変わらない。測ったこと（使い捨てのテストで、コミットしていない）: exclude "*.log" で左右の "app.log" と "a.txt" の中身を変え、`diff app.log` は "app.log" の差分を出したが、`diff . app.log` は "a.txt" だけを出して "app.log" を警告もなく比べなかった。
  - decided_by: 一貫性の確認（consistency phase）
- A3 名前で指定したディレクトリを diff が展開するときの子の一覧は、ツリーの走査ではないものとして [REQ-scan-012](../../ir/scan/listing.md#REQ-scan-012) を当てず、UTF-8 として読めない名前の子をこれまでどおり置換文字に置き換えた名前で扱う（[実装者の A5](./2026-10-12-untested-items-implement.md#A5) のまま）。置き換えた名前で出る誤りを [FLAG-scan-016](../../ir/scan/FLAGS.md) に残し、利用者の判断を待つ。
  - why: [REQ-scan-009](../../ir/scan/limits.md#REQ-scan-009) は、diff が root_dir 全体を走査するのはパスを指定しないときだけと決めており、REQ-scan-012 はツリーの走査を対象にする。一方で REQ-scan-012 の根拠の [quality-scan A3](./2026-10-05-quality-scan.md#A3) は「置き換えた名前では読めず、別の名前と衝突しうる」で、子の一覧にも同じことが言える。どちらの読みも IR と矛盾せず、測っても決まらない。子の一覧に当てるには、ローカル・SSH・エージェントの三つの経路の子の一覧と、エージェントの通信を変える。測ったこと（使い捨てのテストで、コミットしていない）: 左の "d" に "bad\xff.txt" と "ok.txt" を置き、パスを指定しない diff は errors に `d/bad\xff.txt`（理由 `local: file name is not valid UTF-8`）を出し、`diff d` と `diff d/` は errors に置換文字を含む `d/bad\u{fffd}.txt`（理由 `not found on either side`）を出した。どれも終了コード 2 を返した。
  - decided_by: 一貫性の確認（consistency phase）。利用者の判断を待つ
- A4 走査が報告する UTF-8 として読めない名前のパスは、root_dir からのパスの読める部分をそのままにし、読めない部分のバイトを一つずつ "\xNN"（小文字の 16 進 2 桁）と書いた形とする（[実装者の A5](./2026-10-12-untested-items-implement.md#A5) の表し方）。これを [REQ-scan-012](../../ir/scan/listing.md#REQ-scan-012) に書き足し、status の三つの経路の契約テストでその形を確かめる。
  - why: REQ-scan-012 が実装者に委ねているのは failed の理由の文言だけで、報告するパスの形は決めていないが、そのパスは status と diff の errors、merge と sync の failed の path として利用者に見える。置換文字を使わずに元のバイトを示せる形で、[quality-scan A3](./2026-10-05-quality-scan.md#A3) の「置き換えた名前では読めず、別の名前と衝突しうる」を避ける。既存の IR と矛盾しない具体化。測ったこと: 名前が "bad" と 0xff と ".txt" のファイルを置いた status（ローカル・SSH・エージェント）の契約テストで、errors のパスが `bad\xff.txt` だけであることを確かめ、三つとも通った。
  - decided_by: 一貫性の確認（consistency phase）
