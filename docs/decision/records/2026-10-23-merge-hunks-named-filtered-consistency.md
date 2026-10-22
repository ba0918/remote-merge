# --hunks の merge の名前で指定したファイルと仕様の突き合わせ

## Context

--hunks の merge でも、フィルタに当たる名前で指定したファイルを比べ直すようにした変更（103a3bf7）について、変わったコードと仕様を突き合わせた。
[REQ-merge-022](../../ir/merge/basic.md#REQ-merge-022) は名前で指定したファイルにフィルタを当てない規則を「sync も同じ」と書くが --hunks を名指ししておらず、--hunks の merge にも当たるのかを読み分ける必要があった。
また、変更が比べ直した結果を受け取る `merge_hunks` の、左右のどちらにもないパスで止まる分かれ道が、[REQ-merge-035](../../ir/merge/basic.md#REQ-merge-035) の警告を出していなかった。

Position: REQ-merge-022 と [REQ-scan-019](../../ir/scan/limits.md#REQ-scan-019) は --hunks の merge にも当たると読み、IR は変えない（A1）。--hunks の merge が左右のどちらにもないパスを警告してから止まるようコードを直した（A2、53e567d3）。

## Agreements

- A1 [REQ-merge-022](../../ir/merge/basic.md#REQ-merge-022) の「名前で指定したファイルには include と exclude を当てずに書き」と、[REQ-scan-019](../../ir/scan/limits.md#REQ-scan-019) の「名前で指定したファイルは、フィルタに当たっても起点そのものとして集める（merge・sync）」は、--hunks の merge にも当たる。IR は変えない。103a3bf7 のコードはこの読みと合う。
  - why: IR は、--hunks の merge を除く規則を「ファイル全体の merge」と書いて限っている（docs/ir/cli/merge.md:51 の [REQ-cli-051](../../ir/cli/merge.md#REQ-cli-051)、docs/ir/cli/merge.md:74 の [REQ-cli-092](../../ir/cli/merge.md#REQ-cli-092)）。限りのない「merge」は --hunks の merge を含む形で、[TBL-compare-003](../../ir/compare/judgement.md#TBL-compare-003) の「merge・sync｜名前で指定したファイル｜中身」も同じ形である。REQ-merge-022 と REQ-scan-019 が引く [review-decisions の A3](./2026-10-15-review-decisions.md#A3) も「merge・sync で名前で指定したファイルには include と exclude を当てず」とし、--hunks を除いていない。docs/ir/merge/concurrency.md:12 は「--hunks の merge … も同じである」と名指すが、これは限りのない形が --hunks を含むことと食い違わない。したがって 103a3bf7 の前のコード（--hunks の merge だけが比べ直さず "specified path(s) not found on either side" で止まる）が IR と食い違っていて、IR に穴はなかった。「--hunks の merge も同じ」を REQ-merge-022 に書き足すことは読みを変えないので、しない。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A2 --hunks の merge は、指定したパスが左右のどちらにもないとき、ファイル全体の merge と同じくそのパスを示す警告を標準エラーに出してから、書き込まずにエラーで止まる。IR は変えず、コードを [REQ-merge-035](../../ir/merge/basic.md#REQ-merge-035) に合わせた（53e567d3。crates/remote-merge/src/cli/merge_hunks.rs の `comparison.paths.get(&path)` が None の分かれ道で、merge_files.rs の `warn_missing_name` を呼ぶ）。
  - why: REQ-merge-035 は「merge と sync は、指定したパスが左右のどちらにもないとき、--dry-run かどうかによらず、パスごとに警告を出して…すべてないときは書き込まずにエラーで止まる」とし、A1 と同じく限りのない「merge」なので --hunks の merge にも当たる。同じ文書の同じ語を REQ-merge-022 では含む、REQ-merge-035 では含まないと読み分ける根拠はない。[quality-merge の A3](./2026-10-05-quality-merge.md#A3) の目的（ないパスを diff と同じく警告と failed で示す）も --hunks に当たる。ファイル全体の merge は、すべてないときも警告してから止まる（下の Verification の probe F）。--hunks は指定できるパスが一つなので、ないときは常に「すべてない」で、failed は出ずにエラーで止まる。警告の文言は REQ-merge-035 のとおり IR で決めず、テストはパスが示されることだけを確かめる。この分かれ道は、103a3bf7 が比べ直した結果を受け取る箇所で、比べ直しの後は本当に左右のどちらにもないときだけここに来る。
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
