# 計画: ファイルの大きさの上限をすべての経路と操作で一つにする

## Goal

利用者は、diff・merge・sync・rollback が、ローカル・SSH（一件ずつの読み込みと一括読み込み）・エージェントのどの経路でも、比較・書き込み・バックアップのために中身を読む前にファイルの大きさを調べ、104,857,600 バイトを超えるファイルを中身を読まずに各コマンドの読めなかったファイルとして報告し（diff は errors、merge・sync・rollback は failed）、--force でその上限を一律に外し、status にはこの上限を当てない remote-merge を得る。

## Specification

IR は `docs/ir/`。この計画は品質レビュー 1 回目の直す単位 T6（main のチェックアウトの `/home/mizumi/develop/remote-merge/.agents/artifacts/quality/round1/ledger.json` の theme "T6"。worktree の中にはない。routes-10・routes-15・diff-2・merge_sync-5・backup_rollback-3。根拠の詳細は同じディレクトリの routes.json・diff.json・merge_sync.json・backup_rollback.json の同じ番号の項目）を、次の要件に合わせて直す。

- `docs/ir/cli/size-limit.md#REQ-cli-079`（例 EX-cli-081・EX-cli-082・EX-cli-083）
- `docs/ir/cli/merge.md#REQ-cli-074`（--force の merge が上限を当てないという後半の文だけ）
- `docs/ir/cli/status.md#REQ-cli-094`
- `docs/ir/cli/unreadable.md#REQ-cli-089`（例 EX-cli-087。diff の中身の読み込みの失敗の出し方。この単位で T8 から引き取ることは `docs/decision/records/2026-10-05-size-limit.md#A9` で決まった。`docs/decision/records/2026-10-05-size-limit.md#A15` により、読めなかったファイルを files に載せず summary の差分のあるファイルの数に入れないこと（`docs/decision/records/2026-10-05-size-limit.md#A10`）が本文に書かれている）
- `docs/ir/cli/binary.md#REQ-cli-061`（`docs/decision/records/2026-10-05-size-limit.md#A13` で、"missing" はない側だけになり、読めない側は REQ-cli-089 のとおり errors に出るよう改められた。S2 がその印付きのテストを書き換える）
- 読めなかったファイルの扱いとして参照するだけで印を足さないもの: `docs/ir/merge/read-failure.md#REQ-merge-017`（merge・sync の読み込みの失敗を failed に出す。既存の印付きのテストがある）
- 決定: `docs/decision/records/2026-10-05-quality-cli.md#A2`（D3）、`docs/decision/records/2026-10-05-quality-cli.md#A5`（D19）、`docs/decision/records/2026-10-05-quality-cli.md#A12`（E1）、`docs/decision/records/2026-10-05-size-limit.md`（A1〜A19。計画の承認、確認点への利用者の答え、計画の敵対的レビューを受けた修正の承認で確定し、計画の修正の時点で書かれている）

## Approach and why

今の上限は読み込んだ後のバイト数で判定しており、その判定が三か所に分かれている。ローカルの `read_local_file_bytes`（`crates/remote-merge-engine/src/merge/executor.rs`）、SSH の一件ずつの読み込みの `validate_file_size`（`crates/remote-merge/src/runtime/remote_io.rs`）、そしてローカルの一括読み込み（`crates/remote-merge-engine/src/local_io.rs` の `read_files_bytes_batch` が `force=false` で上の関数を呼ぶ）である。SSH の一括読み込みとエージェントの読み込み（`try_agent_read_*`）には上限がなく、`force` も届かない。そのため同じファイルが経路によって読めたり読めなかったりする（diff-2・merge_sync-5・backup_rollback-3）。また、どの経路も中身を全部読んでから判定するので、上限を超えるファイルも一度はメモリに載る（routes-15 の前半）。

方針を一か所に置く。製品のすべての中身の読み込みは、`CoreRuntime` の四つの入口（`crates/remote-merge/src/runtime/side_io.rs` の `read_file`・`read_files_batch`・`read_file_bytes`・`read_files_bytes_batch`）を通る。そこから `target_io::for_side` で経路（`LocalTargetIo`・`RemoteTargetIo`。`RemoteTargetIo` はエージェントがあればエージェント、なければ SSH）に振り分けられる。例外は status のハッシュの経路（`hashes`）と、ローカルのバックアップの保存先からの読み込み（`crates/remote-merge-engine/src/backup_store.rs` の `read_record`。製品の `runtime/backup_store.rs` は再公開するだけ）だけで、どちらも Out of scope である。この入口で、振り分けの前に次の順で処理する。上限の値（`--force` から決まる「上限あり」か「上限なし」）を受け取る。四つの入口はどれも上限の値を受け取る（テキストの読み込みにも --force を届けるため）。「上限なし」なら大きさを尋ねず、そのまま経路に読ませる（`docs/decision/records/2026-10-05-size-limit.md#A17`）。「上限あり」なら経路に大きさだけを尋ねる。上限を超えるパスを読み込みの対象から外す。残りだけを経路に読ませる。超えたパスの返し方は入口によって分かれる。一件ずつの読み込み（`read_file`・`read_file_bytes`）は、上限を超えたことを示す型付きのエラーを Err として返す。一括読み込み（`read_files_batch`・`read_files_bytes_batch`）は、超えたパスを結果に入れずに Ok を返し、呼び出し元が一件ずつ読み直して Err を受け取る（下の一括読み込みの段落）。上限の定数、上限の値の型、型付きのエラー、大きさから「読む」「上限で読まない」を決める純粋関数は、新しい小さなモジュールに置く。`side_io.rs` はすでに 2,735 行あるので、そこには書き足さない。経路の側に残っている読み込みの後の判定と、`TargetIo::read_file_bytes` の `force` の引数は消す。判定が二か所に残ると、経路の差がまた生まれるからである。`CoreRuntime::read_file_bytes` と `save_backup`・`save_backup_if_exists` の公開の `force: bool` の引数は残し、内部で上限の値に変える。既存の契約テストがこれらを直接呼んでいるので、呼び出しの書き換えを減らすためである。`read_file`・`read_files_batch`・`read_files_bytes_batch` は上限の値を新しく受け取るので、`side_io.rs` の `mod tests` の呼び出し（1129 行、1199 行、1237 行、1266 行の付近ほか）も書き換える。

