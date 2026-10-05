# 計画: ファイルの大きさの上限をすべての経路と操作で一つにする

## Goal

利用者は、diff・merge・sync・rollback が、ローカル・SSH（一件ずつの読み込みと一括読み込み）・エージェントのどの経路でも、比較・書き込み・バックアップのために中身を読む前にファイルの大きさを調べ、104,857,600 バイトを超えるファイルを中身を読まずに各コマンドの読めなかったファイルとして報告し（diff は errors、merge・sync・rollback は failed）、--force でその上限を一律に外し、status にはこの上限を当てない remote-merge を得る。

## Specification

IR は `docs/ir/`。この計画は品質レビュー 1 回目の直す単位 T6（`.agents/artifacts/quality/round1/ledger.json` の theme "T6"：routes-10・routes-15・diff-2・merge_sync-5・backup_rollback-3。根拠の詳細は同じディレクトリの routes.json・diff.json・merge_sync.json・backup_rollback.json の同じ番号の項目）を、次の要件に合わせて直す。

- `docs/ir/cli/size-limit.md#REQ-cli-079`（例 EX-cli-081・EX-cli-082・EX-cli-083）
- `docs/ir/cli/merge.md#REQ-cli-074`（--force の merge が上限を当てないという後半の文だけ）
- `docs/ir/cli/status.md#REQ-cli-094`
- `docs/ir/cli/unreadable.md#REQ-cli-089`（例 EX-cli-087。diff の中身の読み込みの失敗の出し方。下の「Approach and why」の「diff の出し方」の段落のとおり、利用者の承認が要る）
- 読めなかったファイルの扱いとして参照するだけで印を足さないもの: `docs/ir/merge/read-failure.md#REQ-merge-017`（merge・sync の読み込みの失敗を failed に出す。既存の印付きのテストがある）
- 決定: `docs/decision/records/2026-10-05-quality-cli.md#A2`（D3）、`docs/decision/records/2026-10-05-quality-cli.md#A5`（D19）、`docs/decision/records/2026-10-05-quality-cli.md#A12`（E1）、S1 で作る `docs/decision/records/2026-10-05-size-limit.md`

## Approach and why

今の上限は読み込んだ後のバイト数で判定しており、その判定が三か所に分かれている。ローカルの `read_local_file_bytes`（`crates/remote-merge-engine/src/merge/executor.rs`）、SSH の一件ずつの読み込みの `validate_file_size`（`crates/remote-merge/src/runtime/remote_io.rs`）、そしてローカルの一括読み込み（`crates/remote-merge-engine/src/local_io.rs` の `read_files_bytes_batch` が `force=false` で上の関数を呼ぶ）である。SSH の一括読み込みとエージェントの読み込み（`try_agent_read_*`）には上限がなく、`force` も届かない。そのため同じファイルが経路によって読めたり読めなかったりする（diff-2・merge_sync-5・backup_rollback-3）。また、どの経路も中身を全部読んでから判定するので、上限を超えるファイルも一度はメモリに載る（routes-15 の前半）。

方針を一か所に置く。製品のすべての中身の読み込みは、`CoreRuntime` の四つの入口（`crates/remote-merge/src/runtime/side_io.rs` の `read_file`・`read_files_batch`・`read_file_bytes`・`read_files_bytes_batch`）を通る。そこから `target_io::for_side` で経路（`LocalTargetIo`・`RemoteTargetIo`。`RemoteTargetIo` はエージェントがあればエージェント、なければ SSH）に振り分けられる。例外は status のハッシュの経路（`hashes`）と、ローカルのバックアップの保存先からの読み込み（`backup_store` の `read_record`）だけで、どちらも Out of scope である。この入口で、振り分けの前に次の順で処理する。上限の値（`--force` から決まる「上限あり」か「上限なし」）を受け取る。経路に大きさだけを尋ねる。上限を超えるパスを読み込みの対象から外す。残りだけを経路に読ませる。超えたパスは、上限を超えたことを示す型付きのエラーとして返す。上限の定数、上限の値の型、型付きのエラー、大きさから「読む」「上限で読まない」を決める純粋関数は、新しい小さなモジュールに置く。`side_io.rs` はすでに 2,740 行あるので、そこには書き足さない。経路の側に残っている読み込みの後の判定と、`TargetIo::read_file_bytes` の `force` の引数は消す。判定が二か所に残ると、経路の差がまた生まれるからである。`CoreRuntime::read_file_bytes` と `save_backup`・`save_backup_if_exists` の公開の `force: bool` の引数は残し、内部で上限の値に変える。既存の契約テストがこれらを直接呼んでいるので、呼び出しの書き換えを減らすためである。

大きさは、読み込みと同じく symlink を辿った先のファイルについて調べる。既存の `stat_files` は使わない。`stat_files` は更新時刻を返す操作で、SSH の実装（`stat -c '%Y %n'`）だけがリンクそのものを調べ、ローカル（`metadata`）とエージェントはリンク先を調べる。大きさにこれを流用すると、その経路差を引き継ぐ。そこで経路ごとに大きさを返す新しい操作を `TargetIo` に足す。ローカルは `std::fs::metadata` を使う。SSH は `remote_io.rs` の新しい `stat -L -c '%s %n'` のコマンドを、他の SSH の操作と同じく `check_sudo_fallback` の後で使う（解析は `stat_remote_files` にならう）。エージェントは既存の `StatFiles` を使う。エージェントの `stat_file`（`crates/remote-merge-agent/src/agent/file_io.rs`）は、`validate_path` で正規化した後のパスに `symlink_metadata` を当てるので、すでにリンク先の大きさを返す。したがってエージェントの crate も通信の形式も変えない。`crates/remote-merge-agent` と `crates/remote-merge-protocol` はこの計画の変更の範囲に入れない。通信の形式を変える必要が見つかったときは、配置済みのエージェントの入れ替えとリリースが要るので、止めて返す（Stop conditions）。

