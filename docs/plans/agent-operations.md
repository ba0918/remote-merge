# Plan: エージェントに削除・権限の変更・子の一覧を足し、接続断でその書き込み先への処理を止める

## Goal

リモートの削除・権限の変更・子の一覧が、sudo の有無によらずエージェントで行われ、三つの経路で同じ結果になる。版の違うエージェントは使われない。接続断が起きると、結果の分からない変更は繰り返されず、その書き込み先への処理が止まって、どこまで済んだかが failed に読める形で出る。

## Specification

IR は `docs/ir/` にある。決定は `docs/decision/records/2026-10-11-agent-operations.md` の A1〜A37。この計画が扱うのは次のとおり。

- 経路の選び方: `docs/ir/ssh/fallback.md#REQ-ssh-005`、`#REQ-ssh-010`、`#REQ-ssh-021`
- 接続断: `docs/ir/ssh/reconnect.md#REQ-ssh-003`、`#REQ-ssh-004`、`#REQ-ssh-022`、`#REQ-ssh-023`、`docs/ir/ssh/sudo.md#REQ-ssh-009`
- 三つの操作: `docs/ir/paths/operations.md#REQ-paths-013`、`#REQ-paths-015`、`#REQ-paths-016`、`#REQ-paths-017`
- diff の一覧の失敗: `docs/ir/cli/directory-paths.md#REQ-cli-096`
- 権限の複製の失敗: `docs/ir/merge/permissions.md#REQ-merge-037`
- 接続断の例外を書き足した既存の要件（この計画で振る舞いを変えるのは接続断のときだけ）: `docs/ir/merge/concurrency.md#REQ-merge-033`、`#REQ-merge-041`、`docs/ir/merge/deletion.md#REQ-merge-034`、`docs/ir/cli/unreadable.md#REQ-cli-088`、`#REQ-cli-089`、`docs/ir/backup/failure.md#REQ-backup-017`、`docs/ir/backup/rollback-path.md#REQ-backup-027`
- 用語: `docs/ir/CONTEXT.md` の「接続断」
- IR に書かない決定で、この計画が実装するもの: A12・A28・A30（三つの操作の ".." と絶対パスの拒否）、A18（通信の版を 6 に上げる）

## Approach and why

依存の根から順に作る。テストで接続断や版の違いを起こす道具がないと、どの要件の契約テストも書けないので、最初にテストの道具を足す。次に通信の版と互換の分岐の削除、三つの操作、経路の選び方、接続断の分類とやり直し、merge・sync・rollback の止め方、diff の順にする。

テストの道具。kotowari が数える印は `crates/remote-merge/tests/contract/**`、`crates/remote-merge/tests/cli_diff.rs`、`tests/container-e2e/tests/sudo.rs` にだけ付けられる（`.kotowari/config.yaml`）。`crates/remote-merge/src/runtime/scripted_agent.rs` は `cfg(test)` の単体テスト用なので、契約テストからは使えない。そこで、契約テストの SSH の偽サーバ `crates/remote-merge/tests/contract/ssh_server.rs` の `TestServer` が持つエージェントの中継（" agent --root " を含む exec を本物のエージェントのプロセスにつなぐ部分）を広げる。中継は Rust のプロセス内で動き、protocol の crate でフレームを読めるので、次の三つを足す。

- 起動の挨拶（handshake）の通信の版を書き換える
- エージェントとの間を切る。SSH の session は残したまま、エージェントのチャネルとプロセスだけを終える切り方と、session ごと切る切り方の両方を持つ。切る時点は三つ選べる。決めた種類の要求（例: WriteFile、ListDir）のフレームをエージェントに渡した後で応答を返す前、N 番目の要求の応答を中継した後（次の要求を送る前に切れている状態。A24）、起動し直した後も含めて毎回（初回だけと毎回の区別。既存の interrupt_first_* と interrupt_all_* と同じ形）
- sandbox_home を持つ偽サーバ（本物のシェルでコマンドを実行する種類）でも、SSH のコマンドを種類（find の走査・stat・パスの確認・子の一覧・ハッシュ・読み込み・書き込み）と回数（初回だけ・毎回）で選んで、session を切る。今の切断の道具は sandbox_home のない偽サーバでしか働かない
- sudo を偽る。`sudo -n true` に 0 を返し、`sudo ` で始まるコマンドは前置きを外して同じ利用者で実行する

本物の root の権限が要る例（EX-ssh-034 の root の持つファイル）だけは、Docker の `tests/container-e2e/tests/sudo.rs` で確かめる。

通信の版（A2・A18）。`crates/remote-merge-protocol/src/lib.rs` の `define_protocol_version!` を 6 に上げ、`check_protocol_version` は手元と同じ版だけを受け入れる。古い版のための分岐はすべて消す。

- `PATH_RESOLUTION_PROTOCOL_VERSION` と `crates/remote-merge/src/runtime/path_route.rs` の版の分岐（`RedeployAgent`）
- `crates/remote-merge-agent/src/agent/client.rs` の `hash_files` の版 3 の確認
- 応答と要求の serde の既定値四つ（`ListTree.include`、`TreeChunk.truncated`、`FileContents.is_last`、`AgentFileEntry.link_is_dir`）

版が違うときの扱いは `crates/remote-merge/src/runtime/core.rs` の `try_start_agent` で決める。sudo=true なら接続のエラー、それ以外は警告して SSH にする。今は起動の失敗を `tracing::debug` だけにしているので、警告が利用者に見えない。