大きさは、読み込みと同じく symlink を辿った先のファイルについて調べる。既存の `stat_files` は使わない。`stat_files` は更新時刻を返す操作で、SSH の実装（`stat -c '%Y %n'`）だけがリンクそのものを調べ、ローカル（`metadata`）とエージェントはリンク先を調べる。大きさにこれを流用すると、その経路差を引き継ぐ。そこで経路ごとに大きさを返す新しい操作を `TargetIo` に足す（`docs/decision/records/2026-10-05-size-limit.md#A2`）。ローカルは `std::fs::metadata` を使う。SSH は `remote_io.rs` の新しいコマンドを、他の SSH の操作と同じく `check_sudo_fallback` の後で、`remote_merge_ssh::exec_strict_bytes` か `exec_strict` で実行する。このコマンドは名前を出力して解析しない（`docs/decision/records/2026-10-05-size-limit.md#A8`）。尋ねたパスの順番に、パスごとに大きさかパスごとの失敗の印を区切りの印で挟んで返し、コマンド全体の失敗は終了コードで分かるようにする。`crates/remote-merge-ssh/src/batch_read.rs` と `crates/remote-merge/src/runtime/remote_path/script.rs` がすでにこの形を使っている。中でパスごとに `stat -L -c %s` を使うことはできる。名前を出力して解析すると、改行を含む名前で壊れる。`docs/decision/records/2026-10-05-size-limit.md#A18` のとおり、このコマンドはさらに次のようにする。最初に、既知のパスで `stat` が使え `-L` と `%s` を受け付けることを確かめ、だめなら 0 以外で終える。終了コード 0 でも解析できない出力は操作の失敗とする。出力の解析は純粋関数にして単体テストを書く。尋ねるパスが多いときは、SSH の一括読み込みと同じ長さの上限でコマンドを分ける。分け方は `crates/remote-merge-ssh/src/batch_read.rs` の公開の `chunk_paths` と `SSH_BATCH_MAX_COMMAND_LEN` を使い回し、SSH の crate は変えない。接続のエラーでは、`remote_io.rs` の他の読み込みと同じく（`crate::error::is_connection_error` と `CoreRuntime::try_reconnect`）一度だけ再接続して再試行する。コマンドの頭には、`remote_path/script.rs` の `INSPECT_PATH_PREFIX` と同じく見分けるための決まった前置きを付け、test-utils の機能のときだけ公開する。試験の SSH サーバ（`crates/remote-merge/tests/contract/ssh_server.rs`）は知らないコマンドに "ready\n" と答えるので、S3 の後に読み込みの前に大きさのコマンドが走ると、REQ-ssh-003 の印付きのテスト（`ssh_reconnect.rs` の EX-ssh-005・EX-ssh-006・EX-ssh-008、interrupt_first_read・interrupt_all_reads）が壊れる。そこで試験のサーバがこの前置きのコマンドに答えるようにする。エージェントは既存の `StatFiles` を使う。エージェントの `stat_file`（`crates/remote-merge-agent/src/agent/file_io.rs`）は、`validate_path` で正規化した後のパスに `symlink_metadata` を当てるので、すでにリンク先の大きさを返す。したがってエージェントの crate も通信の形式も変えない。ただし、既存の `try_agent_stat_files`（`side_io.rs`）は通さない（`docs/decision/records/2026-10-05-size-limit.md#A8`・`docs/decision/records/2026-10-05-size-limit.md#A11`）。これは `with_agent` を通り、エージェントの操作のどんなエラーも None（SSH への切り替え）に変える。また `transform_stat_results` を通り、結果の数が違うと SSH に切り替える。大きさの操作は、`side_io.rs` に新しいエージェントの大きさの補助関数を置き、結果をパスで突き合わせる独自の道で作る。エージェントのエラーは `docs/decision/records/2026-10-05-size-limit.md#A16` のとおり二つに分ける。エージェントが返したエラー（応答のエラー、想定外の応答、フレームの上限超え）は、尋ねたパスすべての読み込みのエラーにする。接続が切れたエラー（`should_invalidate_agent_error` が真になる BrokenPipe・EOF など）は、エージェントを無効にして SSH の大きさのコマンドに切り替える。エージェントが結果から外したパス（root_dir の外を指す symlink など）は、`docs/decision/records/2026-10-05-size-limit.md#A14` のとおり SSH の大きさのコマンドに尋ね直してから上限を判定する。sudo を使うサーバでは、既存の `check_sudo_fallback` のとおり SSH に切り替えずエラーにする。エージェントへの問い合わせは、読み込みと同じ `AGENT_READ_BATCH_SIZE` 件ごとに分ける（`docs/decision/records/2026-10-05-size-limit.md#A18`）。`crates/remote-merge-agent` と `crates/remote-merge-protocol` はこの計画の変更の範囲に入れない。通信の形式を変える必要が見つかったときは、配置済みのエージェントの入れ替えとリリースが要るので、止めて返す（Stop conditions）。

大きさを調べられないときの扱いは、次の三つに分ける。第一に、パスごとの失敗（ない、権限がない。エージェントの `StatFiles` が結果から外したものは、上のとおり SSH の大きさのコマンドに尋ね直した後のパスごとの失敗）は、今までどおり経路の読み込みに進め、その読み込みの失敗を返す。今のエラーの文と「ない」の判定を変えないためである。第二に、大きさを尋ねる操作そのものの失敗（SSH のコマンドが解釈されない、`stat` の確かめに失敗して 0 以外で終わる、終了コードによらず出力が解析できない、エージェントが返したエラー）は、そのパスすべての読み込みのエラーにする。エージェントの接続が切れたエラーだけは、上のとおり SSH の大きさのコマンドに切り替える。`stat_remote_files` の `2>/dev/null || true` をそのまま真似ると、コマンドの失敗がすべてのパスの「大きさ不明」になり、その経路だけ上限が黙って外れるからである。SSH のコマンドは、パスごとの失敗とコマンド全体の失敗を見分けられる形にする。第三に、エージェントの結果はパスで突き合わせる。結果の数がパスの数と合わなくても、それだけでエージェントを無効にせず SSH にも切り替えない（`transform_stat_results` の数の検査を流用しない）。大きさを調べてから読むまでの間にファイルが大きくなった場合は防がない。REQ-cli-079 は読む前に大きさで決めることを求めており、この計画はそれを満たす。経路の中に二つ目の上限の判定を置くと、経路の差がまた生まれる。これらは `docs/decision/records/2026-10-05-size-limit.md#A11`・`docs/decision/records/2026-10-05-size-limit.md#A14`・`docs/decision/records/2026-10-05-size-limit.md#A16`・`docs/decision/records/2026-10-05-size-limit.md#A3` に決まっている。

一括読み込みでは、上限を超えたパスが結果から黙って消えてはいけない。呼び出し元は、そのパスの失敗を受け取らなければならない。今の `fetch_contents_tolerant` は結果にないパスを捨てる。diff はそれを空の中身として比べるので、同じとも違うともされてしまう（diff-1）。そこで次の形にそろえる。一括読み込みは上限を超えたパスを結果に入れない。中身が要る呼び出し元は、`fetch_contents_required`（`crates/remote-merge/src/cli/tolerant_io.rs`）のように結果にないパスを一件ずつ読み直し、上限のエラーをそのパスの失敗として受け取る。これは、main に統合済みの ssh-strict-exec が SSH の一括読み込みで採っている契約（読めなかったパスは結果に入れず、一件ずつの読み込みでやり直す）と同じである（`docs/decision/records/2026-10-05-size-limit.md#A4`）。`fetch_contents_required` と `fetch_contents_tolerant` は、S3 で上限の値を引数に取るようにする。

上限の値の決め方は次のとおりである。--force を持つコマンド（diff・merge・sync・rollback）は、そのコマンドのすべての読み込みに --force から決まる値を渡す。対象には、バックアップのための読み込み（merge の書き込み前、sync の --delete の削除前、rollback の復元前）と、merge の --ref のバッジの読み込み（`crates/remote-merge/src/cli/merge.rs` の三つの `fetch_contents_tolerant`）を含む。今の diff は --force を受け付けるが、読み込みには常に `false` を渡している（ヘルプも "No effect" と言う）。sync・merge の削除前のバックアップ（`crates/remote-merge/src/service/merge_flow.rs` の `execute_deletions` の `save_backup(..., false)`）と、rollback の復元前のバックアップ（`side_io.rs` の `restore_backup` の `save_backup_if_exists(..., false)`。`restore_backup` は `force` を受け取らない）は `false` に固定されている。これらを直す。status は REQ-cli-094 のとおり上限を当てない。そのため status の中身の読み込みは、明示的に「上限なし」を渡す。これをしないと、入口に置いた方針が、今は上限のない SSH とエージェントの status にまで上限を当ててしまい、REQ-cli-094 から遠ざかる。

status の中身の比べ方を、少しずつ読むハッシュに替えること（REQ-cli-095）は、この計画に入れない。理由は三つある。第一に、REQ-cli-094 が求めるのは上限を当てないことで、少しずつ読むことではない。第二に、ローカル・SSH・エージェントのそれぞれのハッシュの経路が要り、SSH の側は統合済みの ssh-strict-exec が変えた所にさらに手を入れる。第三に、上限を一つにすることとは独立に直せる。この計画の後、status はローカルの経路でも上限を超えるファイルの中身をメモリに読むようになる。今の SSH とエージェントの経路はすでにそうしている。`docs/decision/records/2026-10-05-size-limit.md#A12` のとおり、少しずつ読むハッシュはこの単位の直後の別の単位（status のハッシュの単位）で行う。

