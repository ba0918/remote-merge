# 計画: SSH の経路で失敗を成功として扱わない

## Goal

利用者は、リモートの削除（Agent の有無によらず SSH で行う）と、SSH の経路（Agent を使わない経路）の symlink の作成・書き込み・一括読み込み・走査の起点の確認が失敗したとき、それを成功・空の中身・「ファイルがない」として扱わず失敗として報告し、ローカル・SSH・Agent のどの経路でも最後の要素が既存の symlink である書き込み先のリンク先を書き換えず、比べた後に消えていた削除の対象を failed に出す remote-merge を得る。

## Specification

IR は `docs/ir/`。この計画は品質レビュー 1 回目の直す単位 T1（`.agents/artifacts/quality/round1/ledger.json` の theme "T1"：routes-2・routes-3・routes-6・status_scan-7・merge_sync-1・merge_sync-2・merge_sync-3・ssh_agent-4・ssh_agent-5・ssh_agent-6・ssh_agent-12。根拠の詳細は同じディレクトリの routes.json・status_scan.json・merge_sync.json・ssh_agent.json の同じ番号の項目）を、次の要件に合わせて直す。

- `docs/ir/merge/deletion.md#REQ-merge-034`
- `docs/ir/merge/symlink.md#REQ-merge-001`、`docs/ir/merge/symlink.md#REQ-merge-003`、`docs/ir/merge/symlink.md#REQ-merge-004`（ローカルと SSH の書き込みで変えない扱いとしてだけ参照する）、`docs/ir/merge/symlink.md#REQ-merge-023`
- `docs/ir/merge/read-failure.md#REQ-merge-017`、`docs/ir/merge/read-failure.md#REQ-merge-019`
- `docs/ir/ssh/reconnect.md#REQ-ssh-003`
- `docs/ir/scan/listing.md#REQ-scan-015`
- `docs/ir/merge/concurrency.md#REQ-merge-033`（削除の対象が比べた後に消えていたときの扱いの根拠としてだけ参照する。この計画はこの要件の印を足さない）
- 決定: `docs/decision/records/2026-10-05-quality-ssh.md#A3`、`docs/decision/records/2026-10-05-quality-scan.md#A5`、ブランチ harden-security が足す `docs/decision/records/2026-10-05-harden-security.md#A6`（main への統合後に存在する）

## Approach and why

品質レビューの T1 の原因は四つに分かれる。どれも SSH crate（`crates/remote-merge-ssh/src/`）と製品側の SSH 呼び出し（`crates/remote-merge/src/runtime/remote_io.rs`）の中で閉じ、呼び出し元の service の判定は変えない。これに加えて利用者が 2026-10-05 に決めた四つ（SSH の書き込み、SSH の symlink の作成先のディレクトリ、ローカルの書き込み、消えていた削除の対象。下の段落）を直す。ローカルの書き込みだけは engine（`crates/remote-merge-engine/src/merge/executor.rs`）を変える。失敗が呼び出し元に Err として届けば、既存の service がすでに要件どおりに failed へ出す（ローカルの経路はそうなっている）からである。

- 終了コードを捨てる実行（routes-2・merge_sync-1・merge_sync-2・ssh_agent-6）: `SshClient::exec` は 0 以外の終了コードを debug ログに残すだけで成功を返す。リモートの `rm -f`（削除）と `ln -sfn`（symlink の作成）がこれを使うので、失敗が "ok" になる。この二つを終了コードを検査する実行（`exec_strict`。製品側からは `crate::ssh::exec_strict` の委譲関数）に替える。`chmod` はすでに `exec_strict` を使っている。`exec` 自体は改名も削除もしない。Agent の配置（`core.rs` の uname・版・id・配置前の確認）、`stat_remote_files` の `|| true`、`list_dir_with_limit` が、意図して終了コードを見ない実行に頼っており、配置の部分は harden-security が変えた範囲でもあるためである。symlink の作成は、ローカルと Agent と同じく先に親ディレクトリを作る。親の作成には SSH の書き込みが使う既存の `build_mkdir_command`（`mkdir -p`）を使い、その失敗も検査する。
- 終了ステータスのない出力を受け入れる実行（ssh_agent-5 の後半）: バイト列版の `run_command_bytes` は、終了ステータスを受け取らずにチャネルが終わっても Ok を返すため、途中で切れた出力が成功になり、`is_connection_error` に当たらないので REQ-ssh-003 の再接続と再試行が働かない。文字列版の `run_command` と同じく `AppError::SshConnection` にする。一括読み込みの呼び出し元（`read_remote_files_batch*`）はすでに接続エラーで一度だけ再接続して再試行する。
- 一括読み込みの形式（routes-3・status_scan-7・merge_sync-3・ssh_agent-4・ssh_agent-5 の前半）: `cat p 2>/dev/null` と行頭の区切り文字の形式では、ないファイル・読めないファイル・ディレクトリが空の中身になり、区切り文字に似た行を含むファイルで全体が空の結果になる。各ファイルの終了状態とバイト数を出す、中身と衝突しない形式に替える。読めなかったパスは結果に入れない。結果にないパスは、`fetch_contents_required`（`crates/remote-merge/src/cli/tolerant_io.rs`）が一件ずつの厳密な読み込みでやり直してその理由を errors に残す。中身を読み比べる対象のパス（`crates/remote-merge-engine/src/service/status.rs` の `verified_content_pairs` が扱うもの。明示したパスはこれに入る）では、merge と sync は REQ-merge-017・REQ-merge-019 の "read failed: right: 原因" を出す。書き込み先の事前の読み込み（`crates/remote-merge/src/cli/merge.rs` の `fetch_contents_required(&pair.right, ...)`）の失敗は今の文言のまま failed に入り、この計画はその文言を変えない。形式の食い違い（件数・バイト数・終わりの印が合わない）と、コマンド全体の 0 以外の終了・終了ステータスなしは、その呼び出し全体の Err にする。部分的な結果を正しい結果として返さないためである。バイト数で区切るので、文字列への変換（`run_command` は UTF-8 として不正なバイトを置き換える）の後では長さが合わない。文字列版の `read_files_batch` はバイト列で読んでからファイルごとに変換する。
- 走査の起点の確認（routes-6・ssh_agent-12）: `list_tree_recursive` は `test -d` のどんな失敗も `RemoteRootNotFound` にし、サブパスの走査（`crates/remote-merge/src/runtime/core.rs` の `fetch_remote_tree_for_subpath`）はそれを空のツリーにするため、接続断が「リモートにファイルがない」になる。終了コード 1（ないかディレクトリでない）だけを `RemoteRootNotFound` にし、接続エラーなどその他の失敗はそのまま返す。core.rs の「`RemoteRootNotFound` なら空のツリー」の対応付けは変えない。これで空のツリーになるのは確認が終了コード 1 を返したとき（ないか、ディレクトリでないか、親を辿れないとき）だけになり、root_dir 全体の走査で接続が切れたときは `Remote root_dir not found` ではなく接続エラーとして報告される。REQ-ssh-003 の再試行は走査には足さない（T17）。この修正は、接続エラーを再試行の対象として見分けられる形で返すという T17 の前提を作るだけである。

