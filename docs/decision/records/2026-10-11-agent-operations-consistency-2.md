# エージェントの操作と接続断の、IR とコードの二度目の突き合わせで決めたこと

## Context

[2026-10-11-agent-operations-consistency.md](./2026-10-11-agent-operations-consistency.md) の A1〜A3 で直した後のコードを、もう一度 IR と読み比べた。
A1〜A3 の根拠はどれも今のコードとテストで成り立っていて、IR とコードの食い違いは新たに見つからなかった。
ただ、読み比べの途中で、IR と違うように見えて実際には違わない、または起こりえない二つの動きを試しの実行と読み取りで確かめた。
あとで同じ疑いを繰り返さないように、変えないと決めた理由をここに残す。

Position: A1・A2 で出揃った。

## Agreements

- A1 status で、エージェントのまとめてのハッシュが起動し直した後にも `接続断` で失敗したときは、比べるはずだったファイルをそれぞれ、止めたことと最初の `接続断` の原因が分かる理由で errors に出し、終了コード 2 を返す今の動きのままにする。IR もコードも変えない。
  - why: [REQ-ssh-003](../../ir/ssh/reconnect.md#REQ-ssh-003)・[REQ-ssh-023](../../ir/ssh/reconnect.md#REQ-ssh-023) が「まとめに入っていたファイルを理由付きの読み込みの失敗として failed に出す」と決めているのは merge・sync・rollback で、status は「そのコマンドを止めて終了コード 2 を返す」と、読めなかったファイルを理由とともに errors に出す [REQ-cli-088](../../ir/cli/unreadable.md#REQ-cli-088) による。status の結果には「手を付けていない」と見分けるべき別の種類の失敗がなく、文言は [A34](./2026-10-11-agent-operations.md#A34) のとおり IR で決めない。試しの契約テスト（エージェントのあるサーバで "example.txt" と "same.txt" を置き、HashFiles の要求を毎回応答の前に切って `status` を実行する）で、要求は ListTree・HashFiles・HashFiles の三つだけで SSH の経路には回らず、終了コード 2、files は空、errors に二つのファイルがそれぞれ "fixture: not attempted: processing for fixture stopped after the connection was lost (failed to fill whole buffer)" で出た。どちらのファイルも理由と原因付きで errors にあり、要件を満たす。
  - decided_by: 一貫性の段（consistency phase）
- A2 merge の `ReplaceSymlinkWithFile`（`crates/remote-merge/src/service/merge_flow.rs`）の書き込みの失敗に、symlink のバックアップの場所を足さない。
  - why: [REQ-ssh-022](../../ir/ssh/reconnect.md#REQ-ssh-022) は「書きかけ」の error にバックアップの場所を出すと決めていて、この分岐の書き込みは `ConfirmedChange::with_backup` を通していない。ただし `determine_merge_action`（`crates/remote-merge-engine/src/service/merge.rs`）は、読み込み元と書き込み先の両方に項目があって種類が違えば `SkipDifferentKind` を返す（同じファイルのテスト `a_regular_file_does_not_replace_a_symlink`）ので、この分岐に入るのは読み込み元のツリーにそのパスがなく、書き込み先にだけ symlink があるときだけである。そのパスは同じツリーから作る状態で書き込み先にだけあるもの（RightOnly か LeftOnly）になり、`execute_single_merge` の最初の `check_source_exists` が読み込み元にないファイルとして止めるので、CLI からこの書き込みには届かず、利用者に見える違いはない（コードを読んで導いたことで、この分岐に届く試しは作れなかった）。symlink を作る分岐の「書きかけ」は、確かめるまでの間に別の誰かが作成先に項目を作ったときにしか起きず、それを起こす道具も試しもないので、ここでは扱わない。
  - decided_by: 一貫性の段（consistency phase）