diff の出し方。上限を超えたファイルの出し方は、各コマンドの「読めなかったファイルの扱い」に従う（REQ-cli-079）。merge と sync では、読み込みの Err はすでに failed に入る（REQ-merge-017）。diff では、そうなっていない。名前で指定したファイルを両側とも読めないと、fast path（`crates/remote-merge/src/cli/diff.rs` の `run_diff_fast_path` と engine の `status_from_read_results`）は「どちらにもない」とする。一件ずつの比較（`read_file_bytes_tolerant` で読む本体の繰り返し）では、片側だけ読めないとその側を空として全行を差分にし（終了コード 1）、両側とも読めないと `has_read_error` を立てて終了コード 2 にするが、`DiffError` を積まないので errors にそのパスが出ない。走査の比較（`compute_statuses_and_resolve`）は、読めない側を空として判定する。一方、diff にはすでにトップレベルの `errors: Vec<DiffError>` と `has_read_error` があり、外部へのリンク、参照先の読めない symlink、循環、項目数の上限に使われている。S2 はこれを使い回す。この三つは REQ-cli-089（読めない側を空とみなさない）の直す範囲で、品質レビューの単位では T8 に入っている。しかし REQ-cli-089 の文は「REQ-cli-079 の大きさの上限を超えたときを含む」と書いている。EX-cli-081 も、diff が errors に出して終了コード 2 を返すことを求める。上限のエラーだけを特別に errors に出すと、T8 がすぐに一般の読み込みの失敗の扱いで置き換える道を作ることになる。そこでこの計画は、diff のこの三か所の中身の読み込みについて、読み込みの失敗を一般に errors に出す扱いを S2 で先に入れ、REQ-cli-089 と EX-cli-087 の印を付ける（T8 から diff-1・diff-5・diff-6 を引き取る）。status の REQ-cli-088 と、--ref の REQ-cli-090（`diff.rs` の参照先の読み込みの `.ok()`）は T8 に残す。この単位の切り方は `docs/decision/records/2026-10-05-size-limit.md#A9` で決まった。読めなかったファイルは JSON の files に項目として載せず errors だけに出し、summary の差分のあるファイルの数に入れない（`docs/decision/records/2026-10-05-size-limit.md#A10`）。これは `docs/decision/records/2026-10-05-size-limit.md#A15` により REQ-cli-089 の本文に書かれているので、S2 の印付きのテストは files と summary の扱いを確かめてよい。「ない」と「読めない」は、経路のエラーの文ではなく、パスの確認（`inspect_path`）の結果の `TargetPath::Missing` で見分ける（`docs/decision/records/2026-10-05-size-limit.md#A19`）。fast path はすでに `left_path`・`right_path` を持っている。走査の比較は、一括読み込みの結果にないパスを `unwrap_or_default` で空にせず、`tolerant_io.rs` の `fetch_contents_required` のように一件ずつ読み直して失敗を集める。テキストの出力では、`run_diff_with_targets`（`diff.rs`）が errors を標準エラーに出す（`docs/decision/records/2026-10-05-size-limit.md#A19`）。今は外部へのリンクや循環などの既存の errors もテキストでは出ないので、これらも同じく標準エラーに出るようになる。今の `read_file_bytes_tolerant` の "Warning ... (treating as empty)" の出力は消す。これに合わせて、バイナリの差分で "missing" とするのはない側だけになり、読めない側は errors に出て終了コード 2 になる（`docs/decision/records/2026-10-05-size-limit.md#A13` で改められた REQ-cli-061）。読めない側を "missing" と固定している印付きのテスト `crates/remote-merge/tests/contract/diff_file_kinds.rs` の `req_cli_061_the_unreadable_side_of_a_binary_is_missing` は S2 で書き換える。

テストの作り方。104,857,600 バイトを超えるファイルは、一時ディレクトリに `File::set_len`（または `truncate`）で作る疎なファイルにし、ディスクを実際には使わない。中身を違えるには、末尾の一バイトだけを書く。上限で読まないことの確かめは、中身を読まないので軽い。--force で実際に読むテスト（S3 の merge --force のガード、S4・S5・S6）は、CLI のプロセスに 200〜400MB のメモリを使い、SSH の試験サーバ越しに 100MB を超える中身を送る。EX-cli-083 と rollback のテストは、バックアップの保存先に約 100MB の実際の写しを書く。試験の SSH サーバは標準出力をメモリに持つので、実際のメモリはこの見積もりより大きくなる。この環境の WSL はメモリが尽きると全体が落ちるので、--force で読むテストは要件ごとに一つに絞り、テストは Test command のとおりメモリの上限で囲んで実行する。さらに `.config/nextest.toml` を新しく作り、`max-threads = 1` の test-group に --force で実際に読む重いテストを入れて、同時に走らないようにする。「読む前に大きさで決める」ことは、上限を超える疎なファイルのモードを 000 にして示す。大きさは調べられるが、読めば権限のエラーになる。報告される理由が権限ではなく上限であれば、読む前に判定した証拠になる（root で実行すると読めてしまい区別できないので、その場合はテストが前提の崩れを知らせる）。経路の再現には次を使う。`crates/remote-merge/tests/common/mod.rs` の `CliEnv` は、ローカルと、エージェントなしの SSH の試験サーバ（`TestServer::filesystem_without_agent`）に置いた `develop` の組である。エージェントは `crates/remote-merge/tests/contract/scan_listing_cli.rs` の `AgentFixture`（`diff_via_agent`・`diff_with_agent_enabled`・`status_via_agent`。merge と rollback は既存の `run_with_agent(local_root, remote_root, args)` で起動し、これはエージェントが動いたことと SSH のパスの調べが起きなかったことを確かめる）。ローカル同士の経路を CLI のプロセスで再現する手段はないので、ローカルの経路は `RuntimeTargets::with_local`（`crates/remote-merge/tests/contract/sync_support.rs` の使い方）を使うプロセス内のテストか、`side_io.rs` の単体テストで示す。新しい契約テストのファイルは、`crates/remote-merge/tests/contract.rs` の既存の行にならい、`#[cfg(feature = "test-utils")]` を付けた mod 行で足す。`.kotowari/config.yaml` の `tests.files` に入るのは `crates/remote-merge/tests/contract/**`、`crates/remote-merge/tests/cli_diff.rs`、`tests/container-e2e/tests/sudo.rs` で、この計画のテストは前の二つに置く。印（`@kotowari[...]`）はそこにあるテストにだけ付け、単体テストには付けない。

## Scope of change

- 新しい方針のモジュール一つ（`crates/remote-merge/src/runtime/` の下）と `crates/remote-merge/src/runtime/mod.rs` のその宣言
- `crates/remote-merge/src/runtime/side_io.rs`
  - 四つの読み込みの入口（どれも上限の値を受け取る）、新しいエージェントの大きさの補助関数（`with_agent` と `transform_stat_results` を通さない）、`restore_backup` への上限の値の受け渡し、その `mod tests`（入口の呼び出し。1129 行、1199 行、1237 行、1266 行の付近ほか）
- `crates/remote-merge/src/runtime/target_io.rs`
  - `TargetIo` の大きさを返す操作の追加と、`read_file_bytes` の `force` の除去
- `crates/remote-merge/src/runtime/remote_io.rs`
  - SSH の大きさのコマンド（`stat` の確かめ、`chunk_paths` による分け方、接続のエラーでの一度の再接続、見分けるための前置き）と出力の解析の純粋関数、`read_remote_file_bytes` の `force` と `validate_file_size` の除去だけ（`remove_remote_file`・`create_remote_symlink`・一括読み込みは変えない）