大きさを調べられないときの扱いは、次の三つに分ける。第一に、パスごとの失敗（ない、権限がない、エージェントの `StatFiles` が結果から外したもの）は、今までどおり経路の読み込みに進め、その読み込みの失敗を返す。今のエラーの文と「ない」の判定を変えないためである。第二に、大きさを尋ねる操作そのものの失敗（SSH のコマンドが解釈されない、実行が 0 以外で終わり出力が解析できない、エージェントの応答のエラー）は、そのパスすべての読み込みのエラーにする。`stat_remote_files` の `2>/dev/null || true` をそのまま真似ると、コマンドの失敗がすべてのパスの「大きさ不明」になり、その経路だけ上限が黙って外れるからである。SSH のコマンドは、パスごとの失敗とコマンド全体の失敗を見分けられる形にする。第三に、エージェントの結果はパスで突き合わせる。結果の数がパスの数と合わなくても、それだけでエージェントを無効にせず SSH にも切り替えない（`transform_stat_results` の数の検査を流用しない）。大きさを調べてから読むまでの間にファイルが大きくなった場合は防がない。REQ-cli-079 は読む前に大きさで決めることを求めており、この計画はそれを満たす。経路の中に二つ目の上限の判定を置くと、経路の差がまた生まれる。これらは S1 で決定記録に残す。

一括読み込みでは、上限を超えたパスが結果から黙って消えてはいけない。呼び出し元は、そのパスの失敗を受け取らなければならない。今の `fetch_contents_tolerant` は結果にないパスを捨てる。diff はそれを空の中身として比べるので、同じとも違うともされてしまう（diff-1）。そこで次の形にそろえる。一括読み込みは上限を超えたパスを結果に入れない。中身が要る呼び出し元は、`fetch_contents_required`（`crates/remote-merge/src/cli/tolerant_io.rs`）のように結果にないパスを一件ずつ読み直し、上限のエラーをそのパスの失敗として受け取る。これは、先に統合される ssh-strict-exec が SSH の一括読み込みで採る契約（読めなかったパスは結果に入れず、一件ずつの読み込みでやり直す）と同じである。`fetch_contents_required` と `fetch_contents_tolerant` は、S3 で上限の値を引数に取るようにする。

上限の値の決め方は次のとおりである。--force を持つコマンド（diff・merge・sync・rollback）は、そのコマンドのすべての読み込みに --force から決まる値を渡す。対象には、バックアップのための読み込み（merge の書き込み前、sync の --delete の削除前、rollback の復元前）と、merge の --ref のバッジの読み込み（`crates/remote-merge/src/cli/merge.rs` の三つの `fetch_contents_tolerant`）を含む。今の diff は --force を受け付けるが、読み込みには常に `false` を渡している（ヘルプも "No effect" と言う）。sync・merge の削除前のバックアップ（`crates/remote-merge/src/service/merge_flow.rs` の `execute_deletions` の `save_backup(..., false)`）と、rollback の復元前のバックアップ（`side_io.rs` の `restore_backup` の `save_backup_if_exists(..., false)`。`restore_backup` は `force` を受け取らない）は `false` に固定されている。これらを直す。status は REQ-cli-094 のとおり上限を当てない。そのため status の中身の読み込みは、明示的に「上限なし」を渡す。これをしないと、入口に置いた方針が、今は上限のない SSH とエージェントの status にまで上限を当ててしまい、REQ-cli-094 から遠ざかる。

status の中身の比べ方を、少しずつ読むハッシュに替えること（REQ-cli-095）は、この計画に入れない。理由は三つある。第一に、REQ-cli-094 が求めるのは上限を当てないことで、少しずつ読むことではない。第二に、ローカル・SSH・エージェントのそれぞれのハッシュの経路が要り、SSH の側は先に統合される ssh-strict-exec と同じ所を変える。第三に、上限を一つにすることとは独立に直せる。この計画の後、status はローカルの経路でも上限を超えるファイルの中身をメモリに読むようになる。今の SSH とエージェントの経路はすでにそうしている。これを直す別の単位を早めに続けることを、利用者への確認点に挙げる。

diff の出し方。上限を超えたファイルの出し方は、各コマンドの「読めなかったファイルの扱い」に従う（REQ-cli-079）。merge と sync では、読み込みの Err はすでに failed に入る（REQ-merge-017）。diff では、そうなっていない。名前で指定したファイルを両側とも読めないと、fast path（`crates/remote-merge/src/cli/diff.rs` の `run_diff_fast_path` と engine の `status_from_read_results`）は「どちらにもない」とする。一件ずつの比較（`read_file_bytes_tolerant`）は、読めない側を空として全行を差分にする。走査の比較（`compute_statuses_and_resolve`）は、読めない側を空として判定する。この三つは REQ-cli-089（読めない側を空とみなさない）の直す範囲で、品質レビューの単位では T8 に入っている。しかし REQ-cli-089 の文は「REQ-cli-079 の大きさの上限を超えたときを含む」と書いている。EX-cli-081 も、diff が errors に出して終了コード 2 を返すことを求める。上限のエラーだけを特別に errors に出すと、T8 がすぐに一般の読み込みの失敗の扱いで置き換える道を作ることになる。そこでこの計画は、diff のこの三か所の中身の読み込みについて、読み込みの失敗を一般に errors に出す扱いを S2 で先に入れ、REQ-cli-089 と EX-cli-087 の印を付ける（T8 から diff-1・diff-5・diff-6 を引き取る）。status の REQ-cli-088 と、--ref の REQ-cli-090（`diff.rs` の参照先の読み込みの `.ok()`）は T8 に残す。単位の切り方は利用者が決めることなので、これを確認点の一つ目に挙げる。読めなかったファイルを JSON の files に項目として載せるか、summary でどう数えるかは、REQ-cli-089 が決めていない。この計画の推奨は、files に載せず errors だけに出し、summary の差分のあるファイルの数に入れないことである（比べていないファイルを「差分あり」としないため）。これも確認点に挙げ、答えを S1 の決定記録に残す。利用者が引き取りを認めないときは S2 を除き、S3 の diff のテストは T8 の統合の後に行う（Stop conditions）。

