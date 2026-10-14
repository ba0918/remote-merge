# エージェントの操作と接続断の、IR とコードの三度目の突き合わせで決めたこと

## Context

ディレクトリの権限の変更で setuid・setgid を残すと決めた [A42](./2026-10-11-agent-operations.md#A42) と、その後の直しのコードを、IR と読み比べた。
`接続断` で止めた rollback --dry-run の終了コード、権限の変更の確かめで親の一覧が打ち切られたときの扱い、作ったディレクトリの setgid の三つで、IR とコード、または IR の中の要件どうしが合っていなかった。
どれも試しの実行か既存の決定で決められたので、ここに決めたことと根拠を残す。

Position: A1〜A3 で出揃った。

## Agreements

- A1 rollback --dry-run でも、`接続断` で書き込み先への処理を止めたときは終了コード 2 を返す。止めなかった --dry-run は今までどおり 0 を返す。
  - why: [REQ-backup-038](../../ir/backup/rollback-cli.md#REQ-backup-038) は「--dry-run は 0 を返す」とし、[REQ-ssh-003](../../ir/ssh/reconnect.md#REQ-ssh-003)・[REQ-ssh-023](../../ir/ssh/reconnect.md#REQ-ssh-023) は「止めたら終了コード 2」としていて、IR の中でぶつかっていた。REQ-backup-038 の 0 は旧資料と実装の一致をそのまま仕様にしたもの（[adopt-backup A5](./2026-09-27-adopt-backup.md#A5)）で、失敗の場合を決めたものではない。後の [A32](./2026-10-11-agent-operations.md#A32) は「接続断で処理を止めたときは、どのコマンドでも終了コード 2」と決めていて、dry-run を除いていない。集約先の場所が決まらない rollback が --dry-run でもエラーで終わる [REQ-backup-045](../../ir/backup/failure.md#REQ-backup-045) も、0 が完了した予定の表示のためのものであることと合う。試しの契約テスト（"a.txt" を merge した後、rollback --target fixture --dry-run のパスの確認を毎回切る）で、エージェントの経路も SSH の経路も、failed に "read failed: the connection was lost: ..." を出して終了コード 0 だった。`crates/remote-merge/src/cli/rollback.rs` が dry-run を `has_stopped` より先に見て 0 にしていたので、止めたかを先に見るようにした。--list は確かめていない（`connect_if_remote` の前に終わるので、ここでは扱わない）。
  - decided_by: 一貫性の段（consistency phase）
- A2 merge が作った新しいディレクトリが親から受け継いだ setuid・setgid は、どの経路でも残し、0o777 の範囲のビットを設定された権限にする。[REQ-merge-012](../../ir/merge/permissions.md#REQ-merge-012) に書き足し、例 EX-merge-052 を足す。コードは変えない。
  - why: [A42](./2026-10-11-agent-operations.md#A42) は、merge が作ったディレクトリを設定の権限に変えるときに setgid の親の下で経路によって差が出ることを理由に、残す側にそろえると決めた。一方で REQ-merge-012 は「設定された権限で作る」とだけ書き、設定できる権限は 0o777 までなので（[config/values.md](../../ir/config/values.md)）、受け継いだ setgid を残すかを IR が書いていなかった。エージェントの書き込みで作るディレクトリにも同じ規則を当てた直し（`crates/remote-merge-agent/src/agent/file_io.rs` の `apply_dir_permissions`）と、三つの経路で 0o2775 になることを確かめる契約テスト（`crates/remote-merge/tests/contract/merge_created_directory_routes.rs`）は、A42 の決めたとおりの動きである。
  - decided_by: 一貫性の段（consistency phase）
- A3 権限の変更の最中の `接続断` の後、権限を確かめるために読む親の子の一覧は、どの経路でも件数で打ち切らない。そのパスが一覧にないときは「ない」とする。
  - why: [REQ-ssh-022](../../ir/ssh/reconnect.md#REQ-ssh-022) と [A38](./2026-10-11-agent-operations.md#A38) は「結果が分からない」をつなぎ直しか確かめる読み込みが失敗したときだけにしている。前のコードは親の一覧を 10,000 件で打ち切り、その一覧にパスがないと、読み込みは成功していても「結果が分からない」にしていた。試しの契約テスト（SSH の経路で、書き込み先の "a" に 20,000 の子を置き、"a/new/x.txt" を merge して "a/new" の権限の変更を切る）で、error は "content written, but result unknown: ... (\"new\" was not in the listing of its parent, which stopped at 10000 entries) ..." だった。直した後は "content written, but not attempted: ..." になる。一つのパスだけを読む要求をエージェントに足すと通信の版を上げることになるので、今ある一覧を打ち切らずに使う。SSH の経路は今も find の出力を全部受け取ってから打ち切っていたので、読む量は変わらない。読むのは `接続断` の後だけなので、何も起きないときの往復は増えない。エージェントの経路の試しは、子の並びがディレクトリのハッシュの順で決まらず、上限の外に入るようにできなかったので、契約テストを置いていない。
  - decided_by: 一貫性の段（consistency phase）