- `crates/remote-merge/src/runtime/remote_path/mod.rs` か大きさのコマンドを置くモジュール（大きさのコマンドの前置きを test-utils の機能のときだけ公開することだけ）
- `crates/remote-merge/src/runtime/core.rs`
  - `save_backup` と `save_backup_if_exists` の中で上限の値に変えることだけ
- `crates/remote-merge-engine/src/merge/executor.rs`
  - `read_local_file_bytes` の読み込み後の判定の除去と `MAX_BINARY_FILE_SIZE` の扱い、その `mod tests`
- `crates/remote-merge-engine/src/local_io.rs`
  - ローカルの大きさを返す自由関数と、`read_file_bytes`・`read_files_bytes_batch` の `force` の除去
- `crates/remote-merge-engine/src/service/types.rs`（diff の結果の `DiffError`・`MultiDiffOutput` に手を入れる場合だけ）
- `crates/remote-merge-engine/src/service/status.rs`（`status_from_read_results` だけ）
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
  - S2 の diff の読み込みの失敗のテストを置く既存の diff の契約テストのファイル（`crates/remote-merge/tests/contract/diff_*.rs` のうち一つ）か `crates/remote-merge/tests/cli_diff.rs`
  - `crates/remote-merge/tests/contract/diff_file_kinds.rs`（印付きのテスト `req_cli_061_the_unreadable_side_of_a_binary_is_missing` を `docs/decision/records/2026-10-05-size-limit.md#A13` のとおりに書き換えることだけ）
  - `crates/remote-merge/tests/contract/ssh_server.rs`（大きさのコマンドの前置きを見分けて答えることだけ）
- `.config/nextest.toml`（新規。--force で実際に読む重いテストの `max-threads = 1` の test-group だけ）
- 既存のテストの書き換え（直したものは名前と場所を報告に書く。上の二つは期待を変えない。三つ目は期待を変える）
  - 上限の判定の場所を固定していたもの（`crates/remote-merge-engine/src/merge/executor.rs` の `test_read_local_file_bytes_size_limit`。`side_io.rs` と `remote_io.rs` には上限のテストはない）
  - シグネチャの変更に伴う機械的な呼び出しの修正（`execute_deletions` を呼ぶ `crates/remote-merge/tests/contract/merge_paths.rs`・`backup_storage.rs`、`fetch_contents_*` や `read_files_*` を呼ぶもの）
  - S2 で、読めない側を空とする今の振る舞いを固定していたもの（期待を REQ-cli-089 に合わせて変える。REQ-cli-061 のテストは上の項目のとおり）
- `docs/decision/records/2026-10-05-size-limit.md`・`docs/ir/cli/binary.md`・`docs/ir/cli/unreadable.md`（計画の修正の時点で書かれており、計画の承認のときにメインのセッションがコミットする。S1 はコミットされていることを確かめるだけで、finding を直す場合だけ変える）
- `.kotowari/changes/implementation.yaml`（main にある古い `implementation.yaml` と `review.yaml` は除く）

## Step order and prerequisites

全体の前提は次のとおりである。ブランチ ssh-strict-exec（SSH の厳密な実行 `remote_merge_ssh::exec_strict`・`exec_strict_bytes` と `batch_read.rs` の一括読み込み）と path-validation（引数の検査、SSH の種類の調べ、rollback）は main に統合済みで、このブランチは統合後の main（80a3d7e）に rebase 済みである。この計画は SSH crate（`crates/remote-merge-ssh/`）を変えない。SSH の一括読み込みの形式は今の main のものをそのまま使う。今の main では、SSH の一括読み込みは読めなかったパスを結果から外し、空の中身として返さない。

S1 を最初に置く。S3 の大きさの判定のしかた、S2 で T8 から引き取る範囲と diff の出力の形の根拠が、そこで確かめる決定記録にあるからである。S2 は S3 より前に置く。S3 の EX-cli-081 のテストは、読み込みの失敗が errors に出ることを前提にするからである。S2 の後は、今の読み込み後の判定により EX-cli-081 のテストもすでに通る。そのため S3 の RED の証拠は、EX-cli-081 ではなく、エージェントの経路で片側だけ上限を超える diff、モード 000 の merge、エージェントが無効にされず SSH に切り替わらないことで示す。S2 は上限と独立に、モード 000 のファイルで示せる。S4（--force）、S5（バックアップ）、S6（status）は S3 の方針の上に作るので、S3 の後に行う。三つは互いに独立である。S6 は、S3 が status の SSH の経路に上限を当ててしまう期間を短くするため、S3 の直後に行ってもよい。S7 は最後に全体を確かめる。status を少しずつ読むハッシュで比べる単位（`docs/decision/records/2026-10-05-size-limit.md#A12`）は、この計画の後に続く。

## Verification map

| Step | Requirements | Examples | 印を付けるテスト |
|---|---|---|---|
| S1 | REQ-cli-079, REQ-cli-089, REQ-cli-094, REQ-cli-061 | | なし（決定記録と IR のコミットの確かめ） |
| S2 | REQ-cli-089, REQ-cli-061 | EX-cli-087 | 契約テスト一つ: `@kotowari[REQ-cli-089, EX-cli-087]`。既存の `@kotowari[REQ-cli-061]` のテスト `req_cli_061_the_unreadable_side_of_a_binary_is_missing` を `docs/decision/records/2026-10-05-size-limit.md#A13` のとおりに書き換える |
| S3 | REQ-cli-079, REQ-cli-074 | EX-cli-081 | 契約テスト: EX-cli-081 の SSH の組とエージェントの組（`@kotowari[EX-cli-081]`。S2 の後に通るガード）、エージェントの片側だけの diff とモード 000 の merge の SSH・エージェント（`@kotowari[REQ-cli-079]`）、merge --force のガード（`@kotowari[REQ-cli-074, REQ-cli-079]`） |
| S4 | REQ-cli-079 | EX-cli-082 | 契約テスト一つ: `@kotowari[EX-cli-082]` |
| S5 | REQ-cli-079 | EX-cli-083 | 契約テスト二つ: `@kotowari[EX-cli-083]`、rollback の `@kotowari[REQ-cli-079]` |
| S6 | REQ-cli-094 | | 契約テスト一つ: `@kotowari[REQ-cli-094]` |
| S7 | 上のすべて | | — |

EX-cli-081 のテストと merge --force のテストは、S3 で RED にならない。前者は S2 の後、今の読み込み後の判定ですでに通る。後者は今のローカルと SSH の経路が --force を尊重し、エージェントには上限がないのですでに通る。どちらも、S3 で判定の場所を付け替えた後も同じ結果が保たれることを守るガードとして、S3 の付け替えの前に書いて通ることを確かめる。EX-cli-081 と REQ-cli-074 の上限の文にはまだ印付きのテストがないので、印を付ける。S3 の RED → GREEN の証拠は、Verification map の他の三つのテストと、エージェントが無効にされず SSH に切り替わらないことである。

EX-cli-093（status で上限を超えるファイルを "modified" と報告する）には、この計画では印を付けない。今の status は読めなかったファイルをメタデータの判定のまま残す（status_scan-6）。そのため中身の違うファイルは、上限があってもなくても "modified" になり、このテストは今すでに通る。上限の有無を見分けられるのは、REQ-cli-088 で読めないファイルが errors に出るようになった後である。それは T8 が行う。S6 は代わりに、中身が同じで更新時刻の違うファイルで見分ける。上限があればメタデータの判定のまま "modified" に、なければ中身を比べて "equal" になる。S7 の後も `kotowari query EX-cli-093` の `tests` は空のままで、`kotowari check` は EX-cli-093 の scenario_without_test を出し続ける。これをこの計画の失敗としない。REQ-cli-095（少しずつ読むハッシュ）は review で確かめる要件で、Out of scope のとおりこの計画では満たさない。REQ-merge-017 は既存の印付きのテストのまま変えない。

## Left to the implementer

