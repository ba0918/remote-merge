# 大きさの上限の単位で IR と実装の食い違いをそろえる

## Context

ファイルの大きさの上限と読めないファイルの扱いの単位（計画 size-limit、[2026-10-05-size-limit.md](./2026-10-05-size-limit.md)）の変更を、IR と突き合わせ直した。
変更したコードの中に、IR が決めていることと違う動きと、IR が書いていない動きが見つかった。
この記録は、それぞれをどちらに合わせたかと、その根拠を残す。

Position: A1 から A4 を反映した。

## Agreements

- A1 status --checksum は、左右とも symlink のパスを中身を読み比べる対象に入れず、メタデータの比較でリンク先の文字列により決めた判定のままにする。コードを [REQ-cli-028](../../ir/cli/status.md#REQ-cli-028) と [EX-cli-090](../../ir/cli/status.md#EX-cli-090) に合わせる。
  - why: REQ-cli-028 は --checksum の有無と経路によらず symlink の中身を読まないと決めているが、--checksum の中身を読む経路（右がローカルの組）は symlink の組も読み比べていた。一時的な試験で、リンク先の文字列が同じで辿った先の中身が違う組は、ハッシュの経路（右がサーバ）で "equal"、中身の経路で "modified" になり、リンク先のない symlink の組は中身の経路だけ errors に出て終了コード 2 になった。IR の部分どうしは矛盾しておらず、IR を実現できないことも示されていないので、IR ではなくコードを直した。直した後は二つの経路とも "equal" で errors はない（`crates/remote-merge/tests/contract/status_judgement.rs` の `checksum_compares_two_symlinks_by_their_target_text_without_reading_the_content`）。
  - decided_by: 整合の段（consistency phase）
- A2 rollback がバックアップの保存先から写しを読むときは大きさの上限を当てないことを [REQ-cli-079](../../ir/cli/size-limit.md#REQ-cli-079) に書く。rollback で上限を当てるのは、復元の前に書き込み先の今の中身をバックアップするための読み込みである。
  - why: 実装は保存先の写しを `backup_store` の `read_record` で上限なしに読む（`crates/remote-merge-engine/src/backup_store.rs` の `read_record`）。これは [2026-10-05-size-limit.md の A7](./2026-10-05-size-limit.md#A7) で利用者が承認した扱いで、REQ-cli-079 の出典の [2026-10-05-quality-cli.md の A2](./2026-10-05-quality-cli.md#A2) も rollback については復元の前のバックアップのための読み込みだけを挙げている。一方で REQ-cli-079 の本文は rollback が書き込みのために中身を読む前に上限を当てるとだけ書いており、写しの読み込みに当たるかが読み分けられなかった。利用者は 104,857,600 バイトを超える写しを --force なしで書き戻せるかで違いを見るので、IR に書く。出典の決定と食い違わない具体化なので、コードは変えない。
  - decided_by: 整合の段（consistency phase）
- A3 diff の --ref で、テキストとして比べる左右の通常ファイルに対して参照先の同じパスが末尾の symlink かディレクトリのときは、参照先を読まずに "ref" だけを出し、errors にも終了コードにも数えないようにコードを [REQ-cli-090](../../ir/cli/unreadable.md#REQ-cli-090) と [REQ-cli-063](../../ir/cli/reference.md#REQ-cli-063) に合わせる。種類は参照先の経路のパスの確認（末尾の symlink か）と子の一覧（ディレクトリか）で決め、確かめられないときは読みに進めてその読み込みの失敗を errors に出す。
  - why: 利用者が決めた [2026-10-05-size-limit.md の A33](./2026-10-05-size-limit.md#A33) を IR に書いた後も、コードは参照先を読みに行っていた。直す前の契約テスト（`crates/remote-merge/tests/contract/diff_ref_kinds.rs`）で、SSH の参照先がディレクトリのとき errors に "staging: SSH command execution failed: openssl base64 -in '…/staging/file.txt' && test ! -d '…/staging/file.txt': exit=1" が載り、参照先が末尾の symlink のときはリンクを辿った先の中身で "ref_hunks" が出た。IR の部分どうしは矛盾せず、IR を実現できないことも示されていないので、コードを直した。直した後は SSH とローカルの参照先のどちらでも "ref" だけが出て errors はなく終了コード 1 になる（`cargo nextest run --all-features -E 'test(/diff_ref_kinds/)'`）。左右の側でディレクトリかを子の一覧で決めるのと同じ手段を使い、経路ごとの判定を足さない。サーバの子の一覧はエージェントの有無によらず SSH の `list_dir_strict` で取る（`crates/remote-merge/src/runtime/remote_io.rs` の `fetch_remote_children`）ので、エージェントのある経路でも通常ファイルとディレクトリの見分け方は SSH と同じになる。
  - decided_by: 整合の段（consistency phase）
- A4 [FLAG-cli-070](../../ir/cli/FLAGS.md#FLAG-cli-070) の本文を A3 の後の動きに合わせ、参照先の同じパスそのものが symlink のときは読まないので外の内容が出ないこと、途中のディレクトリが root_dir の外を指す symlink のときは今も外のファイルの内容が "ref_hunks" に出ることに書き直す。FLAG は未決のまま残す。
  - why: FLAG-cli-070 は参照先の項目が root_dir の外を指す symlink のとき内容が出うるとしていたが、REQ-cli-090 は種類の違う参照先（末尾の symlink）を読まないとしており、IR の中で食い違っていた。一時的な試験（SSH の参照先 staging の "top.txt" を外のファイルへの symlink、"sub" を外のディレクトリへの symlink にして diff --ref staging を実行）で、"top.txt" には "ref" だけが出て "ref_hunks" はなく、"sub/file.txt" には外のファイルの "SECRET" の行が "ref_hunks" に出た。残る場合の扱いは利用者がまだ決めていないので、FLAG を閉じずに本文だけを測った動きに合わせた。
  - decided_by: 整合の段（consistency phase）