テストの作り方。104,857,600 バイトを超えるファイルは、一時ディレクトリに `File::set_len`（または `truncate`）で作る疎なファイルにし、ディスクを実際には使わない。中身を違えるには、末尾の一バイトだけを書く。上限で読まないことの確かめは、中身を読まないので軽い。--force で実際に読むテスト（S3 の merge --force のガード、S4・S5・S6）は、CLI のプロセスに 200〜400MB のメモリを使い、SSH の試験サーバ越しに 100MB を超える中身を送る。EX-cli-083 と rollback のテストは、バックアップの保存先に約 100MB の実際の写しを書く。この環境の WSL はメモリが尽きると全体が落ちるので、--force で読むテストは要件ごとに一つに絞る。「読む前に大きさで決める」ことは、上限を超える疎なファイルのモードを 000 にして示す。大きさは調べられるが、読めば権限のエラーになる。報告される理由が権限ではなく上限であれば、読む前に判定した証拠になる（root で実行すると読めてしまい区別できないので、その場合はテストが前提の崩れを知らせる）。経路の再現には次を使う。`crates/remote-merge/tests/common/mod.rs` の `CliEnv` は、ローカルと、エージェントなしの SSH の試験サーバ（`TestServer::filesystem_without_agent`）に置いた `develop` の組である。エージェントは `crates/remote-merge/tests/contract/scan_listing_cli.rs` の `AgentFixture`（`diff_via_agent`・`status_via_agent`。merge を起動する関数は path-validation が足していればそれを使い、なければ足す）。ローカル同士の経路を CLI のプロセスで再現する手段はないので、ローカルの経路は `RuntimeTargets::with_local`（`crates/remote-merge/tests/contract/sync_support.rs` の使い方）を使うプロセス内のテストか、`side_io.rs` の単体テストで示す。新しい契約テストのファイルは、`crates/remote-merge/tests/contract.rs` の既存の行にならい、`#[cfg(feature = "test-utils")]` を付けた mod 行で足す。`.kotowari/config.yaml` の `tests.files` に入るのは `crates/remote-merge/tests/contract/**`、`crates/remote-merge/tests/cli_diff.rs`、`tests/container-e2e/tests/sudo.rs` で、この計画のテストは前の二つに置く。印（`@kotowari[...]`）はそこにあるテストにだけ付け、単体テストには付けない。

## Scope of change

- 新しい方針のモジュール一つ（`crates/remote-merge/src/runtime/` の下）と `crates/remote-merge/src/runtime/mod.rs` のその宣言
- `crates/remote-merge/src/runtime/side_io.rs`
  - 四つの読み込みの入口、`restore_backup` への上限の値の受け渡し、その `mod tests`
- `crates/remote-merge/src/runtime/target_io.rs`
  - `TargetIo` の大きさを返す操作の追加と、`read_file_bytes` の `force` の除去
- `crates/remote-merge/src/runtime/remote_io.rs`
  - SSH の大きさのコマンドと解析、`read_remote_file_bytes` の `force` と `validate_file_size` の除去だけ（`remove_remote_file`・`create_remote_symlink`・一括読み込みは ssh-strict-exec の範囲なので変えない）
- `crates/remote-merge/src/runtime/core.rs`
  - `save_backup` と `save_backup_if_exists` の中で上限の値に変えることだけ
- `crates/remote-merge-engine/src/merge/executor.rs`
  - `read_local_file_bytes` の読み込み後の判定の除去と `MAX_BINARY_FILE_SIZE` の扱い、その `mod tests`
- `crates/remote-merge-engine/src/local_io.rs`
  - ローカルの大きさを返す自由関数と、`read_file_bytes`・`read_files_bytes_batch` の `force` の除去
- `crates/remote-merge-engine/src/service/types.rs`（diff の結果の `DiffError`・`MultiDiffOutput` に手を入れる場合だけ）
- `crates/remote-merge-engine/src/service/status.rs`（`status_from_read_results` だけ）と `crates/remote-merge-engine/src/service/fast_path.rs`（fast path の読み込みの失敗を扱う場合だけ）
- `crates/remote-merge/src/merge/executor.rs`（`MAX_BINARY_FILE_SIZE` の再公開を動かす場合だけ）
- `crates/remote-merge/src/cli/tolerant_io.rs`
- `crates/remote-merge/src/cli/diff.rs`
- `crates/remote-merge/src/cli/merge.rs`、`crates/remote-merge/src/cli/sync.rs`（上限の値の受け渡しだけ）
- `crates/remote-merge/src/service/merge_flow.rs`（`execute_deletions` のバックアップと、上限の値の受け渡しだけ）
- `crates/remote-merge/src/cli/rollback.rs`（--force を `restore_backup` に渡すことだけ）
- `crates/remote-merge/src/cli/status.rs`（「上限なし」を渡すことだけ）
- `crates/remote-merge/src/main.rs`（diff の --force のヘルプの文だけ）
- 契約テスト
  - `crates/remote-merge/tests/contract/` の新しいファイル一つ（`size_limit.rs` など）と `crates/remote-merge/tests/contract.rs` のその `mod` 行
  - `crates/remote-merge/tests/contract/scan_listing_cli.rs`（`AgentFixture` に merge を起動する関数を足すことだけ。既存の関数は変えない）
  - S2 の diff の読み込みの失敗のテストを置く既存の diff の契約テストのファイル（`crates/remote-merge/tests/contract/diff_*.rs` のうち一つ）か `crates/remote-merge/tests/cli_diff.rs`