SSH の書き込みが最後の要素の symlink を辿る問題（harden-security の計画の Out of scope に残したもの）もここで直す。`openssl base64 -d -A -out P` と `cat > P` は P が symlink ならリンク先を書き換える。harden-security の A6 は Agent について、最後の要素が既存の symlink のパスへ通常ファイルを書かずエラーにし、リンクもリンク先も変えないと決めた。その根拠（リンクを通常ファイルに置き換えるのは REQ-merge-001 が禁じる変更で、リンク先へ書くのは書き込み先の外を変える）は経路によらないので、SSH の経路にも同じ扱いを当てる。シェルには O_NOFOLLOW がないので、同じコマンドの中で書き込みの直前に `[ -L P ]` を確かめ、当たれば何も書かずに 0 以外で終わる。同じ理由で、symlink の作成先が symlink でない既存のディレクトリなら、`ln -sfn` がその中にリンクを作ってしまう（書き込み先の種類の違う対象を変える）ので、作らずにエラーにする。作成先が既存の symlink のときは、`ln -sfn` がリンクそのものを置き換えリンク先を変えないので、REQ-merge-003 のとおりである。これは品質レビューの T1 の 11 件には入っておらず、harden-security の A6 が「別に扱う」と先送りしたものを、利用者が 2026-10-05 にこの計画の範囲に足すと決めた。

ローカルの経路も同じ扱いに揃える（利用者が 2026-10-05 に決めた）。今の `write_local_file_bytes` と `write_local_file`（`crates/remote-merge-engine/src/merge/executor.rs`）は `std::fs::write` で最後の要素の symlink を辿ってリンク先へ書く。これを、Agent（harden-security の `crates/remote-merge-agent/src/agent/file_io.rs` の `write_chunk_without_following_link`）と同じく O_NOFOLLOW で開く書き込みに替え、最後の要素が symlink ならリンクもリンク先も変えずに Err にする。O_NOFOLLOW は確認と書き込みの間の差し替えも防ぎ、OS のエラーが今の `std::fs::write` と同じく io::Error のまま返るので新しい文言を決めずに済む。そのために engine に `libc` を unix 向けの依存として足す（Agent がすでに使っており `Cargo.lock` に版がある。`Cargo.lock` は engine の依存の一覧に libc が加わるだけである）。書き込み先の途中の symlink は REQ-merge-004 のとおり今までどおり辿る（O_NOFOLLOW は最後の要素だけに効く）。ローカルの symlink の作成（`crates/remote-merge-engine/src/local_io.rs` の `create_local_symlink`）は、既存の項目を `std::fs::remove_file` で消してから作るので、symlink でない既存のディレクトリでは何も変えずに失敗し、既存の symlink ではリンクだけを消して作り直す。すでに SSH の経路に足す扱いと同じなので変えない。