- 新しい方針のモジュールの名前と置き場所（`crates/remote-merge/src/runtime/` の下）。上限の値の型の名前と形（`force` の bool から作る列挙など）。型付きのエラーの作り方（`AppError` に足すか、方針のモジュールの独自の型か）。上限の定数の正本の場所（一つだけにし、`MAX_BINARY_FILE_SIZE` の既存の公開パスを残すかは任せる）
- 上限を超えたときのエラーの文。今の "File too large (N bytes > 104857600 bytes limit): パス. Use --force to override." を保つことを勧めるが、文はどの IR でも契約ではない。テストは文を固定せず、パスと終了コードと、権限の理由（"Permission denied"）を含まないことを確かめる
- 大きさを返す `TargetIo` の操作の名前と戻り値の形（パスごとの大きさの対応表など）。SSH のコマンドの区切りの印と失敗の印の書き方（`docs/decision/records/2026-10-05-size-limit.md#A8` の形の中で）。新しいエージェントの大きさの補助関数の名前
- diff の --force のヘルプの英語の文
- diff の errors の reason の文（側を含めるかを含む。テストは固定しない）
- テストの補助関数（疎なファイルを作る関数など）の置き場所。新しい契約テストのファイルの中か `crates/remote-merge/tests/common/`

## Stop conditions

- エージェントの経路で読む前に大きさを知るのに、通信の形式（`crates/remote-merge-protocol`）かエージェントの crate を変える必要がある。配置済みのエージェントの入れ替えとリリースが要るので、利用者が決める
- 対応するリモート（CentOS 5 と BusyBox を含む）の `stat` が `-L` か `%s` を受け付けない。実装者は Docker の e2e のコンテナ（Alpine の BusyBox）で一度確かめる
- 中身の読み込みのうち、`CoreRuntime` の四つの入口を通らないものが見つかる。ただし、status のハッシュの経路（`hashes`）と、ローカルのバックアップの保存先からの読み込み（`crates/remote-merge-engine/src/backup_store.rs` の `read_record`）は Out of scope のとおり除く
- 既存のテストが、上限を超えるファイルについて、SSH の一括読み込みやエージェントの経路で読めることを期待している
- REQ-ssh-003 か EX-ssh-* の印付きのテストの期待を変えないと、大きさのコマンドを足した後に通らない
- --force で実際に読むテストが、この環境でメモリ不足か時間切れを起こす

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

この環境はメモリが尽きると全体が落ちるので、メモリの上限で囲む。`--all-features` は test-utils の契約テストに要る。

## Out of scope

- status を少しずつ読むハッシュで比べること（`docs/ir/cli/status.md#REQ-cli-095`）。`docs/decision/records/2026-10-05-size-limit.md#A12` のとおり、この単位の直後の別の単位で行う
- status の読めないファイルの出し方（`docs/ir/cli/unreadable.md#REQ-cli-088`）と、diff の --ref の参照先の読み込みの失敗（`docs/ir/cli/unreadable.md#REQ-cli-090`）。どちらも T8 に残す。テキストの読み込みには今はどの経路にも上限がないので、上限を超えた --ref の参照先が「参照先にない」とされるようになるのは S3 の後である
- エージェントが一括読み込みの結果をすべてメモリに持つこと（routes-15 の後半）と、ReadFiles のフレームが 16MB を超えうること。上限でファイル一つの大きさは抑えられるが、一括の大きさは T2・T13 で扱う
- エージェントのテキストの読み込みが UTF-8 として不正なバイトでエラーになること（routes-10 の後半。ssh_agent-14 と同じ原因）
- rollback がローカルのバックアップの保存先から写しを読むこと（`crates/remote-merge-engine/src/backup_store.rs` の `read_record`）。`docs/decision/records/2026-10-05-size-limit.md#A7` のとおり上限を当てない
- merge の --ref の三者の確認（`crates/remote-merge/src/cli/merge.rs` の三つの `read_file_bytes`）の失敗の出し方（`docs/decision/records/2026-10-05-quality-cli.md#A9` の "three-way comparison incomplete"）。上限の値は S3 の方針を通るが、出し方は T10 で扱う
- 大きさを調べてから読むまでの間にファイルが大きくなった場合の防御
- SSH の crate（`crates/remote-merge-ssh/`）とエージェントの crate の変更

## Steps

### S1: 計画の修正の時点で書かれた決定記録と IR がコミットされていることを確かめる

- Purpose: この計画の扱いの根拠である `docs/decision/records/2026-10-05-size-limit.md`（A1〜A19）と、それに合わせて改めた `docs/ir/cli/binary.md`（REQ-cli-061。A13）と `docs/ir/cli/unreadable.md`（REQ-cli-089。A15）が、ブランチにコミットされていて `kotowari check` に通ることを、実装に入る前に確かめる。この三つは計画の承認のときにメインのセッションがコミットするので、S1 はコミットしない
- Specification: `docs/ir/cli/size-limit.md#REQ-cli-079`, `docs/ir/cli/unreadable.md#REQ-cli-089`, `docs/ir/cli/status.md#REQ-cli-094`, `docs/ir/cli/binary.md#REQ-cli-061`
- Prerequisites: none
- May change: `docs/decision/records/2026-10-05-size-limit.md`, `docs/ir/cli/binary.md`, `docs/ir/cli/unreadable.md`（どれも `kotowari check` の finding を直す場合だけ。A 項目の決定と要件の中身は変えない）
- Done when: 三つのファイルがコミット済みで、`git status --short` に出ない。決定記録が A1〜A19 を持つ。REQ-cli-089 の本文が、読めなかったファイルを files に載せず summary の差分のあるファイルの数に入れないことを書いている。`kotowari check --format json` がこの三つのファイルに finding を出さない
- Shown by: check — `git status --short` と `git log` でコミット済みであること、`kotowari check --format json` でこの三つのファイルへの finding がないこと
- Left to the implementer: none
- Stop and hand back if: 三つのファイルのどれかがコミットされていない。決定記録に A1〜A19 のどれかがない。または、finding を直すのに A 項目の決定か要件の中身を変える必要がある

### S2: diff が中身を読めないファイルを空とみなさず errors に出す