三つの操作（A7・A12〜A17・A23・A28・A31・A36）。エージェントに `Remove`・`Chmod`・`ListDir` の要求と応答を足す。基準は今のローカルの振る舞いで、ローカルと SSH の違う点は決定のとおりに直す。

エージェントの既存のパスの確かめ（`crates/remote-merge-agent/src/agent/file_io.rs` の `validate_path` と `validate_entry_path`）は、root_dir の外を指す途中の symlink を拒むので、A7 に合わない。新しい三つの操作には使わない。".." と絶対パスの拒否は、`crates/remote-merge-engine/src/rel_path.rs` の `check_rel_path` と同じ規則と文言にする。末尾の "/" を除く前の値で判定する（A28）。

子の一覧は、三つの経路とも三つの結果を区別した型で返す（A11・A22・A23）。「ディレクトリではない」（通常ファイル、ディレクトリでないものへの symlink、リンク切れ、ないパス）、「一覧を取れなかった」（読めないディレクトリ。diff は errors に出して続ける）、「止めるエラー」（一覧の中の symlink のリンク先か更新時刻を読めない。REQ-scan-015 と同じく止める）の三つである。S4 の経路の選び方と S7 の diff は、この型に頼る。S3 で型を変えると diff の呼び出しがコンパイルできなくなるので、S3 では今の関数の形を残し、新しい型を返す関数を別に足す。diff を新しい関数に移すのは S7 で行う。
ローカルの一覧の値を直す `apply_metadata`（`crates/remote-merge-engine/src/local/mod.rs`）は、子の一覧と全体の走査の両方が使っている。A15 で決めたのは子の一覧の値なので、変えるのは子の一覧だけにし、全体の走査の値（REQ-merge-036 が使う権限など）は変えない。
エージェントの子の一覧の件数の上限は、今の SSH と同じ定数で打ち切る（P2。打ち切ったことを捨てる件 FLAG-cli-049 は変えない）。

経路の選び方（A1）。`crates/remote-merge/src/runtime/target_io.rs` の `RemoteTargetIo` の `chmod_file`・`remove_file`・`fetch_children` を、エージェントがあればエージェントで行う形にする。エージェントがあるときは SSH で行わない。エージェントが失敗を返したときは、そのファイルの失敗にする。

既存の書き込みと symlink の作成は、エージェントが失敗を返すと、今はエージェントを外して SSH で書き直す。この扱いは P2（routes-7・FLAG-merge-012）のとおり、この計画では変えない。新しい三つの操作だけを「失敗を返したらそのファイルの失敗」にするので、二つの扱いが並ぶ。実装者はこれをそろえない。
新しい三つの操作でエージェントが失敗を返したとき（権限がない、ないパス）は、エージェントを外さない。外すと次のファイルから SSH に回り、A1（エージェントがあればエージェントで行う）が崩れる。だから新しい三つの操作は `with_agent`（どの Err でも SSH への切り替えを返す）を使わず、接続断と失敗の応答を分けて返す別の呼び方にする。子の一覧の「ディレクトリではない」はエラーではなく結果として返す。

接続断（A5・A8・A10・A19・A20・A24・A33・A35）。接続断の判定を純粋関数一つにまとめ、表駆動のテストで固める。区別するのは、エージェントがない、要求を送る前に接続が使えない（A24）、送った後に応答が来ない（A8 の接続断）、相手が失敗を返した、の四つである。

今は判定が二つある。`crates/remote-merge/src/runtime/side_io.rs` の `should_invalidate_agent_error` と、`crates/remote-merge/src/runtime/remote_size.rs` の `is_connection_lost` で、InvalidData の扱いが食い違っている。これを一つにする。SSH の側は、`AppError::SshConnection` が送る前の失敗（チャネルを開けない、認証）と送った後の失敗（終わりの状態が来ない）の両方に使われているので、二つを区別できる形にする。

`with_agent` は、接続断を SSH への切り替え（`None`）に変えない。ハッシュの比較（`RemoteTargetIo::hashes` と `try_hash_compare`）と大きさの問い合わせ（`try_agent_file_sizes` の ConnectionLost）も、今は接続断で別の比べ方か SSH に移るので、同じく接続断を返す形にする。ハッシュの比較を呼ぶ `crates/remote-merge/src/cli/status.rs` にも手が入る。
SSH の読み取りの時間切れ（`AppError::SshTimeout`。子の一覧の 30 秒を含む）は、用語の `接続断`（コマンドの終わりの状態を受け取れなかった）に当たるものとして扱い、一度だけやり直してだめなら止める。A11 の「一覧を取れなかった」には入れない。エージェントの読み込みには今は時間切れの設定がないので、時間切れは判定の表駆動の単体テストでだけ確かめる。
送る前に接続が使えなくなっていた場合（A24）は、要求を書く前にエージェントの通信路が閉じたことを確かめて区別する。`crates/remote-merge-agent/src/agent/ssh_transport.rs` の中継は、チャネルの EOF を受けると抜けるだけなので、閉じたことを記録して読めるようにする。
読み取りは、同じ経路で一度だけやり直す。

