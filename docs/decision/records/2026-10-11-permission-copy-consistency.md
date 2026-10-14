# 権限の写しの直しで、作ったディレクトリの権限の失敗とエージェントの中の権限付けを IR に足さない

## Context

[2026-10-11-permission-copy](./2026-10-11-permission-copy.md) の直しで、merge の symlink の分岐も作ったディレクトリに設定の権限を付けるようになり、その失敗を failed に出すようになった。あわせて、エージェントの中で作ったディレクトリに権限を付ける処理をやめ、どの経路でも `crates/remote-merge/src/service/merge_flow.rs` の `set_directory_permissions` が付けるようにした。
この二つを IR に書き足すかを、IR と実装を突き合わせて決める。

Position: 決定は A1・A2 で出揃った。

## Agreements

- A1 merge と sync が作ったディレクトリの権限を変えられなかったとき（`接続断` でない失敗）は、symlink を作ったときも含めて、そのファイルを failed に出し、終了コード 2 を返す。error はファイルなら中身は書いたこと、symlink ならリンクは作ったことが分かるものにし、文言は IR で決めない。IR は変えない。
  - why: [A3](./2026-10-11-permission-copy.md#A3) は IR を変えずにこの扱いを決めている。failed に出すことと終了コード 2 は、[REQ-merge-012](../../ir/merge/permissions.md#REQ-merge-012)（作ったディレクトリを設定の権限にする）、[REQ-ssh-022](../../ir/ssh/reconnect.md#REQ-ssh-022) の「`接続断` でない失敗は、そのファイルだけを failed に出して残りを続ける」、[REQ-cli-047](../../ir/cli/merge.md#REQ-cli-047)（failed があれば 2）から読める。`接続断` のときの「済んだ変更と手を付けていない変更の両方が分かる error」も REQ-ssh-022 の「作ったディレクトリの権限の変更」に入っている。symlink の分岐の「リンクは作った」は A3 の「中身は書いた」と同じ役目の言い換えで、文言を IR で決めないのは [REQ-merge-037](../../ir/merge/permissions.md#REQ-merge-037) と同じ扱いにそろえた。使い捨ての試し（SSH の経路で Chmod を拒む故障を入れ、新しい "linked/new" の下の symlink "linked/new/link" を merge する契約テスト。コミットしていない）で、終了コード 2、`"error": "link created, but failed to change permissions of \"linked\" to 775: ..."`、リンクは "x.txt" を指して作られていた。
  - decided_by: 一貫性の段（consistency phase）
- A2 エージェントが書き込みと symlink の作成の中で作ったディレクトリに権限を付けるのをやめ、merge と sync が書いた後に付ける形にしたことは IR に書かない。
  - why: 利用者から見える結果（作られたディレクトリの権限、setgid を残すこと、失敗のときの failed）は、どの経路でも変わらない。三つの経路で作ったディレクトリの権限を比べる契約テスト（`crates/remote-merge/tests/contract/merge_created_directory_routes.rs` の EX-merge-052 と symlink のテスト）が通っている。どこで権限を付けるかは実装の手順で、IR に入れるかの基準（使う人に違いが見えるか）に当たらない。IR にエージェントの中の手順を書いた文はない（`rg -n "dir_permissions|ディレクトリの権限" docs/ir` で確かめた）。
  - decided_by: 一貫性の段（consistency phase）