- Purpose: diff の三か所の中身の読み込み（fast path の読み込み、走査の中身の比較、一件ずつの比較）で、読み込みの失敗を空の中身や「ない」として扱わず、そのパスと理由を JSON のトップレベルの errors（テキストでは標準エラー）に出し、他のファイルの比較を続けて終了コード 2 を返す。読めなかったファイルは JSON の files に載せず、summary の差分のあるファイルの数に入れない（REQ-cli-089 の本文。`docs/decision/records/2026-10-05-size-limit.md#A10`・`docs/decision/records/2026-10-05-size-limit.md#A15`）。errors と終了コードは、diff にすでにあるトップレベルの `errors: Vec<DiffError>` と `has_read_error` を使い回す。「ない」と「読めない」は、エラーの文ではなく `inspect_path` の結果の `TargetPath::Missing` で見分ける（`docs/decision/records/2026-10-05-size-limit.md#A19`。fast path はすでに `left_path`・`right_path` を持つ）。走査の比較は、一括読み込みの結果にないパスを `unwrap_or_default` で空にせず、`fetch_contents_required`（`tolerant_io.rs`）のように一件ずつ読み直して失敗を集める。テキストの出力では、`run_diff_with_targets`（`diff.rs`）が errors を標準エラーに出し（`docs/decision/records/2026-10-05-size-limit.md#A19`）、外部へのリンクや循環などの既存の errors も同じく標準エラーに出るようになる。`read_file_bytes_tolerant` の "Warning ... (treating as empty)" の出力は消す。バイナリの差分で読めない側を "missing" とせず errors に出す（`docs/decision/records/2026-10-05-size-limit.md#A13`）ので、それを固定している印付きのテストを書き換える。ない側を "missing" と出す REQ-cli-061 の表示は残す
- Specification: `docs/ir/cli/unreadable.md#REQ-cli-089`, `docs/ir/cli/binary.md#REQ-cli-061`
- Prerequisites: S1
- May change: `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge/src/cli/tolerant_io.rs`, `crates/remote-merge-engine/src/service/types.rs`（走査の段階の errors を出力へ運ぶ場合だけ）, `crates/remote-merge-engine/src/service/status.rs`（`status_from_read_results` だけ）, 既存の diff の契約テストのファイル一つか `crates/remote-merge/tests/cli_diff.rs`, `crates/remote-merge/tests/contract/diff_file_kinds.rs`（`req_cli_061_the_unreadable_side_of_a_binary_is_missing` の書き換えだけ）, 読めない側を空とする今の振る舞いを固定している既存のテスト（直したものは名前と場所を報告に書く）
- Done when: `CliEnv`（ローカルと SSH の `develop`）で、左右にある `a.txt` のうちリモートの側のモードを 000 にし、`b.txt` は左右で中身が違う状態にする。`diff a.txt b.txt --left local --right develop --format json` を実行すると次のようになる。終了コードは 2 である。トップレベルの errors に `a.txt` とその理由がある。`a.txt` の全ての行が差分として出ない。`a.txt` は files に載らず、summary の差分のあるファイルの数に入らない。`b.txt` の差分は出る。パスを指定しない diff（走査の比較）でも同じ状態で、`a.txt` が "equal" や空との差分として扱われず、errors に出る。左右の `a.txt` をともにモード 000 にし、大きさを同じに、更新時刻を違えた状態で、パスを指定しない diff（走査の比較）を実行すると、`a.txt` が errors に出る（"equal" にならない）。テキストの出力では、`a.txt` の理由が標準エラーに出て、"(treating as empty)" の警告は出ない。`a.txt` の左右がともに読めないとき、"not found on either side" ではなく errors に `a.txt` が出る（いまは本体の繰り返しが `has_read_error` を立てて終了コード 2 にするが、`DiffError` を積まないので errors に出ない）。片側だけ読めないときは、いまは読めない側を空として全行の差分と終了コード 1 になる。書き換えた `req_cli_061_the_unreadable_side_of_a_binary_is_missing` が、バイナリの読めない側について、files に載らず errors に出て終了コード 2 になることを確かめて通る
- Shown by: test — 契約テスト一つ（`// @kotowari[REQ-cli-089, EX-cli-087]`）で、名前を指定した diff の JSON の errors と終了コード（REQ-cli-089 の本文にあるので、files と summary の扱いも確かめてよい）を RED → GREEN にする。パスを指定しない diff（両側ともモード 000 で更新時刻の違う場合を含む）と、名前を指定して両側とも読めない場合は、同じ契約テストのファイルの印のないテストか単体テストで示す。書き換えた `// @kotowari[REQ-cli-061]` のテストが、書き換えの後に今のコードで RED になり、S2 の変更で GREEN になる。root で実行して読めてしまうときは、前提の崩れを知らせる
- Left to the implementer: 走査の段階で集めた読み込みの失敗を出力の errors へ運ぶ形。三か所の共通部分の切り出し方。`read_file_bytes_tolerant` の改名か削除。書き換えた REQ-cli-061 のテストの名前
- Stop and hand back if: 読めないファイルを errors に出すと、REQ-cli-022（symlink の参照先）や REQ-cli-063（--ref）の既存の印付きのテストの期待と食い違う。または、ファイルが「ない」ことと読めないことを、diff のどこかの経路で見分ける情報がない

### S3: 一つの上限を読む前にすべての経路で当てる