- 既存のテストの書き換え（期待は変えない。直したものは名前と場所を報告に書く）
  - 上限の判定の場所を固定していたもの（`executor.rs` の `test_read_local_file_bytes_size_limit`、`side_io.rs` と `remote_io.rs` の該当テスト）
  - シグネチャの変更に伴う機械的な呼び出しの修正（`execute_deletions` を呼ぶ `crates/remote-merge/tests/contract/merge_paths.rs`・`backup_storage.rs`、`fetch_contents_*` や `read_files_*` を呼ぶもの）
  - S2 で、読めない側を空とする今の振る舞いを固定していたもの（期待を REQ-cli-089 に合わせて変える）
- `docs/decision/records/2026-10-05-size-limit.md`（新規）
- `.kotowari/changes/implementation.yaml`（rebase で入った古い `implementation.yaml` と `review.yaml` は除く）

## Step order and prerequisites

全体の前提は次のとおりである。ブランチ ssh-strict-exec（`crates/remote-merge-ssh/src/client.rs`・`batch_read.rs`、`remote_io.rs` の削除と symlink の作成、`executor.rs` の書き込み、`ssh_server.rs` を変える）と path-validation（引数の検査、SSH の種類の調べ、rollback。`side_io.rs`・`target_io.rs`・`executor.rs` の `validate_remote_path`・`diff.rs`・`merge.rs`・`sync.rs`・`scan_listing_cli.rs` を変える）は、この計画の実装の前に main に統合される。実装者は、最初にこのブランチを統合後の main へ rebase してから S1 に入る。両ブランチが変えるファイルとは衝突しうる。衝突は両ブランチの変更を保つ側で解く。両ブランチが main に入っていないときは始めない（Stop conditions）。rebase の後、path-validation が `stat_files` か `StatFiles` の新しい呼び出し元を足していないかを確かめ、足していれば S3 の大きさの操作と混同しない。この計画は SSH crate（`crates/remote-merge-ssh/`）を変えない。SSH の一括読み込みの形式は、ssh-strict-exec が統合した後のものをそのまま使う。

S1 を最初に置く。S3 の大きさの判定のしかた、S2 で T8 から引き取る範囲と diff の出力の形の根拠がそこにあるからである。S2 は S3 より前に置く。S3 の EX-cli-081 のテストは、読み込みの失敗が errors に出ることを前提にするからである。S2 の後は、今の読み込み後の判定により EX-cli-081 のテストもすでに通る。そのため S3 の RED の証拠は、EX-cli-081 ではなく、エージェントの経路で片側だけ上限を超える diff、モード 000 の merge、エージェントが無効にされないことで示す。S2 は上限と独立に、モード 000 のファイルで示せる。S4（--force）、S5（バックアップ）、S6（status）は S3 の方針の上に作るので、S3 の後に行う。三つは互いに独立である。S6 は、S3 が status の SSH の経路に上限を当ててしまう期間を短くするため、S3 の直後に行ってもよい。S7 は最後に全体を確かめる。

## Verification map

| Step | Requirements | Examples | 印を付けるテスト |
|---|---|---|---|
| S1 | REQ-cli-079, REQ-cli-089, REQ-cli-094 | | なし（決定記録） |
| S2 | REQ-cli-089 | EX-cli-087 | 契約テスト一つ: `@kotowari[REQ-cli-089, EX-cli-087]` |
| S3 | REQ-cli-079, REQ-cli-074 | EX-cli-081 | 契約テスト: EX-cli-081 の SSH の組とエージェントの組（`@kotowari[EX-cli-081]`。S2 の後に通るガード）、エージェントの片側だけの diff とモード 000 の merge の SSH・エージェント（`@kotowari[REQ-cli-079]`）、merge --force のガード（`@kotowari[REQ-cli-074, REQ-cli-079]`） |
| S4 | REQ-cli-079 | EX-cli-082 | 契約テスト一つ: `@kotowari[EX-cli-082]` |
| S5 | REQ-cli-079 | EX-cli-083 | 契約テスト二つ: `@kotowari[EX-cli-083]`、rollback の `@kotowari[REQ-cli-079]` |
| S6 | REQ-cli-094 | | 契約テスト一つ: `@kotowari[REQ-cli-094]` |
| S7 | 上のすべて | | — |

EX-cli-081 のテストと merge --force のテストは、S3 で RED にならない。前者は S2 の後、今の読み込み後の判定ですでに通る。後者は今のローカルと SSH の経路が --force を尊重し、エージェントには上限がないのですでに通る。どちらも、S3 で判定の場所を付け替えた後も同じ結果が保たれることを守るガードとして、S3 の付け替えの前に書いて通ることを確かめる。EX-cli-081 と REQ-cli-074 の上限の文にはまだ印付きのテストがないので、印を付ける。S3 の RED → GREEN の証拠は、Verification map の他の三つのテストとエージェントが無効にされないことである。

EX-cli-093（status で上限を超えるファイルを "modified" と報告する）には、この計画では印を付けない。今の status は読めなかったファイルをメタデータの判定のまま残す（status_scan-6）。そのため中身の違うファイルは、上限があってもなくても "modified" になり、このテストは今すでに通る。上限の有無を見分けられるのは、REQ-cli-088 で読めないファイルが errors に出るようになった後である。それは T8 が行う。S6 は代わりに、中身が同じで更新時刻の違うファイルで見分ける。上限があればメタデータの判定のまま "modified" に、なければ中身を比べて "equal" になる。S7 の後も `kotowari query EX-cli-093` の `tests` は空のままで、`kotowari check` は EX-cli-093 の scenario_without_test を出し続ける。これをこの計画の失敗としない。REQ-cli-095（少しずつ読むハッシュ）は review で確かめる要件で、Out of scope のとおりこの計画では満たさない。REQ-merge-017 は既存の印付きのテストのまま変えない。

## Left to the implementer

