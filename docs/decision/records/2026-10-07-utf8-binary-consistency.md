# ファイル全体の UTF-8 の正しさで分ける単位で IR と記録を実装にそろえる

## Context

テキストとバイナリの判定をファイル全体の UTF-8 の正しさで行う単位（計画 utf8-binary）の変更を、IR と決定記録に突き合わせ直した。
判定の規則（[REQ-cli-061](../../ir/cli/binary.md#REQ-cli-061)）と --hunks の merge の止め方（[REQ-merge-028](../../ir/merge/hunks.md#REQ-merge-028)）は実装と一致していた。
一方で、この単位が消した文字列の読み込みの入口を名指しする決定と、判定の変更の前の動きを書いた FLAG の本文が、今の実装と合わなくなっていた。
また、中身の同じバイナリを項目に出さないとする [REQ-cli-009](../../ir/cli/binary.md#REQ-cli-009) に対し、ファイルを名前で指定した diff の実装が項目を出しており、この単位の判定の変更でそれに当たるファイルが広がっていた。
この記録は、それぞれをどう直したかと、その根拠を残す。

Position: A1〜A3 を反映した。

## Agreements

- A1 ファイルの大きさの上限の判定は、製品のすべての中身の読み込みが通る `CoreRuntime` の二つの読み込みの入口（read_file_bytes・read_files_bytes_batch）に置く。[2026-10-05-size-limit.md の A1](./2026-10-05-size-limit.md#A1) の、入口に一つだけ置き、経路に振り分ける前に大きさだけを尋ねて上限を超えるパスを外すという決定は変えない。
  - why: size-limit の A1 は入口を四つ（read_file・read_files_batch・read_file_bytes・read_files_bytes_batch）と名指ししていたが、この単位で製品のどこからも呼ばれていなかった文字列の読み込みの二つを消した（`crates/remote-merge/src/runtime/side_io.rs` の「読み込み」の節）。`rg -n "fn read_file\b|fn read_files_batch\b" crates --type rust` は何も返さず、残る入口は二つで、どちらも経路に振り分ける前に `refuse_over_limit`・`paths_within_limit` で上限を当てている。上限の振る舞い（[REQ-cli-079](../../ir/cli/size-limit.md#REQ-cli-079)）は入口の名前を書いておらず、IR は変わらない。
  - decided_by: 整合の段（consistency phase）
- A2 [FLAG-cli-059](../../ir/cli/FLAGS.md#FLAG-cli-059) の本文を今の判定に合わせ、左右がバイナリになる条件を「先頭 8192 バイトに NUL を含むか、ファイル全体が正しい UTF-8 でない」とし、置換文字を含むテキストとして競合を調べるのは参照先にだけ後ろの不正な UTF-8 があるときだと書き直す。FLAG は未決のまま残す。
  - why: FLAG-cli-059 は、先頭 8192 バイトより後ろにだけ不正な UTF-8 があるファイルを置換文字を含むテキストとして行単位で競合を調べると書いていたが、[REQ-cli-061](../../ir/cli/binary.md#REQ-cli-061) を実装した後は、左右のファイルはバイナリになる。一時的な試験（左右と参照先の三つのサーバに先頭 8,192 バイトが ASCII の "f" を置き、diff f --left local --right develop --ref staging --format json を実行）で、左右の後ろにだけ不正なバイトがあるときは "binary": true で "conflict_count" も "ref_hunks" も出ず、参照先にだけ後ろの不正なバイトがあるときは "conflict_count": 1 と "ref_hunks" が出た。参照先の中身の扱いは後の単位で決めるので、FLAG を閉じずに本文だけを測った動きに合わせた。
  - decided_by: 整合の段（consistency phase）
- A3 中身の同じバイナリは、ファイルやその親のディレクトリを名前で指定した diff でも、パスを指定しない diff でも項目に出さない（[REQ-cli-009](../../ir/cli/binary.md#REQ-cli-009)、[EX-cli-018](../../ir/cli/binary.md#EX-cli-018)）。IR は変えず、実装とテストを IR に合わせる。
  - why: REQ-cli-009 は利用者が承認した [2026-10-05-quality-cli.md の A4](./2026-10-05-quality-cli.md#A4)（D11）で「一致を示す」から「項目を出さない」に書き換えられたが、名前で指定したファイルの一件ずつの比較（`crates/remote-merge/src/cli/diff.rs` の `run_diff_fast_path`）は、それより前の 2026-09-26 のコミット 80376c6 で足した、中身の同じバイナリを比べる対象に戻す処理を残していた。そのテスト `equal_binary_files_report_matching_hashes` も EX-cli-018 の印を持ちながら一致したハッシュの項目が一件出ることを確かめていた。IR の中に食い違いはなく、IR を実現できないことを示す再現もないので、IR が誤りとは言えない。この処理を消す前に、名前で指定したファイルを比べる新しいテスト（`crates/remote-merge/tests/contract/diff_file_kinds.rs` の `ex_cli_018_equal_binaries_have_no_item_with_or_without_a_path`）を `cargo nextest run --all-features -E 'test(ex_cli_018)'` で走らせると、"d/f" を指定したとき、中身が `\0\xffsame` でも、先頭 8,192 バイトが ASCII で後ろにだけ `\xff` があっても（後者は、中身をそれだけに絞って "f" を指定した一時的な試験で確かめた）、"binary": true で left_hash と right_hash の等しい項目が一件出て失敗した。消した後は、名前で指定したファイル・その親のディレクトリ・パスなしの三通りとも項目が出ず、files_with_changes が 0、終了コードが 0 で通った。パスを指定しない diff とディレクトリの展開は、もともと中身の同じものを比べる対象から外していた（`compute_statuses_and_resolve` の `filter_changed_files` と、展開した子の項目を出す条件）。
  - decided_by: 整合の段（consistency phase）