- エージェントは、起動し直してやり直す。今は `agent_unavailable` に印があると `try_start_agent` が起動しないので、これを通らない道を作る。SSH の session ごと切れたときは、SSH を再接続してからエージェントを起動し直す。起動の手順（sudo の確認、配置先の確認、版と SHA-256 の照合）をもう一度行うことはかまわない。
- SSH は、`try_reconnect` を REQ-ssh-003 の範囲のすべての読み取りで使う。今は `crates/remote-merge/src/runtime/remote_io.rs` の 3 か所だけで使っている。

どちらも、やり直しがまた失敗したら止める。変更は、どの接続断でも繰り返さない。

止め方と報告（A5・A21・A24〜A26・A37・A22）。接続断かどうかは、エラーを文字列に変える前に判定する。今は `crates/remote-merge/src/cli/merge.rs` の 391 行付近と `crates/remote-merge/src/cli/content_reads.rs` で、文字列に変えている。

ほかにも、型を失う場所がある。

- バックアップの失敗を `anyhow::anyhow!("backup failed: {error}")` で作り直す箇所（`merge_flow.rs` の 79・105・156・384 行付近）
- 削除のループで文字列にする箇所（`merge_flow.rs` の 265・276・298 行付近）
- rollback のファイルごとのループ（`crates/remote-merge/src/runtime/side_io.rs` の `restore_backup`、315・332・368・392 行付近）と、その前の確かめのループが確かめの Err を拒否の結果に変える箇所（279〜294 行付近）

どれも、作り直したり文字列にしたりする前に、接続断かどうかを判定する。判定したら、その書き込み先のループを抜け、残りのファイル（削除の対象を含む）を手を付けていない失敗として failed に並べる。まとめて読む処理（`read_contents`）は、接続断なら 1 ファイルずつ読み直さない。

rollback の止め方を変える主な場所は `side_io.rs` の `restore_backup` である。加えて、`crates/remote-merge/src/cli/rollback.rs` の 122〜130 行付近が全体の Err を全ファイルの同じ失敗に変えているので、ここも「手を付けていない」と区別できる形にする。

failed の項目の形は変えない（欄を足さない）。「結果が分からない」「手を付けていない」「読み込みに失敗した」「中身は書いた」の区別は、error の文言で読めるようにする（A34。文言そのものは実装者が英語で書く）。

status と diff で読み取りを止めたときの出力は、今の size_limit の契約テスト（diff の読み込みが二度切れると、JSON の errors にそのパスが出て終了コード 2）と同じ形にする。止めたパスを理由付きで errors に出し、残りのパスは比べずに終了コード 2 を返す。

hunk の merge（`merge_flow.rs` の `execute_hunk_merge`）は、今はファイルごとの failed にならずにコマンド全体のエラーになる。この計画では変えず、接続断でも今と同じくコマンド全体のエラーで終了コード 2 になればよい（同じ書き込みを繰り返さず、SSH に切り替えないことは S5 で満たす）。

`--with-permissions` の権限の複製（`crates/remote-merge/src/service/merge_flow.rs` の `copy_permissions`）は、今は読み込み元の権限を `& 0o777` で切ってから、範囲の外の値を黙って飛ばし、失敗を warn で捨てている。REQ-merge-037 は実装も印もないので、Result を返す形にし、0o777 を超えるビットがあれば写さずに失敗として返すようにして、ここで作る。

## Scope of change

- テストの道具: `crates/remote-merge/tests/contract/ssh_server.rs`、`crates/remote-merge/tests/contract.rs`、`crates/remote-merge/tests/common/`
- 通信: `crates/remote-merge-protocol/src/lib.rs`、`crates/remote-merge-agent/src/agent/client.rs`、`crates/remote-merge-agent/src/agent/dispatch.rs`、`crates/remote-merge-agent/src/agent/file_io.rs`（新しい操作は別のモジュールに分けてよい）、`crates/remote-merge-agent/src/agent/tree_scan.rs`（版の既定値に頼る部分だけ）、`crates/remote-merge/src/agent/`
- エージェントの通信路: `crates/remote-merge-agent/src/agent/ssh_transport.rs`（閉じたことを記録する）
- 実行時: `crates/remote-merge/src/runtime/`（`core.rs`、`side_io.rs`、`target_io.rs`、`remote_io.rs`、`remote_size.rs`、`path_route.rs`。新しいモジュールを足してよい）
- ローカルと SSH の三つの操作: `crates/remote-merge-engine/src/local_io.rs`、`crates/remote-merge-engine/src/local/mod.rs`、`crates/remote-merge-engine/src/merge/executor.rs`（`validate_path_within_root`。`validated_path` は `local_io.rs` にある）、`crates/remote-merge-ssh/src/client.rs`、`crates/remote-merge-ssh/src/tree_parser.rs`、`crates/remote-merge-core/src/`（一覧の結果の型とエラーの型を置くとき）
- コマンド: `crates/remote-merge/src/cli/merge.rs`、`sync.rs`、`rollback.rs`、`diff.rs`、`status.rs`、`content_reads.rs`、`crates/remote-merge/src/service/merge_flow.rs`、`crates/remote-merge-engine/src/service/`（結果の型）
- 契約テスト: `crates/remote-merge/tests/contract/` の新しいファイルと既存のファイル。sudo の実機の例は `tests/container-e2e/tests/sudo.rs`
- 消してよい・書き換えてよい既存のテスト
  - `crates/remote-merge/tests/contract/path_routes.rs` の EX-paths-021・EX-paths-024 のテスト（IR から消えた ID の印を持つ）
  - `crates/remote-merge-protocol/src/lib.rs` の `check_protocol_version_too_old`・`check_protocol_version_v2_accepted`・`check_protocol_version_newer_agent`・`list_tree_without_include_backward_compat`・`response_file_contents_is_last_default_true`・`old_agent_entries_without_directory_link_metadata_remain_readable`・`protocol_version_is_5`
  - `crates/remote-merge-agent/src/agent/client.rs` の `connect_version_too_old`・`connect_newer_version_accepted`・`connect_handshake_eof`（v2 の文字列）・`hash_files_version_too_old`
  - `crates/remote-merge/tests/agent_ssh_deploy.rs` の版を照合する箇所
  - `crates/remote-merge/src/runtime/side_io.rs` のローカルの ".." の扱いを固定しているテスト（`test_remove_file_path_traversal_rejected`、`test_chmod_file_path_traversal_rejected` ほか。期待する文言が A12 に変わる）
  - `a_lost_agent_connection_invalidates_the_agent_and_asks_over_ssh`（接続断で SSH に回ることを固定している）