--delete が消そうとした書き込み先が比べた後に消えていたときは、どの経路でも failed に出す（利用者が 2026-10-05 に決めた。比べたときから書き込み先が変わったことは REQ-merge-033 が書き込みについて failed に出すのと同じ理由である）。今は、バックアップが有効なら削除の前のバックアップが "backup source does not exist" でどの経路も failed になり、バックアップが無効ならローカルは `std::fs::remove_file` の NotFound で "Delete failed: " の failed になるが、SSH は `rm -f` が成功を返して deleted に入る。SSH の削除のコマンドを、ないパスで 0 以外で終わるもの（`rm -f` をやめる）に替え、ローカルと同じく `execute_deletions` の "Delete failed: " の failed に入るようにする。"Delete failed: " の後の原因の文は経路ごとの既存のエラーのままで、揃えない。リモートの削除の関数（`remove_remote_file`）は merge の symlink の更新（`crates/remote-merge/src/service/merge_flow.rs` の `CreateSymlink` で `target_exists` のときと `ReplaceSymlinkWithFile`）でも使われるので、比べた後に消えていた書き込み先のリンクの更新も SSH の経路で failed になる。この呼び出し元は既存の symlink を消すので、リンク先のない symlink（dangling）も存在するものとして消さなければならない（`test -e` はこれを偽にするので、存在の確かめには `-L` も使うか、`-f` なしの `rm` を使う）。--delete そのものは最後の要素が symlink の対象を消さずに skipped にする（`execute_deletions`）。結果の水準（failed に "Delete failed: " で入ること）は、削除が Err を返せば既存の `execute_deletions` と symlink の更新の `?` がそうするので、この計画は削除のコマンドが 0 以外で終わることを単体テストで示すことに任せる。比べた後に対象を消す競合を契約テストで安定して作る手段がないためである。ローカルの経路はすでにそうなっており、経路が揃う。

これら四つの扱い（SSH の書き込み、SSH の symlink の作成先のディレクトリ、ローカルの書き込み、消えていた削除の対象）は IR の要件の文が直接は決めていない新しくエラーにする扱いなので、S1 で決定記録に残す。

テストは四種類に分ける。生成するシェルのコマンドは純粋関数にして、単体テストで実際の `sh` に一時ディレクトリに対して実行させる（`tree_parser.rs` と `client.rs` のテストにある既存のやり方）。`SshClient` の振る舞いは `crates/remote-merge/tests/ssh_integration.rs` のコマンド登録式のテスト用 SSH サーバで確かめる。利用者から見える merge・sync の結果は、`crates/remote-merge/tests/contract/ssh_server.rs` の `TestServer::filesystem_without_agent`（コマンドを実際の `sh` で実行する）を使う契約テストで確かめる。`.kotowari/config.yaml` の `tests.files` に入るのは契約テストだけなので、印（`@kotowari[...]`）は契約テストにだけ付ける。ローカルの書き込み（S7）は engine の `executor.rs` の単体テストで一時ディレクトリに実際に書いて確かめる。単体テストと `ssh_integration.rs` は印を付けない。

## Scope of change

- `crates/remote-merge-ssh/src/client.rs`
  - `run_command_bytes`、`list_tree_recursive` の起点の確認、`read_files_batch`・`read_files_batch_bytes`、`write_file`・`write_file_bytes` のコマンド、`build_mkdir_command` の公開か移動。`exec` と `exec_strict` の意味は変えない
- `crates/remote-merge-ssh/src/batch_read.rs`
- `crates/remote-merge-ssh/src/tree_parser.rs`（コマンドを組み立てる純粋関数を置く場合だけ）
- `crates/remote-merge-ssh/src/lib.rs`（製品側へ委譲関数を足す場合だけ）
- `crates/remote-merge/src/runtime/remote_io.rs`
  - `remove_remote_file` と `create_remote_symlink` だけ
- `crates/remote-merge/src/ssh/mod.rs`（内部の再公開を足す場合だけ）
- `crates/remote-merge-engine/src/merge/executor.rs`
  - `write_local_file_bytes`・`write_local_file` とその `mod tests`
- `crates/remote-merge-engine/Cargo.toml`（unix 向けの `libc` を足すだけ）
- `Cargo.lock`（engine の依存に libc が加わる行だけ）
- `crates/remote-merge/tests/ssh_integration.rs`
- `crates/remote-merge/tests/contract/ssh_server.rs`（書き込みのコマンドの見分け方だけ）
- `crates/remote-merge/tests/common/`（契約テストの準備を共通にする場合だけ）
- 契約テスト: `crates/remote-merge/tests/contract/` の新しいファイル一つ（`ssh_strict_exec.rs` など）と `crates/remote-merge/tests/contract.rs` のその `mod` 行
- `docs/decision/records/2026-10-05-ssh-strict-exec.md`（新規）
- `.kotowari/changes/implementation.yaml`（`.kotowari/changes/review.yaml` は rebase 後に残っていれば削除する）

## Step order and prerequisites

全体の前提: ブランチ harden-security（`crates/remote-merge-ssh/src/client.rs` の再接続、known_hosts、Agent の `file_io.rs`、Agent の配置、`crates/remote-merge/tests/contract/ssh_server.rs` を変える）は、この計画の実装の前に main へ統合される。実装者は最初にこのブランチを統合後の main へ rebase してから S1 に入る。`client.rs` と `ssh_server.rs` は両方のブランチが変えるので、rebase で衝突しうる。衝突は harden-security の変更を保つ側で解く。harden-security が main に入っていないときは始めない（Stop conditions）。

S1 を最初に置く。S5・S6・S7 が新しくエラーにする扱いの根拠がそこにあるからである。S2 は S4 より前に置く。S4 の一括読み込みは S2 で直したバイト列の実行の上に作り、途中で切れた出力を Err にすることを前提にするからである。S3 のコードは S2 と独立だが、テストは S2 で足した終了ステータスなしの応答を使うので S2 の後に行う。S5 は S1 と S4 の後に行う（S4 で作る契約テストのファイルに足すため）。S6 は S1 の後に行い、S5 とは独立である。S7（ローカルの書き込み）は S1 の後に行い、S2〜S6 とは独立である。S8 は最後に全体を確かめる。