- Purpose: `CoreRuntime` の四つの読み込みの入口がどれも上限の値を受け取り、「上限あり」のときだけ経路に大きさを尋ねて上限を超えるパスを読まずに型付きのエラーにし（一件ずつの読み込みは Err、一括読み込みは結果から外して Ok）、「上限なし」のときは大きさを尋ねずにそのまま読ませる（`docs/decision/records/2026-10-05-size-limit.md#A17`）。経路の側の読み込み後の判定と `TargetIo::read_file_bytes` の `force` を消す。各経路は、symlink を辿った先の大きさを返す操作を持つ。ローカルは `metadata` を使う。SSH は `docs/decision/records/2026-10-05-size-limit.md#A8`・`docs/decision/records/2026-10-05-size-limit.md#A18` の形のコマンドを `remote_merge_ssh::exec_strict_bytes` か `exec_strict` で実行する。このコマンドは、最初に既知のパスで `stat` が使え `-L` と `%s` を受け付けることを確かめてだめなら 0 以外で終え、尋ねたパスの順番に大きさかパスごとの失敗の印を区切りの印で返し、コマンド全体の失敗を終了コードで返す（中でパスごとに `stat -L -c %s` を使ってよい）。終了コード 0 でも解析できない出力は操作の失敗とする。尋ねるパスは `remote_merge_ssh::batch_read` の `chunk_paths` と `SSH_BATCH_MAX_COMMAND_LEN` で分ける。接続のエラーでは、`remote_io.rs` の他の読み込みと同じく一度だけ再接続して再試行する。コマンドの頭に見分けるための前置きを付けて test-utils の機能のときだけ公開し、試験の SSH サーバ（`ssh_server.rs`）がそれに答えるようにする。エージェントは既存の `StatFiles` を、`side_io.rs` の新しいエージェントの大きさの補助関数から `AGENT_READ_BATCH_SIZE` 件ごとに呼ぶ。この補助関数は `with_agent` と `transform_stat_results` を通さず、結果をパスで突き合わせる（`docs/decision/records/2026-10-05-size-limit.md#A11`）。エージェントが返したエラーは尋ねたパスすべての読み込みのエラーにし、接続が切れたエラー（`should_invalidate_agent_error` が真）はエージェントを無効にして SSH の大きさのコマンドに切り替える（`docs/decision/records/2026-10-05-size-limit.md#A16`）。エージェントが結果から外したパスは SSH の大きさのコマンドに尋ね直し、sudo を使うサーバでは `check_sudo_fallback` のとおりエラーにする（`docs/decision/records/2026-10-05-size-limit.md#A14`）。`fetch_contents_required` と `fetch_contents_tolerant` は上限の値を引数に取り、今の呼び出し元は今と同じ値（`fetch_contents_required` は呼び出し元の force、`fetch_contents_tolerant` は上限あり）を渡す
- Specification: `docs/ir/cli/size-limit.md#REQ-cli-079`, `docs/ir/cli/merge.md#REQ-cli-074`
- Prerequisites: S1, S2
- May change: 新しい方針のモジュールと `crates/remote-merge/src/runtime/mod.rs`, `crates/remote-merge/src/runtime/side_io.rs`（四つの入口、新しいエージェントの大きさの補助関数、`mod tests` の入口の呼び出し。エージェントの crate と通信の形式は変えない。`StatFiles` は `validate_path` の後の大きさをすでに返す）, `crates/remote-merge/src/runtime/target_io.rs`, `crates/remote-merge/src/runtime/remote_io.rs`, `crates/remote-merge/src/runtime/remote_path/mod.rs`（前置きの公開を並べる場合だけ）, `crates/remote-merge/src/runtime/core.rs`, `crates/remote-merge-engine/src/merge/executor.rs`, `crates/remote-merge-engine/src/local_io.rs`, `crates/remote-merge/src/merge/executor.rs`, `crates/remote-merge/src/cli/tolerant_io.rs`, `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge/src/cli/merge.rs`, `crates/remote-merge/src/cli/sync.rs`, `crates/remote-merge/src/cli/status.rs`（`fetch_contents_tolerant` の引数を足すことだけ）, `crates/remote-merge/src/service/merge_flow.rs`, `crates/remote-merge/tests/contract/ssh_server.rs`（大きさのコマンドの前置きに答えることだけ）, `.config/nextest.toml`（新規。merge --force のガードを `max-threads = 1` の test-group に入れる）, `crates/remote-merge/tests/contract/` の新しい契約テストのファイルと `crates/remote-merge/tests/contract.rs` の mod 行, Scope of change の既存のテストの書き換え
- Done when: 次の十が成り立つ。(1) 方針の純粋関数の単体テストで、大きさ 104,857,600 のパスは読む対象に入り、104,857,601 のパスは上限のエラーになり、大きさの分からないパスは読む対象に入る。「上限なし」では大きさを尋ねず、すべてのパスが読む対象に入る。(2) `CoreRuntime` の単体テスト（ローカルの経路）で、モード 000 の 104,857,601 バイトの疎なファイルの `read_file_bytes`・`read_files_bytes_batch`・`read_file` が上限のエラーを返すか、結果に入れない。対照として、モード 000 のちょうど 104,857,600 バイトの疎なファイルは権限のエラーになり、104,857,601 バイトのものは権限のエラーにならない（エラーの文は固定しない）。大きさを尋ねる操作そのものが失敗すると読み込みのエラーになり、上限を外して読まない。(3) `AgentFixture` で、ローカルに小さい `big.bin`、リモートに 104,857,601 バイトの疎な `big.bin` を置き、`diff big.bin --format json` を --force なしに実行すると、終了コード 2 で、トップレベルの errors に `big.bin` が出る。いまはエージェントが読めてしまい、バイナリの差分と終了コード 1 になる。(4) ローカルに小さい `big.bin`、リモートにモード 000 の 104,857,601 バイトの疎な `big.bin` を置き、`merge big.bin --left local --right <サーバ> --format json` を実行すると、`CliEnv`（SSH）の組と `AgentFixture` の組（既存の `run_with_agent` で起動する）のどちらでも、終了コード 2 で、failed の `big.bin` の error の "right:" の理由が上限の理由で、"Permission denied" を含まない。リモートの中身とモードは変わらない。(5) (3) と (4) のエージェントの組で、エージェントが無効にされず、大きさの問い合わせも読み込みも SSH に切り替わらない（`run_with_agent` の確かめと、試験サーバが受けたコマンドか診断ログで示す）。いまは、一件ずつの読み込み `try_agent_read_file_bytes` は `FileReadResult::Error` でエージェントを無効にせず None を返して SSH の読み込みに切り替わり、一括読み込み `try_agent_read_files_bytes_batch` だけが失敗でエージェントを無効にする。(6) エージェントの大きさの補助関数の単体テストで、エージェントが返したエラーでは尋ねたパスすべてが読み込みのエラーになり SSH に切り替わらず上限も外れない。接続が切れたエラーではエージェントが無効になり SSH の大きさのコマンドに切り替わる。エージェントが結果から外したパスは SSH の大きさのコマンドに尋ね直され、sudo を使うサーバではエラーになる。尋ねるパスは `AGENT_READ_BATCH_SIZE` 件ごとに分かれる。(7) SSH の大きさのコマンドについて、出力の解析の純粋関数の単体テストで、大きさ、パスごとの失敗の印、区切りの崩れ、終了コード 0 で解析できない出力（操作の失敗になる）を確かめる。コマンドを組み立てる関数の単体テストで、`chunk_paths` と `SSH_BATCH_MAX_COMMAND_LEN` によりパスが分かれ、どのコマンドも頭に前置きと `stat` の確かめを持つ。試験の SSH サーバで、大きさのコマンドが接続のエラーで一度だけ再接続して再試行されることを示す。`ssh_reconnect.rs` の REQ-ssh-003 の印付きのテスト（EX-ssh-005・EX-ssh-006・EX-ssh-008、interrupt_first_read・interrupt_all_reads）は期待を変えずに通る。(8) ガードとして、S3 の付け替えの前に書いて通ることを確かめた二つが付け替えの後も通る。一つは EX-cli-081 のとおり、左右に中身の違う 104,857,601 バイトの疎なファイル `big.bin` がある状態で `diff big.bin --format json` を --force なしに実行すると、`CliEnv` の組でも `AgentFixture` の組でも、終了コード 2 で errors に `big.bin` が出ること。もう一つは `CliEnv` の組で、左にだけ 104,857,601 バイトの疎な `big.bin` がある状態の `merge big.bin --left local --right develop --force` が終了コード 0 で、書き込み先の `big.bin` が左と同じ大きさと末尾の一バイトになり、--force なしでは failed になること。(9) diff の `reads_as_file`（一覧の取れない側を中身が読めるかで通常ファイルとみなす判定）は、上限のエラーを「通常ファイルである」とみなし、上限を超えるファイルをディレクトリ扱いにしない。左に `d/x`、右に 104,857,601 バイトの疎なファイル `d` を置いた diff が "not a directory" で止まらない。(10) 既存のテストは、Scope of change の書き換えをしたうえで通る
- Shown by: test — 方針の純粋関数、`CoreRuntime` のローカルの経路、エージェントの大きさの補助関数、SSH の大きさのコマンドの組み立てと解析の単体テスト（印なし）を RED → GREEN にする。契約テストでは、(3)（`// @kotowari[REQ-cli-079]`）と (4) の SSH の組とエージェントの組（それぞれ `// @kotowari[REQ-cli-079]`）を RED → GREEN にし、(5) は (3) か (4) のエージェントの組の中で確かめる。(7) の再接続は試験の SSH サーバを使う印のないテストで示す。(8) のガードは EX-cli-081 の二つ（それぞれ `// @kotowari[EX-cli-081]`）と merge --force の一つ（`// @kotowari[REQ-cli-074, REQ-cli-079]`）で、付け替えの前と後の両方で通ることを示す。(9) は印のない契約テストで示す。モード 000 のテストは root で実行して読めてしまうとき前提の崩れを知らせる
- Left to the implementer: 計画全体の「Left to the implementer」のうち、方針のモジュール・型・エラー・大きさの操作の形。大きさのコマンドの前置きの文字列と置き場所。`stat` の確かめに使う既知のパス。`reads_as_file` で上限のエラーを見分ける方法。一括読み込みで、上限を超えるパスを除いた残りを一度に読むか、経路の一括読み込みに任せるか（どちらでも、上限を超えるパスは経路に読ませない）
- Stop and hand back if: 全体の Stop conditions のエージェントの通信の形式、`stat -L` と `%s`（Docker の e2e のコンテナの BusyBox で一度確かめる）、四つの入口を通らない読み込み、REQ-ssh-003 か EX-ssh-* の印付きのテストの期待の変更のどれかに当たる。または、エージェントの `StatFiles` が symlink のパスに対してリンク先ではなくリンクそのものの大きさを返す。または (3) の今の結果が、エージェントが読めてしまう以外の理由で終了コード 2 になり、RED を作れない

### S4: --force で diff の上限を外す

- Purpose: diff の --force をすべての読み込み（比べる左右、走査の比較、--ref の参照先）に渡して上限を外し、ヘルプの "No effect" の文を改める。merge の --ref のバッジの読み込み（`merge.rs` の三つの `fetch_contents_tolerant`）にも merge の --force から決まる値を渡す
- Specification: `docs/ir/cli/size-limit.md#REQ-cli-079`
- Prerequisites: S3
- May change: `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge/src/cli/merge.rs`（--ref のバッジの三つの読み込みだけ）, `crates/remote-merge/src/cli/tolerant_io.rs`, `crates/remote-merge/src/main.rs`（diff の --force のヘルプの文だけ）, S3 の新しい契約テストのファイル, `.config/nextest.toml`（このテストを `max-threads = 1` の test-group に入れることだけ）
- Done when: EX-cli-082 のとおり、左右に中身の違う 104,857,601 バイトの疎なファイル `big.bin` がある `CliEnv` の組で、`diff big.bin --force --format json` を実行すると次のようになる。終了コードは 1 である。errors は空である。`big.bin` の項目が、左右の中身を比べた結果（バイナリとして左右の違うハッシュ）を持つ。いまは両側の読み込みが上限で失敗する。`remote-merge diff --help` の --force の説明が "No effect" でなくなっている。merge の --ref のバッジの読み込みが merge の --force の値を受け取ることは、コードの読み合わせだけで確かめる（契約テストも単体テストも足さない）
- Shown by: test — 契約テスト一つ（`// @kotowari[EX-cli-082]`）で RED → GREEN にする。merge の --ref のバッジの読み込みへの --force の受け渡しは、テストではなくコードの読み合わせだけで示す
- Left to the implementer: ヘルプの英語の文
- Stop and hand back if: diff の --force に上限以外の効き目を求める既存のテストか IR の要件が見つかる