- 名前を変えてはいけないテスト: REQ-merge-040・041 の how_to_verify が名前で指している `crates/remote-merge/src/runtime/side_io.rs` の `test_remove_local_file_nonexistent` と `creating_a_local_symlink_where_a_link_a_file_or_a_directory_exists_fails_without_change`（中身を A12 に合わせて直すのはよい）

## Step order and prerequisites

S1 → S2 → S3 → S4 → S5 → S6 → S7 の順にする。

- S1 は、後の全ステップの契約テストが使う道具を足す
- S2 は、エージェントの要求の型を変える S3 の前提になる。版を上げる変更は一回にまとめる
- S3 の一覧の型（「ディレクトリではない」と「取れなかった」）は、S4 の経路の選び方と S7 の diff が使う
- S5 の接続断の分類とやり直しは、S6 の止め方と S7 の一覧の接続断の前提になる
- S6 は S5 の分類を使い、エラーを文字列に変える前に判定する
- S7 は S3 の型と S5 のやり直しを使う

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | なし（テストの道具） | なし |
| S2 | REQ-ssh-005、REQ-ssh-021 | EX-ssh-036、EX-ssh-037、EX-ssh-049（EX-ssh-009・EX-ssh-010 は既存のテストが通り続ける） |
| S3 | REQ-paths-013、REQ-paths-015、REQ-paths-016、REQ-paths-017 | EX-paths-026、EX-paths-027、EX-paths-028、EX-paths-031、EX-paths-032、EX-paths-033、EX-paths-034 |
| S4 | REQ-ssh-010 | EX-ssh-034、EX-ssh-035 |
| S5 | REQ-ssh-003、REQ-ssh-004、REQ-ssh-009、REQ-ssh-023 | EX-ssh-019、EX-ssh-041 |
| S6 | REQ-ssh-022、REQ-merge-037 | EX-ssh-038、EX-ssh-039、EX-ssh-040、EX-ssh-042、EX-ssh-043、EX-ssh-044、EX-ssh-045、EX-ssh-046、EX-ssh-047、EX-ssh-048、EX-merge-044、EX-merge-051 |
| S7 | REQ-cli-096 | EX-cli-095、EX-cli-096 |

接続断の例外を書き足した既存の要件（REQ-merge-033・034・041、REQ-cli-088・089、REQ-backup-017・027）は、それぞれの既存のテストが通り続けることと、S6 の REQ-ssh-022 の例で確かめる。REQ-backup-017・027 の接続断（バックアップのための読み込みの接続断）は、S6 で契約テストを一つ足して確かめる。

「変更前の HEAD で RED」は、振る舞いを変える例にだけ求める。今の振る舞いを守る例（EX-ssh-040 の 1 ファイルだけの失敗、EX-ssh-049 の sudo で起動できないとき、EX-cli-096 の通常ファイル）は、HEAD でも通る回帰の守りとして書き、そのことをテストの近くのコメントではなくステップの報告で示す。

## Left to the implementer

- 新しい型・関数・モジュールの名前と置き場所（判定は純粋関数一か所にまとめ、表駆動のテストで固める）
- テストの道具（S1）の作り方（`TestServer` の新しい起動の種類として足すか、引数で切り替えるか）
- 接続断を区別する型の形（`AppError` に足すか、別の型にするか）。ただし、送る前と送った後を区別でき、文字列に変える前に判定できること
- エラー・警告の文言（A34。英語で、区別すべきことが読めるようにする。テストは文言を固定しない）
- 契約テストのファイルの分け方と、テストの名前

## Stop conditions

- S1 のテストの道具で、要求の種類ごとに session を切ることか、挨拶の版を書き換えることができなかった
- エージェントを起動し直す道が、配置済みのエージェントの再配置や sudo の確認をもう一度求めるなど、REQ-ssh-023 の「一度だけやり直す」を一回の起動し直しで満たせない形になった
- 既存の書き込みと symlink の作成の「エージェントが失敗を返したら SSH で書き直す」動き（P2 で変えないもの）を残すと、REQ-ssh-004 か REQ-ssh-022 を満たせないことが分かった
- SSH の `%T@` が、試験に使うサーバで小数を出さず、REQ-paths-017 の例の一致を作れなかった（FLAG-paths-003 の範囲を超える場合）
- 既存のテストが、この変更と関係のない理由で失敗した（`agent_ssh_deploy` のテストのまれな失敗は一度だけやり直してよい）

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