## Verification map

| Step | Requirements | Examples | 印を付けるテスト |
|---|---|---|---|
| S1 | REQ-merge-001, REQ-merge-003, REQ-merge-004, REQ-merge-033, REQ-merge-034 | | なし（決定記録） |
| S2 | REQ-ssh-003 | | なし（`ssh_integration.rs`） |
| S3 | REQ-scan-015（T17 の REQ-ssh-003 の前提） | | なし（`ssh_integration.rs`） |
| S4 | REQ-merge-017, REQ-merge-019 | | 契約テスト一つ: `@kotowari[REQ-merge-017, REQ-merge-019]` |
| S5 | REQ-merge-034, REQ-merge-023, REQ-merge-003, REQ-merge-001 | | 契約テスト二つ: `@kotowari[REQ-merge-023]`、`@kotowari[REQ-merge-034]` |
| S6 | REQ-merge-001, REQ-merge-004 | | なし（単体テスト） |
| S7 | REQ-merge-001 | | なし（engine の単体テスト） |
| S8 | 上のすべて | | — |

REQ-ssh-003 と REQ-scan-015 には、この計画では印を付けない。REQ-ssh-003 の残りの範囲（走査・stat・種類の調べ・子の一覧・ハッシュの再試行、EX-ssh-019）は T17、REQ-scan-015 の Agent の経路は T2 が直すためで、この計画が確かめるのは SSH の経路の一部だけだからである。S8 の後も `kotowari query REQ-ssh-003` と `kotowari query REQ-scan-015` の `tests` が空のままで、それをこの計画の失敗としない。REQ-merge-001 と REQ-merge-003 の SSH とローカルの経路の扱いは単体テストで確かめる。S5 の消えていた削除の対象は、削除のコマンドがないパスで 0 以外で終わることを `sh` の単体テストで確かめ、failed への出し方は既存の `execute_deletions` の Err の扱いに任せる（REQ-merge-001 には既存の印付きのテストがある。engine の単体テストは `tests.files` に入らないので印を付けない）。REQ-merge-004（途中の symlink を辿る書き込み）は S6 と S7 で変えない。S6 は既存の `ssh_external_link.rs` が通ることで示し、S7 は O_NOFOLLOW が最後の要素だけに効くので新しいテストは足さない。EX-ssh-005・EX-ssh-006 は既存のテストの印のままで、この計画は振る舞いを変えないことを既存のテストが通ることで示す。

## Left to the implementer

- 一括読み込みの新しい形式の具体的な書き方。ただし次をすべて満たすこと: POSIX の sh で動き、GNU 独自の機能（`stat -c`、`find -printf` など）に頼らない。古い OpenSSH・Linux の環境（`bench/legacy-ssh/`）で動くことは、POSIX の機能だけを使うことで満たし、その環境での実行は求めない。`wc -c` は使ってよい。どんなバイト列の中身（区切り文字に似た行、末尾の改行なし、NUL、UTF-8 として不正なバイト、空）もそのまま戻る。読めなかったパスは結果に入れない。形式の食い違いは Err にする。コマンド長の見積もり（`chunk_paths`）を新しい形式に合わせる
- 起点の確認、symlink の作成、書き込みの前の確認のシェルの書き方（`test -d` を使うか `[ -d ]` を使うかなど）。ただし `test -L` と `echo SYMLINK` を一つのコマンドに同時に含めない（`ssh_server.rs` の exec の処理がそのコマンドに偽の失敗を返す）
- コマンドを組み立てる純粋関数の名前と置き場所（SSH crate か `remote_io.rs` か）
- `exec_bytes` が使われなくなったときに残すか消すか
- 契約テストのファイル名、設定のサーバ名（`fixture` でも `tests/common/mod.rs` の `develop` でもよい）、テスト用 SSH サーバの準備を共通にするかどうか

## Stop conditions

- harden-security が main に統合されていない、または rebase の衝突を harden-security の変更を保ったまま解けない
- 既存のテストや呼び出し元（rollback・バックアップの復元を含む）が、次のどれかを成功として期待している: SSH の経路で失敗した `rm`・`ln`、SSH の経路で存在しないパスの削除が成功すること、どの経路でも最後の要素が symlink のパスへの書き込みがリンク先に届くこと（バックアップの復元で通常ファイルの記録を書き戻すときを含む）、読めないファイルの一括読み込みが空の中身を返すこと、`test -d` の接続エラーが空のツリーになること
- テストを root で実行していて、権限を落としたファイルやディレクトリで読めない・消せない状態を作れない（前提の崩れをテストの中で知らせ、変えずに報告する）
- 一括読み込みの新しい形式が POSIX の sh の機能だけでは書けないと分かる
- 新しい文言や終了コードを決める必要が出る（この計画は文言を新しく決めない）

## Test command

```sh
cargo nextest run --all-features
```

`ssh_integration.rs` と契約テストの一部は `test-utils` feature がないと動かないため `--all-features` を付ける。整形と lint は `cargo fmt --all --check` と `cargo clippy --all-targets --all-features -- -D warnings`。

## Out of scope

