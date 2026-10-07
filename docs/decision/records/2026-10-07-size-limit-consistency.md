# 大きさの上限の単位で IR と実装の食い違いをそろえる

## Context

ファイルの大きさの上限と読めないファイルの扱いの単位（計画 size-limit、[2026-10-05-size-limit.md](./2026-10-05-size-limit.md)）の変更を、IR と突き合わせ直した。
変更したコードの中に、IR が決めていることと違う動きと、IR が書いていない動きが見つかった。
この記録は、それぞれをどちらに合わせたかと、その根拠を残す。

Position: A1 と A2 を反映した。

## Agreements

- A1 status --checksum は、左右とも symlink のパスを中身を読み比べる対象に入れず、メタデータの比較でリンク先の文字列により決めた判定のままにする。コードを [REQ-cli-028](../../ir/cli/status.md#REQ-cli-028) と [EX-cli-090](../../ir/cli/status.md#EX-cli-090) に合わせる。
  - why: REQ-cli-028 は --checksum の有無と経路によらず symlink の中身を読まないと決めているが、--checksum の中身を読む経路（右がローカルの組）は symlink の組も読み比べていた。一時的な試験で、リンク先の文字列が同じで辿った先の中身が違う組は、ハッシュの経路（右がサーバ）で "equal"、中身の経路で "modified" になり、リンク先のない symlink の組は中身の経路だけ errors に出て終了コード 2 になった。IR の部分どうしは矛盾しておらず、IR を実現できないことも示されていないので、IR ではなくコードを直した。直した後は二つの経路とも "equal" で errors はない（`crates/remote-merge/tests/contract/status_judgement.rs` の `checksum_compares_two_symlinks_by_their_target_text_without_reading_the_content`）。
  - decided_by: 整合の段（consistency phase）
- A2 rollback がバックアップの保存先から写しを読むときは大きさの上限を当てないことを [REQ-cli-079](../../ir/cli/size-limit.md#REQ-cli-079) に書く。rollback で上限を当てるのは、復元の前に書き込み先の今の中身をバックアップするための読み込みである。
  - why: 実装は保存先の写しを `backup_store` の `read_record` で上限なしに読む（`crates/remote-merge-engine/src/backup_store.rs` の `read_record`）。これは [2026-10-05-size-limit.md の A7](./2026-10-05-size-limit.md#A7) で利用者が承認した扱いで、REQ-cli-079 の出典の [2026-10-05-quality-cli.md の A2](./2026-10-05-quality-cli.md#A2) も rollback については復元の前のバックアップのための読み込みだけを挙げている。一方で REQ-cli-079 の本文は rollback が書き込みのために中身を読む前に上限を当てるとだけ書いており、写しの読み込みに当たるかが読み分けられなかった。利用者は 104,857,600 バイトを超える写しを --force なしで書き戻せるかで違いを見るので、IR に書く。出典の決定と食い違わない具体化なので、コードは変えない。
  - decided_by: 整合の段（consistency phase）
