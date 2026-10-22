# --hunks の merge の名前で指定したファイルと仕様の突き合わせ

## Context

--hunks の merge でも、フィルタに当たる名前で指定したファイルを比べ直すようにした変更（103a3bf7）について、変わったコードと仕様を突き合わせた。
[REQ-merge-022](../../ir/merge/basic.md#REQ-merge-022) は名前で指定したファイルにフィルタを当てない規則を「sync も同じ」と書くが --hunks を名指ししておらず、--hunks の merge にも当たるのかを読み分ける必要があった。
また、変更が比べ直した結果を受け取る `merge_hunks` の、左右のどちらにもないパスで止まる分かれ道が、[REQ-merge-035](../../ir/merge/basic.md#REQ-merge-035) の警告を出していなかった。

Position: REQ-merge-022 と [REQ-scan-019](../../ir/scan/limits.md#REQ-scan-019) は --hunks の merge にも当たると読み、IR は変えない（A1）。--hunks の merge が左右のどちらにもないパスを警告してから止まるようコードを直した（A2、53e567d3）。

## Agreements

- A1 [REQ-merge-022](../../ir/merge/basic.md#REQ-merge-022) の「名前で指定したファイルには include と exclude を当てずに書き」と、[REQ-scan-019](../../ir/scan/limits.md#REQ-scan-019) の「名前で指定したファイルは、フィルタに当たっても起点そのものとして集める（merge・sync）」は、--hunks の merge にも当たる。IR は変えない。103a3bf7 のコードはこの読みと合う。
  - why: [REQ-merge-032](../../ir/merge/hunks.md#REQ-merge-032) は --hunks の番号を、同じ --left・--right・パスで実行した diff --format json の hunks の区切りで数えるとし、diff は名前で指定したファイルをフィルタに当てずに比べる（[REQ-cli-081](../../ir/cli/diff-strategy.md#REQ-cli-081)）。フィルタに当たる名前で指定したファイルを --hunks の merge が集めなければ、REQ-merge-032 が前提にする「同じパスの diff が示した hunk を番号で選んで当てる」ことができない。REQ-merge-022 と REQ-scan-019 が引く [review-decisions の A3](./2026-10-15-review-decisions.md#A3) も「merge・sync で名前で指定したファイルには include と exclude を当てず」とし、--hunks を除いておらず、その why（実在して中身も違うファイルが「両側にない」になり書かれなかった）は --hunks にもそのまま当たる。IR は --hunks の merge を除く規則を「ファイル全体の merge」と書いて限っているので（docs/ir/cli/merge.md:51 の [REQ-cli-051](../../ir/cli/merge.md#REQ-cli-051)、docs/ir/cli/merge.md:74 の [REQ-cli-092](../../ir/cli/merge.md#REQ-cli-092)）、限りのない「merge」であることは --hunks を除く理由にならない。ただし限りのない形だけでは決まらない。[REQ-merge-018](../../ir/merge/basic.md#REQ-merge-018) の「読み込み元の内容に更新する」や [REQ-merge-021](../../ir/merge/basic.md#REQ-merge-021) の「配下で中身の違うファイルを上書き」のように、ファイル全体やディレクトリの配下を書くことを中身とする項目は、一つのテキストファイルの選んだ hunk だけを書く --hunks の merge（hunks.md の [REQ-merge-009](../../ir/merge/hunks.md#REQ-merge-009)）にそのままは当たらない。REQ-merge-022 のフィルタの二つの文（二文目と三文目）は、名前で指定したファイルにフィルタを当てるかと、どう集めるかを述べ、--hunks の merge も同じ比べる流れと [TBL-scan-001](../../ir/scan/limits.md#TBL-scan-001) の走査の範囲で集めるので、この種の項目ではない。docs/ir/merge/concurrency.md:12 は「--hunks の merge … も同じである」と名指すが、この読みと食い違わない。したがって 103a3bf7 の前のコード（--hunks の merge だけが比べ直さず "specified path(s) not found on either side" で止まる）が IR と食い違っていて、IR に穴はなかった。「--hunks の merge も同じ」を REQ-merge-022 に書き足すことは読みを変えないので、しない。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A2 --hunks の merge は、指定したパスが左右のどちらにもないとき、ファイル全体の merge と同じくそのパスを示す警告を標準エラーに出してから、書き込まずにエラーで止まる。IR は変えず、コードを [REQ-merge-035](../../ir/merge/basic.md#REQ-merge-035) に合わせた（53e567d3。crates/remote-merge/src/cli/merge_hunks.rs の `comparison.paths.get(&path)` が None の分かれ道で、merge_files.rs の `warn_missing_name` を呼ぶ）。
  - why: REQ-merge-035 は「merge と sync は、指定したパスが左右のどちらにもないとき、--dry-run かどうかによらず、パスごとに警告を出して…すべてないときは書き込まずにエラーで止まる」とする。--hunks の merge で指定したパスが左右のどちらにもないときの扱いを決める IR の項目はこれだけで（hunks.md の [TBL-merge-001](../../ir/merge/hunks.md#TBL-merge-001) の --hunks の誤りにこの場合はない）、103a3bf7 のコードもその後半の「すべてないときはエラーで止まる」を --hunks に当てていた（`paths_not_found`）。同じ一文の後半だけを当て、前半の「パスごとに警告を出して」を当てない読みは IR から出てこない。[quality-merge の A3](./2026-10-05-quality-merge.md#A3) の目的（ないパスを diff と同じく警告と failed で示す）も --hunks に当たる。ファイル全体の merge は、すべてないときも警告してから止まる（下の Verification の probe F）。--hunks は指定できるパスが一つなので、ないときは常に「すべてない」で、failed は出ずにエラーで止まる。警告の文言は REQ-merge-035 のとおり IR で決めず、テストはパスが示されることだけを確かめる。この分かれ道は、103a3bf7 が比べ直した結果を受け取る箇所で、比べ直しの後は本当に左右のどちらにもないときだけここに来る。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- `kotowari check --format json`: too_many_requirements の notice が 2 件（docs/ir/backup/rollback-path.md・docs/ir/config/loading.md）で、どちらも範囲の外。範囲の IR に IR 側の指摘はない。
- 使い捨ての測定（リポジトリに入れない。関数呼び出しの契約テストの道具で 103a3bf7 に対して動かした）:
  - A: 何もフィルタのない構成で、左右のどちらにもない "nope.txt" を --hunks 0 で merge すると "specified path(s) not found on either side" で止まった。
  - B: exclude = ["*.log"] で、左右のどちらにもない "nope.log" を --hunks 0 で merge しても同じエラーで止まった（比べ直しの後も、ないパスは REQ-merge-035 の扱いになる）。
  - C: include = ["other"] で、include の外の "app.txt"（左右で中身が違う）を --hunks 0 で merge すると、終了コード 0、merged に "app.txt"、書き込み先の中身は読み込み元と同じになった。
  - D: exclude = ["logs"] で、"logs/app.log" を --hunks 0 で merge すると、終了コード 0 で書き込み先が読み込み元の中身になった。
  - E: exclude = ["*.log"] で、読み込み元にだけある "app.log" を --hunks 0 で merge すると、終了コード 0 で書き込み先に作られた。
  - F: exclude = ["*.log"] で、左右のどちらにもない "nope.log" をファイル全体で merge すると、"Warning: 'nope.log' not found on either side" を出してから同じエラーで止まった。A・B の --hunks はこの警告を出していなかった。
- RED: 契約テスト `merge_missing_paths::req_merge_035_a_hunk_merge_warns_of_a_path_on_neither_side_before_stopping`（実行ファイルを試験 SSH サーバに対して起動し、"missing.txt" を --hunks 0 で、--dry-run の有無で merge する）は、103a3bf7 では終了コード 2 だが標準エラーに "missing.txt" がなく失敗した。
- GREEN: 53e567d3 で `cargo nextest run -p remote-merge --all-features -E 'test(/merge|sync|hunk/)'` の 371 件が通った。`cargo fmt --all --check` と `cargo clippy --all-targets --all-features -- -D warnings` も通った。