- 新しい方針のモジュールの名前と置き場所（`crates/remote-merge/src/runtime/` の下）。上限の値の型の名前と形（`force` の bool から作る列挙など）。型付きのエラーの作り方（`AppError` に足すか、方針のモジュールの独自の型か）。上限の定数の正本の場所（一つだけにし、`MAX_BINARY_FILE_SIZE` の既存の公開パスを残すかは任せる）
- 上限を超えたときのエラーの文。今の "File too large (N bytes > 104857600 bytes limit): パス. Use --force to override." を保つことを勧めるが、文はどの IR でも契約ではない。テストは文を固定せず、パスと終了コードと、権限の理由（"Permission denied"）を含まないことを確かめる
- 大きさを返す `TargetIo` の操作の名前と戻り値の形（パスごとの大きさの対応表など）。SSH のコマンドで、パスごとの失敗とコマンド全体の失敗を見分ける書き方
- diff の --force のヘルプの英語の文
- diff の errors の reason の文（側を含めるかを含む。テストは固定しない）
- テストの補助関数（疎なファイルを作る関数など）の置き場所。新しい契約テストのファイルの中か `crates/remote-merge/tests/common/`

## Stop conditions

- ssh-strict-exec か path-validation が main に統合されていない。または、rebase の後に SSH の一括読み込みが、読めなかったパスを結果から外さず、空の中身として返している
- エージェントの経路で読む前に大きさを知るのに、通信の形式（`crates/remote-merge-protocol`）かエージェントの crate を変える必要がある。配置済みのエージェントの入れ替えとリリースが要るので、利用者が決める
- 対応するリモート（CentOS 5 を含む）の `stat` が `-L` か `%s` を受け付けない
- 中身の読み込みのうち、`CoreRuntime` の四つの入口を通らないものが見つかる。ただし、status のハッシュの経路（`hashes`）と、ローカルのバックアップの保存先からの読み込み（`backup_store` の `read_record`）は Out of scope のとおり除く
- 利用者が、S2 で REQ-cli-089 の diff の部分を T8 から引き取ることを認めない。その場合は S2 を除き、S3 の EX-cli-081 のテストは T8 の統合の後に行う
- 既存のテストが、上限を超えるファイルについて、SSH の一括読み込みやエージェントの経路で読めることを期待している
- --force で実際に読むテストが、この環境でメモリ不足か時間切れを起こす

## Test command

```sh
cargo nextest run --all-features
```

## Out of scope

- status を少しずつ読むハッシュで比べること（`docs/ir/cli/status.md#REQ-cli-095`）。上の「Approach and why」の理由で、別の単位に分ける
- status の読めないファイルの出し方（`docs/ir/cli/unreadable.md#REQ-cli-088`）と、diff の --ref の参照先の読み込みの失敗（`docs/ir/cli/unreadable.md#REQ-cli-090`。上限を超えた参照先も、今は参照先にないものとされる）。どちらも T8 に残す
- エージェントが一括読み込みの結果をすべてメモリに持つこと（routes-15 の後半）と、ReadFiles のフレームが 16MB を超えうること。上限でファイル一つの大きさは抑えられるが、一括の大きさは T2・T13 で扱う
- エージェントのテキストの読み込みが UTF-8 として不正なバイトでエラーになること（routes-10 の後半。ssh_agent-14 と同じ原因）
- rollback がローカルのバックアップの保存先から写しを読むこと（`backup_store` の `read_record`）。保存先は経路（ローカル・SSH・エージェント）のファイルではない。写しは保存の時点で上限か --force を通っている。A2 の理由が rollback について挙げるのは復元前のバックアップの読み込みだけである
- merge の --ref の三者の確認（`crates/remote-merge/src/cli/merge.rs` の三つの `read_file_bytes`）の失敗の出し方（A9 の "three-way comparison incomplete"）。上限の値は S3 の方針を通るが、出し方は T10 で扱う
- 大きさを調べてから読むまでの間にファイルが大きくなった場合の防御
- SSH の crate（`crates/remote-merge-ssh/`）とエージェントの crate の変更

## Steps

### S1: 上限の判定のしかたを決定記録に残す

- Purpose: この計画で決める扱いを、根拠と決めた役とともに `docs/decision/records/2026-10-05-size-limit.md` に残す。扱いは、方針を `CoreRuntime` の読み込みの入口に一つ置くこと、大きさは symlink を辿った先で調べること、大きさを調べられないときの三つの扱い（パスごとの失敗は読み込みに進める、操作そのものの失敗はエラー、エージェントの結果はパスで突き合わせ数の違いで切り替えない）、調べてから読むまでの間の増加を防がないこと、一括読み込みは上限を超えたパスを結果に入れず呼び出し元が一件ずつ読み直すこと、status に「上限なし」を渡すこと、ローカルのバックアップの保存先の写しの読み込みに上限を当てないこと、diff の中身の読み込みの失敗の出し方（REQ-cli-089）を T8 から引き取ることと、その JSON の files と summary での扱い
- Specification: `docs/ir/cli/size-limit.md#REQ-cli-079`, `docs/ir/cli/unreadable.md#REQ-cli-089`, `docs/ir/cli/status.md#REQ-cli-094`
- Prerequisites: ブランチが ssh-strict-exec と path-validation の統合後の main に rebase されている。利用者がこの計画を承認したときに、確認点（T8 からの引き取り、diff の読めないファイルの files と summary での扱い、大きさを調べられないパスの扱い、status を少しずつ読むハッシュを別の単位にすること）に答えている
- May change: `docs/decision/records/2026-10-05-size-limit.md`
- Done when: 決定記録が、既存の決定記録（`docs/decision/records/2026-10-05-quality-cli.md` など）と同じ形の Context と Agreements を持つ。上の扱いが、それぞれ A 番号の項目として why と decided_by（計画の作成者の提案を利用者が承認した、またはこの計画の確認点への利用者の答え）付きで書かれ、A2・A5・A12 と REQ-cli-079・REQ-cli-089・REQ-cli-094 を参照している。`kotowari check --format json` がこのファイルに finding を出さない
- Shown by: artifact — `docs/decision/records/2026-10-05-size-limit.md`。`kotowari check --format json` でこのファイルへの finding がないこと
- Left to the implementer: 項目の順と文の書き方
- Stop and hand back if: 確認点への利用者の答えが計画の推奨と違い、その答えがこの計画の手順を変える

