# テストのない要件と例にテストを付けた単位の、二度目の IR とコードの突き合わせで決めたこと

## Context

テストのない要件と例に契約テストを付けた単位（計画 untested-items）で、diff の比べ方を IR とそろえた後（[一度目の記録](./2026-10-12-untested-items-consistency.md)）、その修正を含めてもう一度コードを IR と突き合わせた。
"." と一緒に名前で指定したパスを一つずつ比べるようにした修正が、"./" で始まる形の名前で同じファイルを二つの項目にしていた。
バックアップの集約先の準備の失敗で止める要件は、ファイルを保存するときに作る一時的な保存場所の扱いが、保存できないファイルの要件と食い違っていた。
UTF-8 として読めない名前の報告の形を足した要件は、その文を足した決定を出典に挙げていなかった。
この記録は、それぞれをそろえた決定と、その根拠の測定を残す。

Position: 決定は A1〜A3 で出揃い、どれも IR とコードに入れた。利用者の判断を待つものはない。

## Agreements

- A1 diff に "."・"./"・空の値・"/" や glob 文字を含むパスと一緒に名前でパスを指定したときは、名前で指定したパスを、"." の要素と続いた "/" を除いた、root_dir 全体の走査の結果と同じ形にそろえてから一つずつ比べ、そろえて同じになる名前は一つにする。全体を走査しないとき（名前だけを指定したとき）のパスの形は変えない。IR は変えない。
  - why: [REQ-cli-052](../../ir/cli/diff-output.md#REQ-cli-052) は "." があるときに root_dir 全体を比べると決め、[REQ-cli-058](../../ir/cli/diff-output.md#REQ-cli-058) は変更のあるファイルの数を出すと決めている。同じファイルを二つの項目で示して二度数えるのはどちらとも合わない。[一度目の記録の A2](./2026-10-12-untested-items-consistency.md#A2) で "." と名前を一緒に指定したときも名前を一つずつ比べるようにする前（eebdbb0）は、全体の走査だけで比べていたので一度だけ示していた。全体を走査しないときに形をそろえないのは、この単位で利用者から見える形を変えないため。測ったこと（使い捨てのテストで、コミットしていない）: 左右で中身の違う "a.txt" と "b.txt" を置き、`diff . ./a.txt` は eebdbb0 で files が "a.txt" と "b.txt"・files_with_changes 2、d3a8d00 で "./a.txt"・"a.txt"・"b.txt"・files_with_changes 3 だった。SSH とエージェントの経路でも d3a8d00 で "./a.txt" と "a.txt" の二つを出し、修正後は "a.txt" の一つだけを出した。契約テスト `diff_named_files::req_cli_081_a_file_named_beside_the_root_dir_in_another_form_is_shown_once`（"." と "./a.txt" と "./d"）は修正前に 2 件のはずが 4 件で失敗し、修正後に通った。
  - decided_by: 一貫性の確認（consistency phase）
- A2 [REQ-backup-045](../../ir/backup/failure.md#REQ-backup-045) でコマンド全体を書き込む前に止める準備の失敗は、セッション ID の予約、集約先と書き込み先ごとの領域の作成、予約の読み取りとする。ファイルごとに書き換え前の内容を保存するときに作る一時的な保存場所を作れないときは、そのファイルの書き換え前の内容を保存できないものとして [REQ-backup-002](../../ir/backup/storage.md#REQ-backup-002) のとおりそのファイルを書かずに失敗として報告し、他のファイルを続ける。REQ-backup-045 をそう書き改める。コードは変えない。
  - why: REQ-backup-045 は「一時的な保存場所の作成」の失敗でも「どのファイルも書き込む前に」止めるとしていたが、一時的な保存場所はファイルを保存するたびにそのファイルのために作るもので、二つ目以降のファイルのときの失敗は一つ目を書いた後に起きるので、書き込む前に止めることはできない。一方で REQ-backup-002 は、書き換え前の内容を保存できないファイルを書かずに他を続けると決めており、二つの要件がこの失敗で食い違う。REQ-backup-045 の出典の [quality-backup A4](./2026-10-05-quality-backup.md#A4) の理由は「集約先そのものが使えないときは全ファイルが保存できないので、最初に止める」で、ファイルごとの保存の失敗は REQ-backup-002 を変えないとしている。その理由を保ったまま、止める対象を書き込む前に準備する場所にそろえた。[backup-mutant-flags A2](./2026-09-27-backup-mutant-flags.md#A2) は一時的な保存場所の作成の失敗に触れるが、REQ-backup-045 が出典に挙げておらず、扱いを決めないまま残した記録なので、制約としては読まなかった。読んだこと: 書き込む前の準備（`crates/remote-merge-engine/src/backup_store.rs` の `reserve_session` と `prepare_target`）は集約先・予約の場所・書き込み先ごとの領域・その説明・記録の場所を作り、失敗すればコマンド全体を止める。一時的な保存場所（記録の場所の隣の "pending" とその中の項目）は `save` がファイルごとに作り、その失敗は `crates/remote-merge/src/service/merge_flow.rs` と `crates/remote-merge/src/runtime/side_io.rs` でそのファイルの "backup failed" になる。ファイルごとの保存の失敗を "backup failed" とする扱いは 2026-09-14 の 6941573 からある。一時的な保存場所だけを作れない状況を作る手段がなく（記録の場所は同じ親に直前に作れている）、測ってはいない。
  - decided_by: 一貫性の確認（consistency phase）
- A3 [REQ-scan-012](../../ir/scan/listing.md#REQ-scan-012) の出典に、報告するパスの形の文を足した [一度目の記録の A4](./2026-10-12-untested-items-consistency.md#A4) を加える。
  - why: 一度目の記録の A4 は REQ-scan-012 に報告するパスの形を書き足したが、REQ-scan-012 の出典はその表し方を決めた [実装者の A5](./2026-10-12-untested-items-implement.md#A5) だけを挙げ、文を IR に入れた決定を挙げていなかった。同じ記録の A1 で書き足した [REQ-cli-084](../../ir/cli/diff-strategy.md#REQ-cli-084) は A1 を出典に加えており、それとそろえる。
  - decided_by: 一貫性の確認（consistency phase）