- sudo=true のサーバの削除・権限の変更・子の一覧を Agent で行うこと、それまでの明示的なエラー（`docs/ir/ssh/sudo.md#REQ-ssh-011`、T2・D1）。この計画の後も sudo=true の削除は通常権限の SSH の `rm` で行われ、失敗すると REQ-ssh-011 のエラーではなく `rm` の失敗として failed に出る
- REQ-ssh-003 の再試行の範囲を走査・stat・種類の調べ・子の一覧・ハッシュ・書き込み前の確認に広げること（T17、EX-ssh-019）
- status と diff が読めないファイルを errors に出すこと、diff が結果にないパスを空として扱うこと（REQ-cli-088、REQ-cli-089、REQ-cli-090。T8）、--ref の merge の三者の判定（REQ-cli-092）。この計画は SSH の一括読み込みが空の中身を作らないようにするだけで、`fetch_contents_tolerant` を使う呼び出し元が結果にないパスをどう扱うかは変えない
- Agent のファイル単位の読み込みエラーで Agent を無効にすること、Agent の走査が読めないディレクトリを飛ばすこと（T2）
- symlink の merge で新しく作った親ディレクトリに設定の権限を当てること（`docs/ir/merge/permissions.md#REQ-merge-012`）。今は Agent の経路だけが当て、ローカルの経路も当てない。merge_flow で当てると sudo=true の chmod が通常権限の SSH に流れる（T2）
- 親ディレクトリを辿れない（実行権限がない）サブパス: `test -d` はないときと同じく 1 を返すので、そのサブパスの走査は空のツリーのままになる
- 読めない（モード 000 の）リモートの root_dir: `test -d` は 0 を返し、続く find の失敗が REQ-scan-015 の "Remote root_dir not found" ではない SSH の実行エラーとして出る。この計画は変えない
- 走査の途中の読めないディレクトリ（REQ-scan-014）
- unix 以外の環境のローカルの書き込み（O_NOFOLLOW がないので今までどおり）
- 消えていた削除の対象の "Delete failed: " の後の原因の文を経路の間で揃えること
- `stat_remote_files`、`list_dir_with_limit`、Agent の配置のコマンドが終了コードを見ないこと

## Steps

### S1: 書き込み・symlink の作成・削除で新しくエラーにする四つの判断を記録する

- Purpose: S5・S6・S7 が新しくエラーにする四つの扱いを、根拠と決めた人とともに決定記録に残す
- Specification: `docs/ir/merge/symlink.md#REQ-merge-001`, `docs/ir/merge/symlink.md#REQ-merge-003`, `docs/ir/merge/symlink.md#REQ-merge-004`, `docs/ir/merge/deletion.md#REQ-merge-034`, `docs/ir/merge/concurrency.md#REQ-merge-033`
- Prerequisites: このブランチが harden-security の統合後の main に rebase され、`docs/decision/records/2026-10-05-harden-security.md` に A6 がある
- May change: `docs/decision/records/2026-10-05-ssh-strict-exec.md`
- Done when: 決定記録に次の四つがあり、どれも decided_by が「利用者（2026-10-05）」である。A1（SSH の経路は、最後の要素が既存の symlink であるパスへ通常ファイルを書かずエラーにし、リンクもリンク先も変えない。why: REQ-merge-001 と harden-security の A6 へのリンク、A6 の根拠が経路によらないこと、シェルには O_NOFOLLOW がなく書き込みの直前の確認になること、harden-security の A6 が別に扱うとしたものを利用者がこの計画の範囲に足したこと）。A2（SSH の経路の symlink の作成は、作成先が symlink でない既存のディレクトリならエラーにして何も変えず、作成先が既存の symlink ならリンクそのものを置き換えリンク先を変えない。why: REQ-merge-001・REQ-merge-003 へのリンクと、ローカルの経路がディレクトリの作成先で失敗することと揃えること）。A3（ローカルの経路も、最後の要素が既存の symlink であるパスへ通常ファイルを書かずエラーにし、リンクもリンク先も変えない。途中の symlink は REQ-merge-004 のとおり辿る。ローカルの symlink の作成は A2 と同じ扱いをすでにしている。why: A1 と同じ根拠で、ローカル・SSH・Agent の三つの経路を揃えること）。A4（--delete が消そうとした書き込み先が比べた後に消えていたときは、どの経路でもそのファイルを failed に出し、他のファイルの処理は続ける。SSH の経路は `rm -f` の「ないパスは成功」をやめる。リモートの削除を使う symlink の更新でも、消えていた書き込み先は failed になる。failed の原因の文は経路ごとの既存のエラーのままにする。why: REQ-merge-033 が書き込みについて比べたときから書き込み先が変わったものを failed に出すのと同じ理由、REQ-merge-034 へのリンク、ローカルの経路はすでに失敗にしていること）。Context が品質レビューの T1、harden-security の A6 が残した SSH の経路、ローカルの経路との食い違いを説明している。`kotowari check --format json` がこのファイルに指摘を出さない
- Shown by: artifact — `docs/decision/records/2026-10-05-ssh-strict-exec.md`。形は `docs/decision/records/2026-10-05-harden-security.md` にならう
- Left to the implementer: 文の言い回し（意味は変えない）
- Stop and hand back if: harden-security の A6 が、統合された main で上の内容と食い違う形に変わっている

### S2: 終了ステータスのない一括読み込みを接続エラーにする