### S2: diff が中身を読めないファイルを空とみなさず errors に出す

- Purpose: diff の三か所の中身の読み込み（fast path の読み込み、走査の中身の比較、一件ずつの比較）で、読み込みの失敗を空の中身や「ない」として扱わず、そのパスと理由を JSON のトップレベルの errors（テキストでは標準エラー）に出し、他のファイルの比較を続けて終了コード 2 を返す。読めなかったファイルの files と summary での扱いは S1 の決定記録のとおりにする
- Specification: `docs/ir/cli/unreadable.md#REQ-cli-089`
- Prerequisites: S1
- May change: `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge/src/cli/tolerant_io.rs`, `crates/remote-merge-engine/src/service/types.rs`（走査の段階の errors を出力へ運ぶ場合だけ）, `crates/remote-merge-engine/src/service/status.rs`（`status_from_read_results` だけ）, `crates/remote-merge-engine/src/service/fast_path.rs`, 既存の diff の契約テストのファイル一つか `crates/remote-merge/tests/cli_diff.rs`, 読めない側を空とする今の振る舞いを固定している既存のテスト（直したものは名前と場所を報告に書く）
- Done when: `CliEnv`（ローカルと SSH の `develop`）で、左右にある `a.txt` のうちリモートの側のモードを 000 にし、`b.txt` は左右で中身が違う状態にする。`diff a.txt b.txt --left local --right develop --format json` を実行すると次のようになる。終了コードは 2 である。トップレベルの errors に `a.txt` とその理由がある。`a.txt` の全ての行が差分として出ない。files と summary での `a.txt` の扱いが S1 の決定記録のとおりである。`b.txt` の差分は出る。パスを指定しない diff（走査の比較）でも同じ状態で、`a.txt` が "equal" や空との差分として扱われず、errors に出る。テキストの出力では、`a.txt` の理由が標準エラーに出る。`a.txt` の左右がともに読めないとき、"not found on either side" ではなく errors に出る。いまは全行の差分と終了コード 1 になる
- Shown by: test — 契約テスト一つ（`// @kotowari[REQ-cli-089, EX-cli-087]`）で、名前を指定した diff の JSON の errors と終了コードを RED → GREEN にする。パスを指定しない diff と両側とも読めない場合は、同じ契約テストのファイルの印のないテストか単体テストで示す。root で実行して読めてしまうときは、前提の崩れを知らせる
- Left to the implementer: 走査の段階で集めた読み込みの失敗を出力の errors へ運ぶ形。三か所の共通部分の切り出し方。`read_file_bytes_tolerant` の改名か削除
- Stop and hand back if: 読めないファイルを errors に出すと、REQ-cli-022（symlink の参照先）や REQ-cli-063（--ref）の既存の印付きのテストの期待と食い違う。または、ファイルが「ない」ことと読めないことを、diff のどこかの経路で見分ける情報がない

### S3: 一つの上限を読む前にすべての経路で当てる