Docker の実機の sudo の例は `scripts/run-container-e2e.sh` で走らせる（Docker が要るので、人が走らせて結果を確かめる）。

## Out of scope

- エージェントが失敗を返した書き込みを SSH でやり直すかと、エージェントの書き込みが root_dir の外を指す途中の symlink を拒む件（P2、routes-7、FLAG-merge-012）
- 1 ファイルのエラーでエージェント全体を外す件（P2、routes-5）。ただし接続断の扱いは S5 で変わる
- 子の一覧の件数の上限で打ち切られたことを捨てる件（P2、FLAG-cli-049）
- SSH の一覧と走査の名前に TAB・改行を含む子（FLAG-paths-001）、子の一覧の exclude の当て方（FLAG-paths-002）、古い find の更新時刻（FLAG-paths-003）
- REQ-merge-041 の確かめ方の書き直し（FLAG-merge-020）
- リモートを読み込み元にした --with-permissions（REQ-merge-036 の未実装の部分）
- 設定・バックアップ・CLI・コードの中の互換の分岐を捨てること（A3、次の単位）

## Steps

### S1: 契約テストで接続断・版の違い・sudo を起こせる道具を足す

- Purpose: 後のステップの契約テストが、エージェントの要求の途中の切断、挨拶の版の違い、sudo のサーバを、本物のエージェントのプロセスで起こせるようにする
- Specification: docs/decision/records/2026-10-11-agent-operations.md#A5, docs/decision/records/2026-10-11-agent-operations.md#A2（どちらも試すための道具で、要件は足さない）
- Prerequisites: none
- May change: `crates/remote-merge/tests/contract/ssh_server.rs`, `crates/remote-merge/tests/contract.rs`, `crates/remote-merge/tests/common/`
- Done when: `TestServer` に次の起動の種類か設定がある。エージェントの中継で、挨拶の通信の版を決めた値に書き換える。エージェントとの間を、SSH の session を残してエージェントだけ終える切り方と session ごと切る切り方で、決めた種類の要求をエージェントに渡した後で応答の前に、または N 番目の応答を中継した後に、初回だけか毎回かを選んで切る。sandbox_home を持つ偽サーバで、SSH のコマンドを種類と回数で選んで session を切る。`sudo -n true` に 0 を返し、`sudo ` で始まるコマンドを前置きを外して同じ利用者で実行する。既存の契約テストはすべて変わらず通る
- Shown by: check — `systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features` がすべて通る。新しい道具は S2 以降の契約テストが使って確かめる
- Left to the implementer: 道具の API の形（起動の種類を足すか、設定の構造体を足すか）
- Stop and hand back if: 中継でフレームの境目を読めず、要求の種類ごとに切れなかった。SSH の session を残したままエージェントだけを終えられなかった。sandbox_home の確かめ（すべてのコマンドが home のパスを含む）と sudo を偽る扱いが両立しなかった

### S2: 通信の版を 6 にして、版の違うエージェントを使わない

- Purpose: 手元と同じ通信の版のエージェントだけを使い、古い版のための分岐をすべて消す
- Specification: `docs/ir/ssh/fallback.md#REQ-ssh-021`, `docs/ir/ssh/fallback.md#REQ-ssh-005`
- Prerequisites: S1
- May change: `crates/remote-merge-protocol/src/lib.rs`, `crates/remote-merge-agent/src/agent/client.rs`, `crates/remote-merge-agent/src/agent/tree_scan.rs`, `crates/remote-merge/src/agent/`, `crates/remote-merge/src/runtime/core.rs`, `crates/remote-merge/src/runtime/path_route.rs`, `crates/remote-merge/src/runtime/target_io.rs`, `crates/remote-merge/src/runtime/side_io.rs`, `crates/remote-merge/tests/contract/path_routes.rs`, `crates/remote-merge/tests/contract/`, `crates/remote-merge/tests/agent_ssh_deploy.rs`
- Done when: 通信の版は 6。`check_protocol_version` は手元と同じ版だけを受け入れる。`PATH_RESOLUTION_PROTOCOL_VERSION`・`RedeployAgent` の分岐・`hash_files` の版の確認・serde の既定値四つがない。挨拶で違う版を答えるエージェントに対しては、次のとおりになる。sudo=true のサーバの merge は、ファイルを読み書きする前に接続のエラーで止まり、終了コード 2 を返し、SSH の経路で読み書きしない。sudo=true でないサーバの diff と merge は、版が違う理由を含む警告を標準エラーに出し、SSH の経路で差分を出して書き込む。sudo=true でエージェントを起動できないサーバの merge も、同じく接続のエラーで止まる。`path_routes.rs` に、IR から消えた ID の印は残らない
- Shown by: test — 契約テスト `// @kotowari[REQ-ssh-021, EX-ssh-036]`（sudo=true、偽の sudo と版を書き換えた挨拶）、`// @kotowari[EX-ssh-037]`（sudo=true でない、警告と SSH での書き込み）、`// @kotowari[REQ-ssh-005, EX-ssh-049]`（sudo=true、起動できないエージェント）。版の受け入れの単体テスト（同じ版だけ受け入れる）。EX-ssh-036・EX-ssh-037 は変更前の HEAD で RED になることを確かめる。EX-ssh-049 は今の動きを守る回帰のテストなので HEAD でも通る
- Left to the implementer: 版の違いを `AgentClient::connect` で判定するか `try_start_agent` で判定するか（どちらでも、読み書きの前に止まり、警告は利用者に見えること）
- Stop and hand back if: 版を書き換えた挨拶を受けたとき、配置の確認（版の文字列と SHA-256）が先に置き直しを始めてしまい、版の確認まで届かなかった