- Purpose: `run_command_bytes` が終了ステータスを受け取らずに終わった出力を `AppError::SshConnection` にし、途中で切れた一括読み込みを再接続と再試行の対象にする
- Specification: `docs/ir/ssh/reconnect.md#REQ-ssh-003`
- Prerequisites: このブランチが harden-security の統合後の main に rebase されている
- May change: `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge/tests/ssh_integration.rs`
- Done when: `ssh_integration.rs` のテスト用 SSH サーバに、登録したコマンドに中身を送ったあと終了ステータスを送らずに eof と close だけを返す応答を足し、一括読み込みのコマンドにその応答をパスなど形式に依存しない部分で登録すると（S4 で形式を変えても当たるように）、`SshClient::read_files_batch_bytes` が Err を返し、その Err に `remote_merge::error::is_connection_error`（core の同じ関数）が true を返す。終了ステータスを返す既存の応答のテストは変えずに通る
- Shown by: test — `ssh_integration.rs` のテスト一つ（印なし）を RED → GREEN にする
- Left to the implementer: テスト用 SSH サーバの応答の足し方
- Stop and hand back if: russh のクライアントが、終了ステータスなしで閉じたチャネルを `channel.wait()` の None 以外の形で知らせ、終了ステータスなしを見分けられない

### S3: 走査の起点がないときだけを「ない」とする

- Purpose: `list_tree_recursive` が起点の確認の終了コード 1 だけを `RemoteRootNotFound` にし、接続エラーなどその他の失敗をそのまま返して、サブパスの走査が接続断を空のツリーにしないようにする
- Specification: `docs/ir/scan/listing.md#REQ-scan-015`, `docs/ir/ssh/reconnect.md#REQ-ssh-003`
- Prerequisites: S2
- May change: `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge-ssh/src/tree_parser.rs`, `crates/remote-merge/tests/ssh_integration.rs`
- Done when: `ssh_integration.rs` で、起点の確認のコマンドが終了コード 1 を返すと `list_tree_recursive` の Err が `RemoteRootNotFound` で、その文言が "Remote root_dir not found: サーバ名:パス" になる。起点の確認が終了ステータスなしで終わると（S2 の応答）、Err は `RemoteRootNotFound` ではなく `is_connection_error` が true になる。`crates/remote-merge/src/runtime/core.rs` のサブパスの対応付けは変わっておらず、既存の走査のテストは変えずに通る
- Shown by: test — `ssh_integration.rs` のテスト一つ（印なし）で、起点の確認の結果の見分け（終了ステータスなしは接続エラー、終了コード 1 は `RemoteRootNotFound`）を RED → GREEN にする
- Left to the implementer: 終了コードの見分け方（`exec_strict` の Err から取り出すか、終了コードを返す内部の実行を使うか）
- Stop and hand back if: 既存のテストが、起点の確認の接続エラーで空のツリーや `Remote root_dir not found` になることを期待している

### S4: 一括読み込みで読めないファイルを空の中身にしない

- Purpose: SSH の一括読み込みを、各ファイルの終了状態とバイト数を持つ衝突しない形式に替え、読めなかったパスを結果から外し、形式の食い違いを Err にする
- Specification: `docs/ir/merge/read-failure.md#REQ-merge-017`, `docs/ir/merge/read-failure.md#REQ-merge-019`
- Prerequisites: S2
- May change: `crates/remote-merge-ssh/src/batch_read.rs`, `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge/tests/ssh_integration.rs`（S2 のテストの登録を新しい形式に合わせる場合だけ）, `crates/remote-merge/tests/contract/` の新しい契約テストのファイル, `crates/remote-merge/tests/contract.rs`, `crates/remote-merge/tests/common/`
- Done when: 生成したコマンドを実際の `sh` で一時ディレクトリに対して実行して解析すると、ないファイル、モード 000 のファイル、ディレクトリのパスは結果になく、中身が空のファイルは空の中身として結果にあり、`___BATCH_DELIM___0` で始まる行を含むファイル、末尾に改行のないファイル、NUL と UTF-8 として不正なバイトを含むファイルは中身がそのまま戻る。区切りの件数・バイト数・終わりの印のどれかが合わない出力を解析すると Err になる。コマンド全体が 0 以外で終わったときも Err になる。文字列版の `read_files_batch` は同じ形式をバイト列で読んでから変換する。契約テストでは、`TestServer::filesystem_without_agent` のリモートの root_dir にモード 000 の `a.txt`（中身 "remote\n"）、ローカルに中身の違う `a.txt` を置き、`merge a.txt --left local --right fixture --format json`（Agent 無効、バックアップ無効）を実行すると、終了コード 2 で、failed に `a.txt` があり、その error が "read failed: right: " で始まり、モードを戻した後のリモートの中身が "remote\n" のまま。旧形式を固定していた `parse_batch_output*` と `chunk_paths` の既存のテストは新しい形式に合わせて書き直されている
- Shown by: test — SSH crate の単体テスト（印なし、`sh` で実行）で上の各場合を、`crates/remote-merge/tests/contract/` の契約テスト一つ（`// @kotowari[REQ-merge-017, REQ-merge-019]`）で merge の結果を、それぞれ RED → GREEN にする。契約テストは `merge_paths.rs` の REQ-merge-034 のテストにならい、root で実行して読めてしまうときは前提の崩れを知らせる
- Left to the implementer: 計画全体の「Left to the implementer」に書いた一括読み込みの形式の範囲だけ
- Stop and hand back if: 読めなかったパスを結果から外しても、`fetch_contents_required` が一件ずつの読み込みでやり直さない経路が merge か sync にある、またはテスト用 SSH サーバのシェルでモード 000 のファイルを読めない状態を作れない