- Purpose: `CoreRuntime` の四つの読み込みの入口で、経路に大きさを尋ねて上限を超えるパスを読まずに型付きのエラーにし、経路の側の読み込み後の判定と `TargetIo::read_file_bytes` の `force` を消す。各経路は、symlink を辿った先の大きさを返す操作を持つ（ローカルは `metadata`、SSH は `stat -L -c '%s %n'`、エージェントは既存の `StatFiles`）。`fetch_contents_required` と `fetch_contents_tolerant` は上限の値を引数に取り、今の呼び出し元は今と同じ値（`fetch_contents_required` は呼び出し元の force、`fetch_contents_tolerant` は上限あり）を渡す
- Specification: `docs/ir/cli/size-limit.md#REQ-cli-079`, `docs/ir/cli/merge.md#REQ-cli-074`
- Prerequisites: S1, S2
- May change: 新しい方針のモジュールと `crates/remote-merge/src/runtime/mod.rs`, `crates/remote-merge/src/runtime/side_io.rs`, `crates/remote-merge/src/runtime/target_io.rs`, `crates/remote-merge/src/runtime/remote_io.rs`, `crates/remote-merge/src/runtime/core.rs`, `crates/remote-merge-engine/src/merge/executor.rs`, `crates/remote-merge-engine/src/local_io.rs`, `crates/remote-merge/src/merge/executor.rs`, `crates/remote-merge/src/cli/tolerant_io.rs`, `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge/src/cli/merge.rs`, `crates/remote-merge/src/cli/sync.rs`, `crates/remote-merge/src/cli/status.rs`（`fetch_contents_tolerant` の引数を足すことだけ）, `crates/remote-merge/src/service/merge_flow.rs`, `crates/remote-merge/tests/contract/` の新しい契約テストのファイルと `crates/remote-merge/tests/contract.rs` の mod 行, `crates/remote-merge/tests/contract/scan_listing_cli.rs`（`AgentFixture` に merge を起動する関数を足すことだけ）, Scope of change の既存のテストの書き換え
- Done when: 次の七つが成り立つ。(1) 方針の純粋関数の単体テストで、大きさ 104,857,600 のパスは読む対象に入り、104,857,601 のパスは上限のエラーになり、「上限なし」ではどちらも読む対象に入り、大きさの分からないパスは読む対象に入る。(2) `CoreRuntime` の単体テスト（ローカルの経路）で、モード 000 の 104,857,601 バイトの疎なファイルの `read_file_bytes`・`read_files_bytes_batch`・`read_file` が上限のエラーを返すか、結果に入れない。権限のエラーにはならない。大きさを尋ねる操作そのものが失敗すると読み込みのエラーになり、上限を外して読まない。(3) `AgentFixture` で、ローカルに小さい `big.bin`、リモートに 104,857,601 バイトの疎な `big.bin` を置き、`diff big.bin --format json` を --force なしに実行すると、終了コード 2 で、トップレベルの errors に `big.bin` が出る。いまはエージェントが読めてしまい、バイナリの差分と終了コード 1 になる。(4) ローカルに小さい `big.bin`、リモートにモード 000 の 104,857,601 バイトの疎な `big.bin` を置き、`merge big.bin --left local --right <サーバ> --format json` を実行すると、`CliEnv`（SSH）の組と `AgentFixture` の組のどちらでも、終了コード 2 で、failed の `big.bin` の error の "right:" の理由が上限の理由で、"Permission denied" を含まない。リモートの中身とモードは変わらない。(5) (3) と (4) のエージェントの組で、エージェントが無効にされず SSH に切り替わらない（診断ログか `AgentFixture` の既存の確かめ方で示す。いまはエージェントの読み込みの失敗でエージェントが無効になる）。(6) ガードとして、S3 の付け替えの前に書いて通ることを確かめた二つが付け替えの後も通る。一つは EX-cli-081 のとおり、左右に中身の違う 104,857,601 バイトの疎なファイル `big.bin` がある状態で `diff big.bin --format json` を --force なしに実行すると、`CliEnv` の組でも `AgentFixture` の組でも、終了コード 2 で errors に `big.bin` が出ること。もう一つは `CliEnv` の組で、左にだけ 104,857,601 バイトの疎な `big.bin` がある状態の `merge big.bin --left local --right develop --force` が終了コード 0 で、書き込み先の `big.bin` が左と同じ大きさと末尾の一バイトになり、--force なしでは failed になること。(7) diff の `reads_as_file`（一覧の取れない側を中身が読めるかで通常ファイルとみなす判定）は、上限のエラーを「通常ファイルである」とみなし、上限を超えるファイルをディレクトリ扱いにしない。既存のテストは、Scope of change の書き換えをしたうえで通る
- Shown by: test — 方針の純粋関数と `CoreRuntime` のローカルの経路の単体テスト（印なし）を RED → GREEN にする。契約テストでは、(3)（`// @kotowari[REQ-cli-079]`）と (4) の SSH の組とエージェントの組（それぞれ `// @kotowari[REQ-cli-079]`）を RED → GREEN にし、(5) を (3) か (4) のエージェントの組の中で確かめる。(6) のガードは EX-cli-081 の二つ（それぞれ `// @kotowari[EX-cli-081]`）と merge --force の一つ（`// @kotowari[REQ-cli-074, REQ-cli-079]`）で、付け替えの前と後の両方で通ることを示す。モード 000 のテストは root で実行して読めてしまうとき前提の崩れを知らせる
- Left to the implementer: 計画全体の「Left to the implementer」のうち、方針のモジュール・型・エラー・大きさの操作の形。`reads_as_file` で上限のエラーを見分ける方法。一括読み込みで、上限を超えるパスを除いた残りを一度に読むか、経路の一括読み込みに任せるか（どちらでも、上限を超えるパスは経路に読ませない）
- Stop and hand back if: 全体の Stop conditions のエージェントの通信の形式、`stat -L`、四つの入口を通らない読み込みのどれかに当たる。または、エージェントの `StatFiles` が symlink のパスに対してリンク先ではなくリンクそのものの大きさを返す。または (3) の今の結果が、エージェントが読めてしまう以外の理由で終了コード 2 になり、RED を作れない

### S4: --force で diff の上限を外す

- Purpose: diff の --force をすべての読み込み（比べる左右、走査の比較、--ref の参照先）に渡して上限を外し、ヘルプの "No effect" の文を改める。merge の --ref のバッジの読み込み（`merge.rs` の三つの `fetch_contents_tolerant`）にも merge の --force から決まる値を渡す
- Specification: `docs/ir/cli/size-limit.md#REQ-cli-079`
- Prerequisites: S3
- May change: `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge/src/cli/merge.rs`（--ref のバッジの三つの読み込みだけ）, `crates/remote-merge/src/cli/tolerant_io.rs`, `crates/remote-merge/src/main.rs`（diff の --force のヘルプの文だけ）, S3 の新しい契約テストのファイル
- Done when: EX-cli-082 のとおり、左右に中身の違う 104,857,601 バイトの疎なファイル `big.bin` がある `CliEnv` の組で、`diff big.bin --force --format json` を実行すると次のようになる。終了コードは 1 である。errors は空である。`big.bin` の項目が、左右の中身を比べた結果（バイナリとして左右の違うハッシュ）を持つ。いまは両側の読み込みが上限で失敗する。`remote-merge diff --help` の --force の説明が "No effect" でなくなっている。merge の --ref のバッジの読み込みが merge の --force の値を受け取ることは、コードの読み合わせで確かめる（契約テストは足さない）
- Shown by: test — 契約テスト一つ（`// @kotowari[EX-cli-082]`）で RED → GREEN にする
- Left to the implementer: ヘルプの英語の文
- Stop and hand back if: diff の --force に上限以外の効き目を求める既存のテストか IR の要件が見つかる

### S5: バックアップのための読み込みに --force を届ける