### S3: 削除・権限の変更・子の一覧を、三つの経路で同じ結果にする

- Purpose: エージェントに Remove・Chmod・ListDir を足し、ローカルと SSH の違う点を決定のとおりに直して、三つの経路の結果をそろえる
- Specification: `docs/ir/paths/operations.md#REQ-paths-013`, `docs/ir/paths/operations.md#REQ-paths-015`, `docs/ir/paths/operations.md#REQ-paths-016`, `docs/ir/paths/operations.md#REQ-paths-017`
- Prerequisites: S2
- May change: `crates/remote-merge-protocol/src/lib.rs`, `crates/remote-merge-agent/src/agent/`, `crates/remote-merge/src/agent/`, `crates/remote-merge-engine/src/local_io.rs`, `crates/remote-merge-engine/src/local/mod.rs`, `crates/remote-merge-engine/src/merge/executor.rs`, `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge-ssh/src/tree_parser.rs`, `crates/remote-merge-core/src/`, `crates/remote-merge/src/runtime/`, `crates/remote-merge/tests/contract/`
- Done when: 同じ一時ディレクトリに対して、ローカル・SSH・エージェントの経路で三つの操作を行うと、次のとおりになる。末尾の symlink の削除はリンクだけを消す。ディレクトリとないパスの削除は失敗して何も変えない。root_dir の外を指す途中の symlink の先の権限の変更は成功する。0o777 を超える値の権限の変更はエラーで何も変えない。失敗の文言は、操作・パス・原因を含む。子の一覧の各項目は、種類・大きさ・0o7777 の範囲の権限・ナノ秒までの更新時刻・前後の空白を削らないリンク先が一致する。ローカルの経路でリンク先の読み取りの失敗を差し込むと、一覧は "???" を返さずエラーで止まる。三つの経路の子の一覧は、「ディレクトリではない」（通常ファイル、ディレクトリでないものへの symlink、リンク切れ、ないパス）、「一覧を取れなかった」（読めないディレクトリ）、「止めるエラー」（リンク先か更新時刻を読めない）の三つを区別した結果を返す新しい関数を持つ。".." と絶対パスは、どの経路でも A12 の文言で、末尾の "/" を除く前の値で拒否する（A30 のとおり印を付けない単体テストで固める）。エージェントの子の一覧は、exclude を直下の子の root_dir からのパスに当て、件数を SSH と同じ上限で打ち切る。全体の走査の値は変わらない。diff は S3 の後も今の関数で動く
- Shown by: test — 三つの経路を同じディレクトリで比べる契約テスト（`crates/remote-merge/tests/contract/path_routes_agree.rs` の EX-paths-019 の形）`// @kotowari[REQ-paths-013, EX-paths-026]`、`// @kotowari[EX-paths-027]`、`// @kotowari[EX-paths-028]`、`// @kotowari[REQ-paths-015, EX-paths-031]`、`// @kotowari[REQ-paths-016, EX-paths-032]`、`// @kotowari[REQ-paths-017, EX-paths-033]`（exclude の当て方も同じテストで確かめる）、`// @kotowari[EX-paths-034]`（ローカルの一覧の関数にリンク先の読み取りを差し込める継ぎ目を作り、契約テストから失敗を差し込む）。印のない単体テストで、".." と絶対パスと "/" の拒否（A12・A28）、一覧の結果の型の分類、SSH の `%T@` を文字列のまま読むこと、件数の上限。エージェントの経路は操作がまだないので、どの契約テストも変更前の HEAD で RED になることを確かめる
- Left to the implementer: エージェントの新しい操作のパスの辿り方の実装（既存の `inspect_path` の辿り方を使うか、別に書くか。root_dir の外を指す途中の symlink を辿ること）。一覧の結果の型の形
- Stop and hand back if: ローカルと SSH で、決定（A31）が同じとした点が実際には違った。リンク先の読み取りの失敗を差し込む継ぎ目を、製品のふるまいを変えずに作れなかった。SSH の find の `%m` が特別なビットを出さなかった。子の一覧の値だけを直すことができず、全体の走査の値まで変わってしまった

### S4: エージェントがあれば三つの操作をエージェントで行う