### S5: リモートの削除と symlink の作成の失敗を報告する

- Purpose: `remove_remote_file` と `create_remote_symlink` を終了コードを検査する実行に替え、削除はないパスで失敗にし、symlink の作成の前に親ディレクトリを作り、作成先が symlink でない既存のディレクトリなら作らずにエラーにする
- Specification: `docs/ir/merge/deletion.md#REQ-merge-034`, `docs/ir/merge/symlink.md#REQ-merge-023`, `docs/ir/merge/symlink.md#REQ-merge-003`, `docs/ir/merge/symlink.md#REQ-merge-001`
- Prerequisites: S1, S4
- May change: `crates/remote-merge/src/runtime/remote_io.rs`, `crates/remote-merge-ssh/src/client.rs`（非公開の `build_mkdir_command` を `tree_parser.rs` へ移すか公開する）, `crates/remote-merge-ssh/src/tree_parser.rs`, `crates/remote-merge-ssh/src/lib.rs`, `crates/remote-merge/src/ssh/mod.rs`, `crates/remote-merge/tests/contract/` の S4 と同じ契約テストのファイル, `crates/remote-merge/tests/common/`
- Done when: 生成した削除のコマンドを実際の `sh` で一時ディレクトリに対して実行すると、通常ファイルとリンク先のない symlink は消えて 0 で終わり（symlink のリンク先は残る）、ないパスでは 0 以外で終わって何も作られない。生成した symlink の作成のコマンドを実際の `sh` で一時ディレクトリに対して実行すると、親のないパスでは親を作ってリンクを作り、既存の symlink（通常ファイルを指す）の場所ではリンクだけがリンク先を変え元のリンク先のファイルの中身は変わらず、symlink でない既存のディレクトリの場所では 0 以外で終わりディレクトリの中に何も作られず、親の位置に通常ファイルがあって親を作れないパスでは 0 以外で終わる。契約テスト（`TestServer::filesystem_without_agent`、Agent 無効）で、ローカルに `newdir/link -> x` があり リモートに `newdir` がない状態の `merge newdir/link` が、終了コード 0 で、リモートの `newdir/link` のリンク先を `x` にする。ローカルにない `locked/a.txt` がリモートのモード 0555 の `locked/` にあり、`free.txt` がリモートにだけある状態の `merge . --delete --left local --right <サーバ名> --format json`（バックアップ無効。`.` で root_dir 全体を走査する）が、終了コード 2 で、failed に `locked/a.txt` を出し、`locked/a.txt` を残し、`free.txt` を消す
- Shown by: test — 単体テスト（印なし、`sh` で実行）で削除のコマンドのないパスの場合と symlink の作成のコマンドの各場合を、契約テスト二つ（`// @kotowari[REQ-merge-023]` と `// @kotowari[REQ-merge-034]`）で merge の結果を、それぞれ RED → GREEN にする。REQ-merge-034 のテストは `merge_paths.rs` の同じ要件のテストと同じく、root で実行して消せてしまうときは前提の崩れを知らせる
- Left to the implementer: コマンドを組み立てる関数の置き場所。削除のコマンドの書き方（`-f` なしの `rm` か、`[ -e P ] || [ -L P ]` で確かめてから消すか）。ただし `test -L` と `echo SYMLINK` を一つのコマンドに同時に含めない
- Stop and hand back if: `--delete` の merge が、リモートの削除の前の種類の確認で `locked/a.txt` を飛ばすなどして `rm` まで届かず、この状態では削除の失敗を作れない。または、ないパスの SSH の削除が成功することを既存のテスト（rollback や symlink の更新を含む）が期待している

### S6: SSH の書き込みが既存の symlink の先を書き換えない

- Purpose: `write_file` と `write_file_bytes` のコマンドを、最後の要素が既存の symlink なら何も書かずに 0 以外で終わるようにし、リンクもリンク先も変えない
- Specification: `docs/ir/merge/symlink.md#REQ-merge-001`, `docs/ir/merge/symlink.md#REQ-merge-004`
- Prerequisites: S1
- May change: `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge-ssh/src/tree_parser.rs`, `crates/remote-merge/tests/contract/ssh_server.rs`, `crates/remote-merge/tests/ssh_integration.rs`
- Done when: 生成した書き込みのコマンドを実際の `sh` で一時ディレクトリに対して実行し、標準入力に base64 の中身（テキスト版では中身そのもの）を渡すと、書き込み先が `real.txt` を指す symlink のときは 0 以外で終わり、`real.txt` の中身とリンクのリンク先が変わらない。書き込み先が通常ファイルかないときは今までどおり中身が書かれる。途中の symlink 越しの書き込み（REQ-merge-004）は既存の `ssh_external_link.rs` が変えずに通ることで示す。テスト用 SSH サーバの書き込みの見分け方の四か所（`ssh_server.rs` のサンドボックスの分岐の `starts_with("openssl base64 -d -A -out ") || starts_with("cat > ")`、`ssh_server.rs` の `incomplete_writes` の分岐の `starts_with(b"openssl base64 -d")`、`ssh_integration.rs` の `exec_request` と `channel_eof`）が新しい書き込みのコマンドを書き込みとして見分け、SSH の書き込みを使う既存のテスト（`ssh_integration.rs` の `write_file` 系、`ssh_external_link.rs`、`ssh_disconnect.rs`、`merge_cli_ssh.rs` など）が期待を変えずに通る
- Shown by: test — SSH crate の単体テスト（印なし、`sh` で実行）で、symlink の書き込み先の拒否をバイト列版とテキスト版の一つずつ RED → GREEN にする
- Left to the implementer: 確認と書き込みを一つのコマンドにまとめる書き方と、テスト用 SSH サーバの見分け方の変え方（書き込みの判定以外の振る舞いは変えない）
- Stop and hand back if: Agent の配置（`core.rs` の `.tmp` への書き込み）やバックアップの復元が、最後の要素が symlink のパスへの書き込みを使っている