### S5: バックアップのための読み込みに --force を届ける

- Purpose: sync・merge の --delete の削除前のバックアップ（`execute_deletions` の `save_backup(..., false)`）と、rollback の復元前のバックアップ（`restore_backup` の `save_backup_if_exists(..., false)`）に、そのコマンドの --force から決まる値を渡す
- Specification: `docs/ir/cli/size-limit.md#REQ-cli-079`
- Prerequisites: S3
- May change: `crates/remote-merge/src/service/merge_flow.rs`, `crates/remote-merge/src/cli/merge.rs`, `crates/remote-merge/src/cli/sync.rs`, `crates/remote-merge/src/runtime/side_io.rs`（`restore_backup` とその `mod tests`）, `crates/remote-merge/src/cli/rollback.rs`, S3 の新しい契約テストのファイル, `.config/nextest.toml`（この二つのテストを `max-threads = 1` の test-group に入れることだけ）, Scope of change の既存のテストの書き換え（`execute_deletions` と `restore_backup` の呼び出し）
- Done when: 次の二つが成り立つ。(1) EX-cli-083 のとおり、バックアップが有効で、書き込み先にだけ 104,857,601 バイトの疎なファイル `big.bin` がある `CliEnv` の組で、`sync . --left local --right develop --delete --force --format json` を実行すると、終了コード 0 で、書き込み先の `big.bin` が消え、バックアップの保存先にその写しがある。写しは、`XDG_DATA_HOME/remote-merge/backups/` の下を辿って大きさ 104,857,601 の通常ファイルがあることで確かめ、`record.json` や中身のファイルの名前には頼らない。(2) バックアップが有効な `CliEnv` の組で、小さい `a.txt` の merge でバックアップのセッションを作る。その後、書き込み先の `a.txt` を 104,857,601 バイトの疎なファイルに替える。`rollback --target develop --force --format json` を実行すると、終了コード 0 で、`a.txt` が戻り、restored の `a.txt` の `pre_rollback_backup` にセッションがある。いまはどちらも "backup failed" で failed になる
- Shown by: test — 契約テスト二つで RED → GREEN にする。sync の削除（`// @kotowari[EX-cli-083]`）と rollback（`// @kotowari[REQ-cli-079]`）である
- Left to the implementer: `execute_deletions` と `restore_backup` へ上限の値を渡す引数の形
- Stop and hand back if: rollback の --force の既存の効き目（確認のプロンプトを省くことと、期限切れのセッションの復元を許すこと）を固定している既存のテストか IR の要件と、上限を外すことが食い違う。または、今の main の `restore_backup` の構造が、復元前のバックアップを別の関数で行っている（その場合はその関数に渡す。渡し先が二つ以上に分かれるなら返す）

### S6: status に上限を当てない

- Purpose: status の中身の読み込み（`fetch_contents_tolerant` の三つの呼び出し）に「上限なし」を渡し、どの経路でも 104,857,600 バイトを超えるファイルの中身を比べて判定する
- Specification: `docs/ir/cli/status.md#REQ-cli-094`
- Prerequisites: S3
- May change: `crates/remote-merge/src/cli/status.rs`, S3 の新しい契約テストのファイル, `.config/nextest.toml`（このテストを `max-threads = 1` の test-group に入れることだけ）
- Done when: `CliEnv` の組（エージェントなし。ハッシュの比較が使われず中身を読む組）で、左右に中身が同じで大きさが同じ 104,857,601 バイトの疎なファイル `big.bin` を、更新時刻を違えて置く。`status --format json` を実行すると、`big.bin` が "equal" と報告される。いまはローカルの一括読み込みが上限で読めず、メタデータの判定のまま "modified" になる
- Shown by: test — 契約テスト一つ（`// @kotowari[REQ-cli-094]`）で RED → GREEN にする。EX-cli-093 には印を付けない（Verification map の理由）
- Left to the implementer: none
- Stop and hand back if: この組の status がハッシュの比較で決まり、中身を読まない（その場合は今すでに上限がなく RED にならない）。または、SSH で 100MB を超える中身を読むテストがこの環境で時間切れかメモリ不足になる

### S7: 全体の確認と変更の記録

- Purpose: この計画の変更がすべての確認を通り、比較全体の実装者の記録があることを示す
- Specification: `docs/ir/cli/size-limit.md#REQ-cli-079`, `docs/ir/cli/merge.md#REQ-cli-074`, `docs/ir/cli/status.md#REQ-cli-094`, `docs/ir/cli/unreadable.md#REQ-cli-089`, `docs/ir/cli/binary.md#REQ-cli-061`
- Prerequisites: S1, S2, S3, S4, S5, S6
- May change: `.kotowari/changes/implementation.yaml`, `.kotowari/changes/review.yaml`（main から引き継いだ古いものの削除だけ）、および S1〜S6 の May change のファイルの整形と clippy の指摘の修正
- Done when: `cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features`、`kotowari changes --base <比較元の完全な ID> --staged --phase implementation --format json` が成功する。`kotowari query` で、REQ-cli-079・REQ-cli-074・REQ-cli-094・REQ-cli-089・REQ-cli-061・EX-cli-081・EX-cli-082・EX-cli-083・EX-cli-087 の tests に S2〜S6 の契約テストがある。`kotowari check --format json` は終了コード 0 を求めない（main の時点ですでに他の error がある）。このブランチで変えたファイルと、この計画の ID に finding がないことを求める。ただし EX-cli-093 の scenario_without_test は Verification map のとおり残ってよい。main から引き継いだ古い比較の記録が除かれている。新しい `implementation.yaml` が、比較元（`git merge-base main HEAD` の完全なコミット ID）からこのブランチの変更をすべて覆う。`docs/decision/records/2026-10-05-size-limit.md` の決定を使う変更のうち IR に書かれていないもの（大きさを調べられないときの扱い、一括読み込みの契約、ローカルのバックアップの保存先の除外、SSH の大きさのコマンドの確かめと分け方と再試行）は、gap（disposition recorded、`docs/decision/records/2026-10-05-size-limit.md` と REQ-cli-079 の IR を参照）として記録する。diff の読めないファイルの files と summary での扱いは REQ-cli-089 の本文にある（`docs/decision/records/2026-10-05-size-limit.md#A15`）ので gap にしない。他は、既存の要件と IR を参照する existing として記録する。`kotowari check` が、このブランチで変えていないファイルの error を出しても直さず、報告に書く。レビューの記録 `review.yaml` は、実装と別のレビュー担当が作る
- Shown by: check — `cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features`、`kotowari check --format json`（上の範囲の finding だけを見る）、`kotowari changes --base <比較元の完全な ID> --staged --phase implementation --format json`。統合の前には、独立したレビューの記録を足した後で `kotowari changes --base <比較元の完全な ID> --head <対象の完全な ID> --phase review --format json` が終了コード 0 になること
- Left to the implementer: 記録の entry の分け方と ID
- Stop and hand back if: このブランチで変えたファイルか、この計画の ID（EX-cli-093 の scenario_without_test を除く）に、直し方の分からない kotowari check の finding が残る