- Purpose: `RemoteTargetIo` の削除・権限の変更・子の一覧を、sudo の有無によらずエージェントで行い、エージェントがあるときは SSH で行わない
- Specification: `docs/ir/ssh/fallback.md#REQ-ssh-010`
- Prerequisites: S3
- May change: `crates/remote-merge/src/runtime/target_io.rs`, `crates/remote-merge/src/runtime/side_io.rs`, `crates/remote-merge/src/runtime/remote_io.rs`, `crates/remote-merge/src/runtime/`（新しい呼び方のモジュール）, `crates/remote-merge/tests/contract/`, `tests/container-e2e/tests/sudo.rs`
- Done when: エージェントを先に置いたサーバで、新しいファイルの merge・--delete の sync・ディレクトリを指定した diff を行うと、権限の変更・削除・子の一覧はエージェントで行われ、SSH の偽サーバが受けたコマンドのうち root_dir のパスを含むものに chmod・rm・find（一覧）が現れない（エージェントの配置のコマンドは数えない）。エージェントが失敗を返した三つの操作は、そのファイルの失敗になり、エージェントは外れず、次のファイルもエージェントで行われ、SSH で行わない。エージェントがないサーバでは SSH で行う（REQ-ssh-005）。Docker の sudo の環境で、書き込み先にだけある root の持つ "old.txt" が --delete の sync で消える（結果だけを確かめる）
- Shown by: test — 契約テスト `// @kotowari[REQ-ssh-010, EX-ssh-035]`（受けたコマンドを確かめる）。`tests/container-e2e/tests/sudo.rs` に `// @kotowari[EX-ssh-034]`（`scripts/run-container-e2e.sh` で人が走らせ、結果を確かめる）。どれも変更前の HEAD で RED になることを確かめる（container-e2e は人が確かめる）
- Left to the implementer: none
- Stop and hand back if: sudo のエージェントで消したファイルのバックアップの保存が、通常権限の読み込みに頼っていて、sudo の環境で失敗した

### S5: 接続断を一か所で判定し、読み取りは同じ経路で一度だけやり直す

- Purpose: 接続断の判定を純粋関数一つにまとめ、読み取りは同じ経路で一度だけやり直してだめなら止め、変更は繰り返さず、回の途中で SSH に切り替えない
- Specification: `docs/ir/ssh/reconnect.md#REQ-ssh-003`, `docs/ir/ssh/reconnect.md#REQ-ssh-004`, `docs/ir/ssh/reconnect.md#REQ-ssh-023`, `docs/ir/ssh/sudo.md#REQ-ssh-009`
- Prerequisites: S4
- May change: `crates/remote-merge/src/runtime/`, `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge-ssh/src/error.rs`, `crates/remote-merge-core/src/error.rs`, `crates/remote-merge-agent/src/agent/client.rs`, `crates/remote-merge-agent/src/agent/ssh_transport.rs`, `crates/remote-merge/src/cli/status.rs`, `crates/remote-merge/src/cli/diff.rs`（読み取りを止めたときの errors の出し方だけ）, `crates/remote-merge/tests/contract/`
- Done when: 判定の純粋関数が、エージェントがない・送る前に接続が使えない・送った後に応答が来ない・相手が失敗を返したを区別する（SSH は送る前と送った後を区別できる）。判定は一つだけで、`should_invalidate_agent_error` と `is_connection_lost` の食い違いがない。SSH の走査が一度だけ切れると、再接続して走査をやり直し、やり直した一覧で比べる。REQ-ssh-003 の範囲のどの SSH の読み取りも、一度だけやり直し、また接続断なら止める。sudo=true でないサーバでエージェントの読み込みが一度だけ切れると、エージェントを起動し直して読み込みをやり直し、SSH では読まない。エージェントの書き込みが接続断になると、SSH の経路で書き直さない。ハッシュの比較と大きさの問い合わせの接続断も、別の比べ方や SSH に移らずに、一度やり直してだめなら止める。status と diff で止めたときは、そのパスを理由付きで errors に出し、残りのパスは比べずに終了コード 2 を返す。SSH の時間切れは接続断として扱う。sudo=true のサーバで、エージェントを外さない種類の失敗が起きても、通常権限の SSH に回らない
- Shown by: test — 判定の表駆動の単体テスト（印なし。時間切れ、InvalidData、送る前と送った後の区別をここで確かめる）。契約テスト `// @kotowari[EX-ssh-019]`（sandbox の偽サーバで SSH の走査を初回だけ切る）、`// @kotowari[REQ-ssh-023, EX-ssh-041]`（SSH の session を残してエージェントだけを初回の読み込みで終える）、`// @kotowari[REQ-ssh-004]`（SSH の session を残してエージェントだけを書き込みの要求の後に終え、SSH の偽サーバが書き込みのコマンドを受けない）、`// @kotowari[REQ-ssh-003]`（stat・パスの確認・子の一覧のどれかを毎回切ると止まる）、`// @kotowari[REQ-ssh-009]`（偽の sudo で、エージェントの読み込みの最中にエージェントだけを毎回終え、SSH の偽サーバが通常権限の読み込みのコマンドを受けずに止まる。これは今の動きを守る回帰のテストで HEAD でも通る。A19 の穴であるエージェントを外さない種類の失敗は、契約テストで起こせないので判定の単体テストで確かめる）。EX-ssh-019・EX-ssh-041・REQ-ssh-004 の契約テストは、変更前の HEAD で RED になることを確かめる
- Left to the implementer: エージェントを起動し直す道の作り方（`agent_unavailable` の印との関係）
- Stop and hand back if: SSH の `SshConnection` を送る前と送った後に分けると、既存の接続のエラーの文言か終了コードが変わった。エージェントの通信路が閉じたことを、次の要求を書く前に知る方法がなかった（A24 と A8 を区別できない）。起動し直しで置き直しが起きた、または一度のやり直しで起動し直しが二回になった

### S6: 接続断でその書き込み先への処理を止め、どこまで済んだかを failed に出す