### S7: ローカルの書き込みが既存の symlink の先を書き換えない

- Purpose: `write_local_file_bytes` と `write_local_file` を O_NOFOLLOW で開く書き込みに替え、最後の要素が既存の symlink なら何も書かずに Err を返し、リンクもリンク先も変えない
- Specification: `docs/ir/merge/symlink.md#REQ-merge-001`, `docs/ir/merge/symlink.md#REQ-merge-004`
- Prerequisites: S1
- May change: `crates/remote-merge-engine/src/merge/executor.rs`, `crates/remote-merge-engine/Cargo.toml`, `Cargo.lock`（engine の依存に libc が加わる行だけ）
- Done when: engine の単体テストで、書き込み先が `real.txt` を指す symlink のとき `write_local_file_bytes` と `write_local_file` が Err を返し、`real.txt` の中身とリンクのリンク先が変わらない。書き込み先が通常ファイルかないとき（親ディレクトリの作成を含む）は今までどおり書かれ、`executor.rs` の既存のテストが変えずに通る。途中の symlink を辿る書き込み（REQ-merge-004）を止める確認は足さない。`libc` は `cargo add libc --target 'cfg(unix)' -p remote-merge-engine` で unix 向けの依存として足され、`Cargo.lock` の解決は Agent と同じ版のままである。unix 以外の環境では今の `std::fs::write` のままでビルドが通る。ローカルの symlink の作成（`create_local_symlink`）は変えない
- Shown by: test — engine の `executor.rs` の単体テスト二つ（印なし。バイト列版とテキスト版の symlink の書き込み先の拒否）を RED → GREEN にする。エラーの文は契約ではないので、テストは Err とファイルが変わらないことだけを確かめる
- Left to the implementer: 二つの関数の共通部分の切り出し方
- Stop and hand back if: `create_local_symlink` が symlink でない既存のディレクトリの作成先で何かを変える、または既存の symlink のリンク先を消すと分かる（S1 の A3 の前提が崩れる）。バックアップの復元や merge の流れが、最後の要素が symlink のローカルのパスへの通常ファイルの書き込みを使っている

### S8: 全体の確認と変更の記録

- Purpose: この計画の変更がすべての確認を通り、比較全体の実装者の記録があることを示す
- Specification: `docs/ir/merge/deletion.md#REQ-merge-034`, `docs/ir/merge/symlink.md#REQ-merge-023`, `docs/ir/merge/read-failure.md#REQ-merge-017`, `docs/ir/merge/read-failure.md#REQ-merge-019`, `docs/ir/ssh/reconnect.md#REQ-ssh-003`, `docs/ir/scan/listing.md#REQ-scan-015`
- Prerequisites: S1, S2, S3, S4, S5, S6, S7
- May change: `.kotowari/changes/implementation.yaml`, `.kotowari/changes/review.yaml`（rebase 後に残っていれば削除だけ）、および S1〜S7 の May change のファイルの整形と clippy の指摘の修正
- Done when: 下のコマンドがすべて成功する。`kotowari query` で REQ-merge-017・REQ-merge-019・REQ-merge-023・REQ-merge-034 の tests に S4・S5 の契約テストがある（REQ-ssh-003 と REQ-scan-015 の tests が空なのは Verification map のとおり失敗としない）。`kotowari check --format json` が、このブランチで変えたファイルとこの計画の ID に error を出さない。rebase 後の main にある古い比較の記録（`implementation.yaml`、残っていれば `review.yaml`）が除かれ、新しい `implementation.yaml` が、比較元（rebase 後の `git merge-base main HEAD` の完全なコミット ID。harden-security の統合を含む main のコミットで、a22ff0b ではない）からこのブランチの変更をすべて覆い、S1 の決定を使う変更（S5 のディレクトリの拒否とないパスの削除の失敗、S6、S7）を gap（disposition recorded、S1 の決定記録と REQ-merge-001・REQ-merge-003・REQ-merge-004・REQ-merge-033・REQ-merge-034 の IR を参照）として、他を既存の要件と IR を参照する existing として記録している
- Shown by: check — `cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`、`kotowari check --format json`、`kotowari changes --base <比較元の完全な ID> --staged --phase implementation --format json`
- Left to the implementer: 記録の entry の分け方と ID
- Stop and hand back if: `kotowari check` が、このブランチで変えていないファイルの error だけで終了コード 0 にならない（その場合は報告して進める。直さない）