- Purpose: sync・merge の --delete の削除前のバックアップ（`execute_deletions` の `save_backup(..., false)`）と、rollback の復元前のバックアップ（`restore_backup` の `save_backup_if_exists(..., false)`）に、そのコマンドの --force から決まる値を渡す
- Specification: `docs/ir/cli/size-limit.md#REQ-cli-079`
- Prerequisites: S3
- May change: `crates/remote-merge/src/service/merge_flow.rs`, `crates/remote-merge/src/cli/merge.rs`, `crates/remote-merge/src/cli/sync.rs`, `crates/remote-merge/src/runtime/side_io.rs`（`restore_backup` とその `mod tests`）, `crates/remote-merge/src/cli/rollback.rs`, S3 の新しい契約テストのファイル, Scope of change の既存のテストの書き換え（`execute_deletions` と `restore_backup` の呼び出し）
- Done when: 次の二つが成り立つ。(1) EX-cli-083 のとおり、バックアップが有効で、書き込み先にだけ 104,857,601 バイトの疎なファイル `big.bin` がある `CliEnv` の組で、`sync . --left local --right develop --delete --force --format json` を実行すると、終了コード 0 で、書き込み先の `big.bin` が消え、バックアップの保存先にその写しがある（大きさが同じ）。(2) バックアップが有効な `CliEnv` の組で、小さい `a.txt` の merge でバックアップのセッションを作る。その後、書き込み先の `a.txt` を 104,857,601 バイトの疎なファイルに替える。`rollback --target develop --force --format json` を実行すると、終了コード 0 で、`a.txt` が戻り、restored の `a.txt` の `pre_rollback_backup` にセッションがある。いまはどちらも "backup failed" で failed になる
- Shown by: test — 契約テスト二つで RED → GREEN にする。sync の削除（`// @kotowari[EX-cli-083]`）と rollback（`// @kotowari[REQ-cli-079]`）である
- Left to the implementer: `execute_deletions` と `restore_backup` へ上限の値を渡す引数の形
- Stop and hand back if: rollback の --force の既存の効き目（確認のプロンプトを省くことと、期限切れのセッションの復元を許すこと）を固定している既存のテストか IR の要件と、上限を外すことが食い違う。または、path-validation の rebase 後の `restore_backup` の構造が、復元前のバックアップを別の関数で行っている（その場合はその関数に渡す。渡し先が二つ以上に分かれるなら返す）

### S6: status に上限を当てない

- Purpose: status の中身の読み込み（`fetch_contents_tolerant` の三つの呼び出し）に「上限なし」を渡し、どの経路でも 104,857,600 バイトを超えるファイルの中身を比べて判定する
- Specification: `docs/ir/cli/status.md#REQ-cli-094`
- Prerequisites: S3
- May change: `crates/remote-merge/src/cli/status.rs`, S3 の新しい契約テストのファイル
- Done when: `CliEnv` の組（エージェントなし。ハッシュの比較が使われず中身を読む組）で、左右に中身が同じで大きさが同じ 104,857,601 バイトの疎なファイル `big.bin` を、更新時刻を違えて置く。`status --format json` を実行すると、`big.bin` が "equal" と報告される。いまはローカルの一括読み込みが上限で読めず、メタデータの判定のまま "modified" になる
- Shown by: test — 契約テスト一つ（`// @kotowari[REQ-cli-094]`）で RED → GREEN にする。EX-cli-093 には印を付けない（Verification map の理由）
- Left to the implementer: none
- Stop and hand back if: この組の status がハッシュの比較で決まり、中身を読まない（その場合は今すでに上限がなく RED にならない）。または、SSH で 100MB を超える中身を読むテストがこの環境で時間切れかメモリ不足になる

### S7: 全体の確認と変更の記録

- Purpose: この計画の変更がすべての確認を通り、比較全体の実装者の記録があることを示す
- Specification: `docs/ir/cli/size-limit.md#REQ-cli-079`, `docs/ir/cli/merge.md#REQ-cli-074`, `docs/ir/cli/status.md#REQ-cli-094`, `docs/ir/cli/unreadable.md#REQ-cli-089`
- Prerequisites: S1, S2, S3, S4, S5, S6
- May change: `.kotowari/changes/implementation.yaml`, `.kotowari/changes/review.yaml`（rebase 後に残っていれば削除だけ）、および S1〜S6 の May change のファイルの整形と clippy の指摘の修正
- Done when: `cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`、`kotowari changes --base <比較元の完全な ID> --staged --phase implementation --format json` が成功する。`kotowari query` で、REQ-cli-079・REQ-cli-074・REQ-cli-094・REQ-cli-089・EX-cli-081・EX-cli-082・EX-cli-083・EX-cli-087 の tests に S2〜S6 の契約テストがある。`kotowari check --format json` は終了コード 0 を求めない（main の時点ですでに他の error がある）。このブランチで変えたファイルと、この計画の ID に finding がないことを求める。ただし EX-cli-093 の scenario_without_test は Verification map のとおり残ってよい。rebase 後の main にある古い比較の記録が除かれている。新しい `implementation.yaml` が、比較元（rebase 後の `git merge-base main HEAD` の完全なコミット ID）からこのブランチの変更をすべて覆う。S1 の決定を使う変更（大きさを調べられないときの扱い、一括読み込みの契約、ローカルのバックアップの保存先の除外、diff の読めないファイルの files と summary での扱い）は、gap（disposition recorded、S1 の決定記録と REQ-cli-079・REQ-cli-089 の IR を参照）として記録する。他は、既存の要件と IR を参照する existing として記録する。`kotowari check` が、このブランチで変えていないファイルの error を出しても直さず、報告に書く。レビューの記録 `review.yaml` は、実装と別のレビュー担当が作る
- Shown by: check — `cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`、`kotowari check --format json`（上の範囲の finding だけを見る）、`kotowari changes --base <比較元の完全な ID> --staged --phase implementation --format json`。統合の前には、独立したレビューの記録を足した後で `kotowari changes --base <比較元の完全な ID> --head <対象の完全な ID> --phase review --format json` が終了コード 0 になること
- Left to the implementer: 記録の entry の分け方と ID
- Stop and hand back if: このブランチで変えたファイルか、この計画の ID（EX-cli-093 の scenario_without_test を除く）に、直し方の分からない kotowari check の finding が残る