- Purpose: merge・sync・rollback で接続断が起きたら、その書き込み先のループを止め、済んだもの・結果の分からないもの・読み込みに失敗したもの・手を付けていないものを区別して failed に出し、権限の複製の失敗も failed に出す
- Specification: `docs/ir/ssh/reconnect.md#REQ-ssh-022`, `docs/ir/merge/permissions.md#REQ-merge-037`
- Prerequisites: S5
- May change: `crates/remote-merge/src/cli/merge.rs`, `crates/remote-merge/src/cli/sync.rs`, `crates/remote-merge/src/cli/rollback.rs`, `crates/remote-merge/src/cli/content_reads.rs`, `crates/remote-merge/src/cli/status.rs`, `crates/remote-merge/src/service/merge_flow.rs`, `crates/remote-merge/src/runtime/side_io.rs`, `crates/remote-merge/src/runtime/core.rs`（バックアップの読み込み）, `crates/remote-merge-engine/src/service/`, `crates/remote-merge/tests/contract/`
- Done when: 三つのファイルの merge で、二つ目の書き込みの最中に切ると、一つ目は書かれ、二つ目は結果が分からない失敗、三つ目は手を付けていない失敗として failed に出て、三つ目は変わらず、終了コード 2 を返す。エージェントの書き込みの応答が来ないとき、同じ書き込みは SSH でも行われない。書き込み先の権限がなくて 1 ファイルが失敗しただけなら、そのファイルだけ failed で、残りは書かれる。--delete の削除の最中、rollback の書き戻しの最中に切ったときも同じ形になる。二つの書き込み先の sync で一方だけ切れると、他方は全部書かれる。次の要求を送れないときは、そのファイルから後が手を付けていない失敗になる。中身を書いた後の権限の変更で切れると、中身は書いたことと結果が分からないことの両方が読める error になる。最初の読み込みが、やり直し後も切れると、何も書かずに止まる。SSH の読み取りが、やり直し後も切れたときも同じく止まる。まとめて読む処理が接続断なら、そのまとめのファイルはすべて読み込みの失敗として出て、1 ファイルずつ読み直さない。バックアップのための読み込みが接続断になったときも、そのファイルは 1 ファイルの失敗として続けず、同じく止まる。--with-permissions の権限の変更が拒まれたファイルと、読み込み元に setuid があるファイルは、中身は書いたことが分かる error で failed に出て、終了コード 2 を返し、権限は変わらない。failed の項目に欄は足さず、区別は error の文言で読める
- Shown by: test — 契約テスト `// @kotowari[REQ-ssh-022, EX-ssh-039]`、`// @kotowari[EX-ssh-038]`、`// @kotowari[EX-ssh-040]`、`// @kotowari[EX-ssh-043]`、`// @kotowari[EX-ssh-044]`、`// @kotowari[EX-ssh-045]`、`// @kotowari[EX-ssh-046]`、`// @kotowari[EX-ssh-047]`、`// @kotowari[EX-ssh-042]`、`// @kotowari[EX-ssh-048]`、`// @kotowari[REQ-merge-037, EX-merge-044]`、`// @kotowari[EX-merge-051]`、バックアップの読み込みの接続断の契約テスト `// @kotowari[REQ-backup-017]`。まとめて読む処理の接続断の扱いは、EX-ssh-042 か EX-ssh-048 のテストの中で、読み込みの要求の回数で確かめる。EX-ssh-040 は今の動きを守る回帰のテストなので HEAD でも通る。それ以外は変更前の HEAD で RED になることを確かめる
- Left to the implementer: 内部の結果の型の形（出力の JSON の形は変えない）
- Stop and hand back if: error の文言だけでは、テストが「結果が分からない」「手を付けていない」「読み込みに失敗した」「中身は書いた」を区別できなかった

### S7: diff で子の一覧を取れなかったディレクトリをエラーとして出す

- Purpose: diff の 8 か所の子の一覧の呼び出しで、「ディレクトリではない」と「一覧を取れなかった」を分け、後者をそのパスと理由で errors に出す
- Specification: `docs/ir/cli/directory-paths.md#REQ-cli-096`
- Prerequisites: S5
- May change: `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge/tests/contract/`, `crates/remote-merge/tests/cli_diff.rs`
- Done when: diff の 8 か所の子の一覧の呼び出しが S3 の新しい関数を使う。一覧の中の symlink のリンク先か更新時刻を読めないときは、errors に出して続けずにエラーで止まる。sudo=true でないサーバへ root でない利用者でつなぎ、右の "secret" が読めないディレクトリで左の "secret" に "a.txt" があるとき、"secret" を指定した diff は、"secret" をファイルとしても右にないものとしても比べず、左の "secret/a.txt" も files に出さず、"secret" と理由を errors に出して終了コード 2 を返す。通常ファイルを指定した diff は、今までどおりファイルとして比べ、errors を出さない。--ref の側の一覧（`diff.rs` の 960 行付近）も同じ扱いになる。一覧の途中の接続断は、S5 のやり直しの後に止まる。最後に、`kotowari check` が、この計画の変えたファイルと扱う ID にエラーを出さず、この計画の Verification map のどの ID も `kotowari query ID` で空でない `tests` を返す
- Shown by: test — 契約テスト `// @kotowari[REQ-cli-096, EX-cli-095]`、`// @kotowari[EX-cli-096]`。変更前の HEAD で RED になることを確かめる。続けて check — `kotowari check --format json` と、Verification map の各 ID についての `kotowari query ID | jq '.items[0].tests | length'`
- Left to the implementer: none
- Stop and hand back if: root で走るテストの環境で、読めないディレクトリを作れなかった
