# エージェントの操作と接続断の、IR とコードの突き合わせで決めたこと

## Context

[2026-10-11-agent-operations.md](./2026-10-11-agent-operations.md) の A1〜A41 を実装した後、変えたコードを IR と読み比べた。
変更の最中の `接続断` の後の確かめと、バックアップが無効な rollback のまとめてのハッシュで、IR に書かれていない場合と、IR と違う動きが見つかった。
ここでは、試しの実行で確かめた事実をもとに、IR とコードのどちらをどう合わせたかを決める。

Position: A1〜A2 まで決めた。

## Agreements

- A1 「書けた」「消せた」の後にそのファイルの変更が残っているとき（新しいファイルや作ったディレクトリの権限の変更、--with-permissions の権限の複製、symlink を置き換えるために消した後の作成）は、残りの変更を行わず、そのファイルを、済んだ変更と手を付けていない変更の両方が分かる error で failed に出す。[REQ-ssh-022](../../ir/ssh/reconnect.md#REQ-ssh-022) にこの文を足し、EX-ssh-054 を足した。コードは変えない。
  - why: REQ-ssh-022 の「書けたファイルは済んだファイルとして結果に出す」と「確かめた後もその回の処理は止める」は、一つのファイルに変更が二つ以上ある場合にぶつかり、IR はどちらを取るかを書いていなかった。残りの変更を行うと [A38](./2026-10-11-agent-operations.md#A38) の「確かめた後も止める」に反し、済んだ結果に出すと権限を付けていないファイルが成功に見える。中身を書いた後の権限の変更の失敗を「中身は書いた」と分かる error で failed に出す [REQ-merge-037](../../ir/merge/permissions.md#REQ-merge-037) と A38 の形にそろえた。試しの契約テスト（書き込み先にない "a.txt" と、ある "b.txt" を merge し、エージェントが "a.txt" を書いた後で応答の前に切る）で、今のコードは "a.txt" を "content written, but not attempted: processing for fixture stopped after the connection was lost (...)"、"b.txt" を "not attempted: ..." で failed に出し、終了コード 2、"a.txt" の権限は既定の権限にならず umask のまま（664）だった。これはこの決定と同じ動きである。
  - decided_by: 一貫性の段（consistency phase）
- A2 バックアップが無効な rollback が書き戻す前にまとめてハッシュを取る処理（[A40](./2026-10-11-agent-operations.md#A40)・[A41](./2026-10-11-agent-operations.md#A41)。SSH でハッシュのコマンドがないときに代わりにまとめて読む処理を含む）は、[REQ-ssh-003](../../ir/ssh/reconnect.md#REQ-ssh-003)・[REQ-ssh-023](../../ir/ssh/reconnect.md#REQ-ssh-023) の「書き込み先の中身をまとめて読む処理」に入る。やり直しても `接続断` で止めたときは、そのまとめに入っていたファイル（書き戻す先にある通常ファイル）を理由付きの読み込みの失敗に、入っていなかった残りを手を付けていない失敗にする。二つの要件の文にこのことを書き足し、コードをこれに合わせた。
  - why: ハッシュを取るには中身を読むので、まとめて読む処理と同じ扱いにするのが REQ-ssh-003 の読み取りの範囲（ハッシュを含む）と [A37](./2026-10-11-agent-operations.md#A37) に合う。試しの契約テスト（エージェントを使わないサーバとエージェントを使うサーバのそれぞれで、"a.txt" は書き戻す先にあり "b.txt" はない rollback を、バックアップを無効にして、ハッシュのコマンドか HashFiles の要求を毎回切って行う）で、直す前のコードは SSH で "a.txt"・"b.txt" とも "not attempted: SSH connection failed to fixture: Remote command ended without an exit status"、エージェントで両方とも "not attempted: failed to fill whole buffer" を出した。まとめに入っていた "a.txt" まで手を付けていない失敗になり、IR の区別と違っていた。直した後は "a.txt" が "read failed: the connection was lost: ..."、"b.txt" が "not attempted: ..." になる（`crates/remote-merge/src/runtime/side_io.rs` の `restore_backup`）。
  - decided_by: 一貫性の段（consistency phase）
