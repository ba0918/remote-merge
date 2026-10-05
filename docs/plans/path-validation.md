# Plan: パスの検査をどの経路・どの走査方式でも同じにする

## Goal

diff・merge・sync に絶対パスか ".." を含む引数を渡すと、ローカル・SSH・エージェントのどの経路でも、どの走査方式でも、接続・走査・読み込み・書き込みの前に同じ文言・終了コード 2 で止まる。rollback の復元前のパス検査も三つの経路で同じ判定（同じ TargetPath か同じエラー）になり、親が通常ファイルに変わったファイルはどの経路でもスキップされる。

## Specification

IR は `docs/ir/`。この計画が扱う要件と例:

- `docs/ir/cli/diff-output.md#REQ-cli-078`、例 EX-cli-069
- `docs/ir/merge/symlink.md#REQ-merge-007`、例 EX-merge-013、EX-merge-014、EX-merge-040
- `docs/ir/backup/rollback-path.md#REQ-backup-003`、`#REQ-backup-031`、`#REQ-backup-033`、例 EX-backup-027

判断の根拠は次の三つ。

- `docs/decision/records/2026-10-05-quality-cli.md#A1`（D2 の diff 側）
- `docs/decision/records/2026-10-05-quality-merge.md#A1`（D2 の merge・sync 側）
- `docs/decision/records/2026-10-05-quality-backup.md#A2`（D13）

もとになった品質レビューの指摘は、T5 の routes-11・routes-12・diff-14・merge_sync-10・backup_rollback-1 と、D13 の backup_rollback-2（T11）。台帳は作業ツリーの外にあり、実装者は読まなくてよい。この計画に必要な事実はここに書いた。

関連して守る既存の要件は次の二つで、どちらも振る舞いは変えない。

- `docs/ir/cli/diff-output.md#REQ-cli-052`: "/" は root_dir 全体の指定。テストは `crates/remote-merge/tests/contract/diff_selection.rs` に既にある
- `docs/ir/cli/symlink-diff.md#REQ-cli-026`: 入力パスの親への遡りを禁止する

## Approach and why

今の状態（main 668161d のコードで確かめた事実）:

- 引数の検査は `crates/remote-merge-engine/src/service/path_resolver.rs` の `check_path_traversal` で、".." だけを拒否する。
  - diff は FastPath なら走査の前にこれを呼び、PartialScan と FullScan では接続と走査の後に `resolve_target_files_from_statuses` の中で呼ぶ。merge と sync も、走査の後の `resolve_target_files_from_statuses` で呼ぶ。
  - 絶対パスを拒否するのは `local_io::fetch_tree_for_subpath` と `RemoteTargetIo::fetch_tree_for_subpath` だけで、部分走査のときに限られる。`--delete`・glob・21 個以上のパスで全体走査になると素通りする。
- 素通りした絶対パスの扱いは経路で違う。
  - ローカル: `root.join` で root_dir の外を指す。
  - SSH: `executor::validate_remote_path` が先頭の "/" を削り、root_dir の下と読み替える。
  - エージェント: `file_io::reject_escaping_rel_path` が拒否する。
  - "a/../b" は、ローカルでは字面の正規化で通り、SSH とエージェントでは拒否される。
- SSH の経路のパス検査は `crates/remote-merge/src/runtime/remote_path.rs` の `build_inspect_path_command`。ローカル（`local_io::inspect_local_path`）とエージェント（`file_io::inspect_path`）は同じ手順で、lstat の ENOENT だけを Missing にし、それ以外の失敗ではエラーを返す。SSH は次の四つの状態で、ローカルとエージェントと違う結果を返す（ローカルとエージェントはどれもエラー）。
  - 通常ファイルの下（ENOTDIR）: SSH は missing
  - 検索権限のないディレクトリの中（EACCES）: SSH は missing
  - 自分を指す symlink（ELOOP）: SSH は symlink
  - リンク切れかループする祖先の symlink の下: SSH は missing
  - SSH の missing の `real_parent` は末尾の要素を含むが、ローカルは含まない。
  - 存在するパスで `readlink -f` が失敗したときは、SSH もエラーになる。
- rollback の流れは次のとおり。
  - `crates/remote-merge/src/runtime/side_io.rs` の `restore_backup`・`inspect_recorded_path`・`inspect_restore_path` が検査し、engine の `service/rollback.rs` の `decide_restore_path`・`restore_request_refusal` が判定する。
  - 第一 pass で検査がエラーになると、セッション全体を "cannot resolve path: " で失敗にする。
  - `TargetPath::File` はディレクトリと通常ファイルを区別しない。親が通常ファイルに変わったとき、ローカルとエージェントは ENOTDIR でセッション全体を失敗にし、SSH は書き戻しに進む。どの経路も REQ-backup-031 のスキップを出せない。

方針:

- 検査の規則は一つの関数に置き、引数の境界で呼ぶ（ba0918-design の「一つの境界に一つの検査」）。
  - 中心になる関数は、絶対パス（"/" を含む）と ".." の要素を、IR の二つの文言で拒否する。
  - engine の中で依存が循環しないよう、置き場所は `merge/executor.rs` か、engine の新しい末端のモジュールにする。`service` は既に `merge::executor` に依存しており、逆向きの依存を作らないため。
  - 引数用の検査は `path_resolver.rs` に置く。`has_root_marker` と同じく末尾の "/" を除くと空になる値（"/"、"//"、"."、空など）を飛ばしたうえで、中心の関数を呼ぶ。
  - diff・merge・sync はこれを引数の検査の段階で呼ぶ。サーバの解決・接続・バックアップの整理・走査より前に置くので、走査方式による違いはなくなる。diff はパスの末尾の "/" を書き換える前の生の値で検査し、エラーには利用者が入力したとおりの値が出るようにする。
- 経路ごとの TargetIo の境界でも、同じ中心の関数を再利用して多層の防御を置く。範囲は次の二つに限る。
  - SSH の `validate_remote_path` が先頭の "/" を削る読み替えを、拒否に変える。"/" も拒否し、root_dir 全体として通すのは空の値だけにする。ローカルの `inspect_local_path` は既に `validate_remote_path` を呼んでいるので、同じ変更で守られる。
  - 検査のない `RemoteTargetIo::fetch_children` に、同じ検査を足す。
- SSH の検査スクリプトは、ローカルの `inspect_local_path` を基準にし、同じ入力で同じ TargetPath（missing の `real_parent` の形を含む）か、同じくエラー（終了コードが 0 以外）を返すように書き直す。
  - ENOENT と判断するのは、存在する最も近い祖先が、辿った先で検索できるディレクトリで、その下の要素がないときだけにする。
  - 正しさは、同じ一時ディレクトリでスクリプトとローカルの関数の結果を突き合わせる単体テストで示す。
  - エージェントはローカルと同じ手順なので、これで三つの経路がそろう。
- D13 の「親のパスが通常ファイルに変わった」を見分けるため、TargetIo に「そのパスが、辿った先で通常ファイルか」を問う狭い操作を一つ足す。エージェントの crate と通信の型は変えない。
  - ローカル: `std::fs::metadata`
  - SSH: `remote_path.rs` に組み立てる短いコマンド
  - エージェント: 既存の StatFiles。`file_io::stat_file` が返す `meta.permissions().mode()` は、Unix では種類のビットを含む st_mode そのものなので、種類を読める。エラーのエントリは黙って落とされるので、結果にエントリがなければ「通常ファイルとは分からない」と扱う
- rollback がスキップに切り替えるのは、次の三つがそろったときだけにする。
  - 戻す先の検査がエラーになった。
  - すぐ上の親の検査が、symlink でない `File` を返した。
  - 親が通常ファイルだと分かった。
  - このとき `CurrentRestorePath::Missing { real_parent: None }` を作り、既存の判定でスキップ理由 "parent directory no longer exists" を出す。それ以外のエラーは、今のまま REQ-backup-003 の "cannot resolve path: " にする。

計画の判断（利用者がこの計画を承認すると確定する。実装者は `docs/decision/records/2026-10-05-path-validation.md` に、内容・根拠・決めた役「利用者（計画の承認）」で書く）:

- リンク切れの祖先の symlink の下のパスは、REQ-backup-003 の「リンク先を辿れない」と読み、どの経路でも "cannot resolve path: " にする。
  - 根拠: ローカルとエージェントの今の振る舞いがこれで、書き戻さない安全な側である。REQ-backup-033 の「リンク切れの symlink」は、末尾の symlink の例として書かれている。
- D13 のスキップは、すぐ上の親が symlink でない通常ファイルのときだけにする。
  - 祖父より上の祖先が通常ファイルに変わったときと、親が通常ファイルを指す symlink になったときは、IR が決めていない。この計画では振る舞いを変える実装をせず、テストもしない（今のままエラーになる）。
- 境界の多層の防御（S3）は、引数の検査の後には利用者の入力が届かない。このため新しい分岐は作らず、既存の検査の置き換えと一つの検査の追加に限る。

変更の照合（`.kotowari/config.yaml` の changes が有効）:

- 比較元と対象は、記録の自己申告ではなくブランチの履歴から決める。
  - 比較元: ssh-strict-exec の取り込み後の main に rebase した後の、`git merge-base main HEAD` の完全なコミット ID。
  - 対象: 最終の HEAD の完全なコミット ID。
- rebase 後に main から入った `.kotowari/changes/implementation.yaml` と `.kotowari/changes/review.yaml` は、今回の比較の記録ではないので両方を消す。
- 実装者が `.kotowari/changes/implementation.yaml` を新しく作り、実装と別のレビュー担当が `.kotowari/changes/review.yaml` を作る。
- 統合の条件は、次の二つがどちらも終了コード 0 になること。
  - `kotowari check --format json`
  - `kotowari changes --base <base> --head <head> --phase review --format json`
- この計画の最終手順で見るのは実装者の記録までで、レビュー担当の記録はサイクルの最後のレビューで作る。

## Scope of change

- `crates/remote-merge-engine/src/merge/executor.rs`
  - `validate_remote_path`、中心の検査関数をここに置く場合のその関数、両者の `mod tests` だけ。書き込みの関数は ssh-strict-exec の範囲なので触らない
- engine の新しい末端のモジュールと `crates/remote-merge-engine/src/lib.rs` のその宣言（中心の検査関数をそこに置く場合だけ）
- `crates/remote-merge-engine/src/service/path_resolver.rs`
  - 引数用の検査とその `mod tests`
- `crates/remote-merge-engine/src/service/fast_path.rs`
  - 検査関数の名前や戻り値を変えたときの呼び出しとその `mod tests` だけ
- `crates/remote-merge-engine/src/local_io.rs`
  - ローカルの「通常ファイルか」の自由関数を足すことだけ。書き込みの関数は触らない
- `crates/remote-merge-engine/src/service/rollback.rs`
  - 検査のエラーと親の種類から `CurrentRestorePath` を決める純粋関数を置く場合だけ
- `crates/remote-merge/src/service/` の再公開（新しい engine の項目を製品側から使うときだけ）
- `crates/remote-merge/src/cli/diff.rs`、`crates/remote-merge/src/cli/merge.rs`、`crates/remote-merge/src/cli/sync.rs`
  - 引数の検査の呼び出しだけ
- `crates/remote-merge/src/runtime/target_io.rs`
  - `RemoteTargetIo::fetch_children` の検査と、新しい「通常ファイルか」の操作
- `crates/remote-merge/src/runtime/remote_path.rs`
  - 検査スクリプト、新しい SSH のコマンド、その実行を担う `CoreRuntime` の impl ブロック、`mod tests`
  - `remote_io.rs` の private な `resolve_remote_path` は呼べないので、`executor::validate_remote_path` を直接使う
- `crates/remote-merge/src/runtime/side_io.rs`
  - `inspect_restore_path`、エージェントで種類を問う `try_agent_*` の関数一つ、`mod tests`
- テスト
  - `crates/remote-merge/tests/cli_diff.rs`
  - `crates/remote-merge/tests/contract/merge_paths.rs`
  - `crates/remote-merge/tests/contract/sync_cli.rs`
  - `crates/remote-merge/tests/contract/backup_rollback_path.rs`
  - `crates/remote-merge/tests/contract/scan_listing_cli.rs`（`AgentFixture` に起動関数を足すことだけ。既存の関数とその確認は変えない）
  - `crates/remote-merge/tests/contract/` の新しいファイルと、`crates/remote-merge/tests/contract.rs` のその `mod` 行。CliEnv か AgentFixture を使うファイルは、既存の行にならい `#[cfg(feature = "test-utils")]` を付ける
  - 古い振る舞いを固定している既存のテスト。手順ごとに書いた範囲で、見つけたものだけ
- `docs/decision/records/2026-10-05-path-validation.md`（新規）
- `.kotowari/changes/implementation.yaml`。`.kotowari/changes/review.yaml` は、rebase 後に残っていれば消すだけ

次は変えない。

- ssh-strict-exec の範囲
  - `crates/remote-merge-ssh/src/client.rs`
  - `crates/remote-merge-ssh/src/batch_read.rs`
  - `crates/remote-merge/src/runtime/remote_io.rs`
  - `crates/remote-merge-engine/src/merge/executor.rs` と `local_io.rs` の書き込みの関数
- `crates/remote-merge/tests/contract/ssh_server.rs`。この試験サーバの模擬の応答は、コマンドが `resolve_existing_prefix()` で始まることと、`; p=<引用したパス>; if [` の並びで検査を見分ける。S4 はこの形を保つ
- エージェントの crate（`crates/remote-merge-agent/`）と通信の型（`crates/remote-merge-protocol/`）

## Step order and prerequisites

前提: ブランチ ssh-strict-exec が先に main に入っていること。このブランチは、SSH の実行の厳格化、一括読み込みの区切り、SSH とローカルの末尾 symlink への書き込み、消えたファイルの削除の失敗を扱う。

- path-validation は、ssh-strict-exec を含む main に rebase してから実装を始める。ssh-strict-exec が main に入っていなければ、実装を始めずに止める。
- rebase では `executor.rs`、`contract.rs` の `mod` 行、`tests/common/` が衝突しうる。相手の変更を残して解消する。
- ssh-strict-exec は `exec` と `exec_strict` の意味を変えない計画である。このため S4 は、スクリプトの終了コードが 0 以外ならエラーになる今の `exec_strict` を前提にしてよい。

順序:

- S1 → S2: S2 は S1 の検査関数を merge・sync で呼ぶ。
- S1 → S3: S3 は同じ中心の関数を境界で再利用する。S3 を先にすると、引数の段階より前に境界の文言で止まる経路ができる。
- S4 → S5: S4 で SSH の検査が通常ファイルの下をエラーとして返すようになってはじめて、S5 の判定が三つの経路で同じ入力から始まる。S5 は S4 で足す SSH の rollback の準備も使う。
- S6 は最後。
- S4 は S1〜S3 のコードに依存しないが、rebase の衝突を一度で済ませるため S3 の後に置く。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-cli-078 | EX-cli-069 |
| S2 | REQ-merge-007 | EX-merge-013, EX-merge-014, EX-merge-040 |
| S3 | REQ-merge-007, REQ-cli-078（多層の防御。`src/` の単体テストなので印は数えられない） | なし |
| S4 | REQ-backup-003, REQ-backup-033 | なし |
| S5 | REQ-backup-031 | EX-backup-027 |
| S6 | 上のすべての確認 | 上のすべての確認 |

印（`@kotowari[...]`）が数えられるのは、`.kotowari/config.yaml` の `tests.files` にある次のファイルだけ。

- `crates/remote-merge/tests/contract/**/*.rs`
- `crates/remote-merge/tests/cli_diff.rs`
- `tests/container-e2e/tests/sudo.rs`

REQ-cli-078 と EX-cli-069 の印は `cli_diff.rs` か `contract/` に置き、そのほかの印は `contract/` に置く。`src/` の `#[cfg(test)]` と、`cli_diff.rs` 以外の `tests/cli_*.rs` の印は数えられない。

## Left to the implementer

- 中心の検査関数と引数用の検査関数の名前と戻り値の型。`check_path_traversal` を広げても、新しい名前に置き換えてもよい
- 中心の検査関数を `merge/executor.rs` に置くか、engine の新しい末端のモジュールに置くか。どちらでも `service` から `merge` への一方向の依存を保つ
- `resolve_scan_strategy` の中の検査を残すか除くか。コマンドが先に止まるので、どちらでも振る舞いは変わらない
- 新しい TargetIo の操作の名前と、rollback の判定を engine の純粋関数に置くか SideIO に置くか
- 検査スクリプトの組み方（`test -d`・`test -x`・`readlink` の使い方、関数の分け方）。次の三つの範囲に収める
  - POSIX sh と、今のスクリプトが既に使う `readlink -f` だけを使う
  - コマンドの先頭は `resolve_existing_prefix()` の定義のままにする
  - `; p=<引用したパス>; if [` の並びを保つ
- 新しい契約テストのファイルの名前と、準備の関数の分け方

## Stop conditions

- ssh-strict-exec が main に入っていない
- rebase の後、この計画が名指ししたものが、計画の書いた形と違う（`exec_strict` の意味が変わった、`AgentFixture` か `CliEnv::reject_logins_after` がない、試験サーバの検査の見分け方が変わった、など）
- エージェントの StatFiles の `permissions` が種類のビットを持たないと分かった。親の種類を経路をまたいで知るには通信の型の変更が要るので、利用者の判断に戻す
- 祖父より上の祖先が通常ファイルに変わったときか、親が通常ファイルを指す symlink になったときを、どう扱うかの答えを利用者から受け取った。その答えが今のエラーの扱いと違えば、その答えを決定記録と IR に反映する手順がこの計画にないので、実装を止めて戻す
- 既存のテストが、この計画の要件と関係のない理由で失敗する
- 内部の呼び出しが、先頭に "/" のある相対パスや "/" を境界へ正当に渡していて、S3 で壊れると分かった

## Test command

呼び出し元の指定に従う（PROJECT.md の通常の指定は `cargo nextest run`）。

```sh
cargo nextest run --all-features
```

## Out of scope

- `docs/ir/scan/limits.md#TBL-scan-001` のテスト（走査の範囲の表で、別の修正単位で扱う）
- diff が読めないファイルを errors に出す扱い（REQ-cli-089 などの D19 の単位）。S4 の後に SSH の経路が通常ファイルの下をエラーとして返すのは、ローカルの今の扱いと同じになるだけで、その出し方は変えない
- ローカルの読み書き（`executor::read_local_file`・`write_local_file`）の字面の正規化（ssh-strict-exec の範囲と重なるため）
- エージェントの crate と通信の型の変更
- 今の `RemoteTargetIo::fetch_children` が、sudo でもエージェントを使わないこと（T2 の範囲）
- 祖父より上の祖先が通常ファイルに変わったときと、親が通常ファイルを指す symlink になったときの rollback の扱い（IR が決めていない）

## Steps

### S1: 一つの検査関数と diff の引数の検査

- Purpose: 絶対パスと ".." の要素を拒否する検査を一つの関数にまとめ、diff が走査方式と経路によらず、接続の前の引数の段階で止まるようにする
- Specification: docs/ir/cli/diff-output.md#REQ-cli-078, docs/ir/cli/diff-output.md#REQ-cli-052
- Prerequisites: ssh-strict-exec を含む main への rebase が済んでいる
- May change: crates/remote-merge-engine/src/merge/executor.rs（中心の検査関数を置く場合だけ）, engine の新しい末端のモジュールと lib.rs のその宣言, crates/remote-merge-engine/src/service/path_resolver.rs, crates/remote-merge-engine/src/service/fast_path.rs, crates/remote-merge/src/service/ の再公開, crates/remote-merge/src/cli/diff.rs, crates/remote-merge/tests/cli_diff.rs, crates/remote-merge/tests/contract/scan_listing_cli.rs（AgentFixture に、エージェントを有効にした設定で diff を起動して受けたコマンドを返す関数を足すことだけ。既存の diff_via_agent は変えない）
- Done when: テキスト形式の diff が、パスの末尾の "/" を書き換える前の生の値で検査し、ログインを受け付けない SSH の右側に対しても接続の前に止まる。"a.txt" と "/etc/passwd" では比較の結果を出さず、標準エラーへ "absolute subpath not allowed: /etc/passwd" を出して終了コード 2 で終わる。"a/../b.txt" では "path traversal not allowed: a/../b.txt" で同じく止まる。glob を含む組や 21 個以上のパスで全体走査になる組でも同じく止まる。エージェントを有効にした設定でも、試験サーバにコマンドが一つも届かずに止まる。"/" だけを渡したときは、今までどおり root_dir 全体を比べる
- Shown by: test — RED を確かめてから実装する。cli_diff.rs の CliEnv に、EX-cli-069 そのもの（今は SSH の右側で root_dir/etc/passwd を比べようとする）、全体走査になる組の絶対パス、reject_logins_after(0) にした CliEnv での "a/../b.txt"（今は接続のエラーが先に出る）の三つのテストを書く。AgentFixture の新しい起動関数で、エージェントを有効にした設定（assert_isolated_agent_config で確かめる）の絶対パスで試験サーバにコマンドが届かないテストを書く。それぞれに @kotowari[REQ-cli-078] か @kotowari[EX-cli-069] の印を付ける。path_resolver.rs の mod tests に、引数用の検査が受け付ける値（"/"、"//"、"."、"./"、空）と拒否する値の表のテストを置く
- Left to the implementer: 絶対パスであり ".." も含む値（"/../x" など）の文言は、今の fetch_tree_for_subpath と同じく絶対パスを先に調べる。テストはこの組の文言を固定しない
- Stop and hand back if: 既存の cli_diff.rs のテスト（REQ-cli-026 の parent_directory_path_is_rejected_even_with_follow_flag を含む）が、文言の確認以外の理由で通らなくなる

### S2: merge と sync の引数の検査

- Purpose: merge（--hunks を含む）と sync が、走査方式と経路によらず引数の段階で止まり、どのファイルも書かないようにする
- Specification: docs/ir/merge/symlink.md#REQ-merge-007
- Prerequisites: S1
- May change: crates/remote-merge/src/cli/merge.rs, crates/remote-merge/src/cli/sync.rs, crates/remote-merge/tests/contract/merge_paths.rs, crates/remote-merge/tests/contract/sync_cli.rs, crates/remote-merge/tests/contract/ の SSH の経路の merge 用の新しいファイルと contract.rs の mod 行
- Done when: validate_merge_args と validate_sync_args が S1 の引数用の検査を呼ぶ。--delete を付けた絶対パスの merge と、絶対パスの sync は "absolute subpath not allowed: パス" で止まり、何も書かない。書き込み先が SSH の経路のときも、終了コード 2 で root_dir の下に何も作らない。"a.txt" と "../b.txt" の merge は "path traversal not allowed: ../b.txt" で止まり、a.txt も書かない
- Shown by: test — merge_paths.rs の既存の merge_rejects_parent_traversal_before_writing（EX-merge-013）と merge_rejects_absolute_paths_before_writing（EX-merge-014）は、承認済みの要件が変わったので、文言の全体と「何も書かない」を確かめる形に直す。merge_paths.rs に --delete と絶対パスの merge のテスト（今は計画に入る RED）を、sync_cli.rs に絶対パスの sync のテストを足し、@kotowari[REQ-merge-007] の印を付ける。merge_cli_ssh.rs の形にならい、CliEnv で書き込み先が SSH の merge --delete --force に絶対パスを渡すテストを contract/ に足し、終了コード 2 と root_dir の下に何もできないことを確かめて @kotowari[REQ-merge-007] の印を付ける。EX-merge-040 は今も同じ文言で止まるので、そのテストは変更の前から通る。例の印を付けるために merge_paths.rs に書き、@kotowari[EX-merge-040] の印を付ける
- Left to the implementer: none
- Stop and hand back if: merge のバックアップの整理（cleanup_expired_backups）か接続が、引数の検査より前に動く順序に変わっていて、検査を前に置くには順序を変える必要がある

### S3: 経路ごとの境界の多層の防御

- Purpose: 引数の検査をすり抜けた値があっても、SSH の経路が絶対パスを root_dir の下と読み替えず、子の一覧も root_dir の外を見ないようにする
- Specification: docs/ir/merge/symlink.md#REQ-merge-007, docs/ir/cli/diff-output.md#REQ-cli-078
- Prerequisites: S1
- May change: crates/remote-merge-engine/src/merge/executor.rs（validate_remote_path とその mod tests だけ）, crates/remote-merge/src/runtime/target_io.rs（RemoteTargetIo::fetch_children だけ）, crates/remote-merge/src/runtime/side_io.rs（mod tests だけ）
- Done when: validate_remote_path が中心の検査関数で絶対パス（"/" を含む）と ".." を拒否し、先頭の "/" を削らない。root_dir 全体を指す空の値は今までどおり通る。RemoteTargetIo::fetch_children は、SSH を呼ぶ前に同じ検査をする
- Shown by: test — 既存の executor.rs の test_validate_remote_path に、"/etc/passwd" と "/" の拒否を足して RED を確かめる。side_io.rs の既存の test_fetch_tree_for_subpath_remote_rejects_absolute_subpath と同じく、未接続の CoreRuntime で Side::Remote の子の一覧に "/etc" を渡すテストを書く。検査のない今は "SSH not connected" になる（RED）ことを確かめる
- Left to the implementer: none
- Stop and hand back if: 既存のテストが、内部の呼び出しが先頭に "/" のある相対パスや "/" を境界へ渡しているために失敗する

### S4: SSH の検査スクリプトをローカルの検査にそろえる

- Purpose: SSH の経路のパス検査が ENOENT のときだけ missing を返し、それ以外の失敗ではローカルとエージェントと同じくエラーを返すようにして、辿れないパスを含むセッションの rollback を SSH でもセッションごと止める
- Specification: docs/ir/backup/rollback-path.md#REQ-backup-003, docs/ir/backup/rollback-path.md#REQ-backup-033
- Prerequisites: S3（順序の都合だけ）
- May change: crates/remote-merge/src/runtime/remote_path.rs, crates/remote-merge/tests/contract/ の SSH の経路の rollback 用の新しいファイルと contract.rs の mod 行, SSH の古い missing を固定している既存のテスト（直したものは名前と場所を報告に書く）, docs/decision/records/2026-10-05-path-validation.md
- Done when: 一時ディレクトリの次の各状態で、sh で実行した build_inspect_path_command の出力を parse_inspect_path_output で読んだ結果と、remote_merge_engine::local_io::inspect_local_path の結果が、両方成功して同じ TargetPath になるか、両方エラーになる。状態は、通常ファイル、親のある存在しないファイル、親もない存在しないファイル、通常ファイルの下のパス、検索権限のないディレクトリの中のパス、自分を指す末尾の symlink、リンク切れの末尾の symlink、リンク切れの祖先の symlink の下のパス、ループする祖先の symlink の下のパス。加えて、an_unresolvable_path_blocks_every_file_in_the_rollback_session と同じ準備（二つのファイルを merge し、second.txt を自分を指す symlink に置き換える）で、CliEnv（エージェントなしの試験 SSH サーバ）の rollback が first.txt を書き戻さず、二つとも "cannot resolve path: " に続く空でない原因で failed に出し、終了コード 2 を返す。決定記録にリンク切れの祖先の読み方を書いている
- Shown by: test — スクリプトを直す前に二つのテストを書いて RED を確かめる。一つ目は remote_path.rs の mod tests に置く、上の状態を並べた表で二つの結果を突き合わせるテスト。検索権限のない状態は、実効ユーザーが root のときは飛ばす。二つ目は contract/ に置く SSH の経路の rollback のテストで、今は first.txt を書き戻す。こちらに @kotowari[REQ-backup-003] の印を付ける。既存の command_resolves_a_dangling_terminal_symlink_to_the_path_it_names は通ったままにする。エージェントの経路は今もローカルと同じに止まるので、新しいテストは足さない
- Left to the implementer: スクリプトの組み方（Left to the implementer の節の範囲で）
- Stop and hand back if: POSIX sh と readlink -f の範囲では、上のどれかの状態でローカルと同じ結果を出せない（その状態と理由を報告する）

### S5: 親が通常ファイルに変わったファイルをどの経路でもスキップする

- Purpose: 戻す先にファイルがなく、すぐ上の親のパスが symlink でない通常ファイルに変わったとき、どの経路でもそのファイルだけを "parent directory no longer exists" でスキップし、ほかのファイルは書き戻す
- Specification: docs/ir/backup/rollback-path.md#REQ-backup-031, docs/ir/backup/rollback-path.md#REQ-backup-003
- Prerequisites: S4
- May change: crates/remote-merge/src/runtime/target_io.rs, crates/remote-merge/src/runtime/side_io.rs, crates/remote-merge/src/runtime/remote_path.rs, crates/remote-merge-engine/src/local_io.rs（通常ファイルかの自由関数だけ）, crates/remote-merge-engine/src/service/rollback.rs, crates/remote-merge/src/service/ の再公開, crates/remote-merge/tests/contract/backup_rollback_path.rs, S4 で足した SSH の rollback の契約テストのファイル, crates/remote-merge/tests/contract/scan_listing_cli.rs（AgentFixture に、エージェントを有効にした設定で merge と rollback を起動する関数を足すことだけ。HOME と XDG_DATA_HOME は一時ディレクトリに向け、エージェントが起動したことを確かめる）, crates/remote-merge/tests/contract/ のエージェントの経路の rollback 用の新しいファイルと contract.rs の mod 行, docs/decision/records/2026-10-05-path-validation.md
- Done when: --delete で "dir/a.txt" ともう一件を消したセッションの後に "dir" を通常ファイルに置き換えると、ローカル（local_override）・SSH（CliEnv）・エージェント（AgentFixture）のどの経路の rollback も "dir/a.txt" をスキップ理由 "parent directory no longer exists" で skipped に出し、ディレクトリを作らず、もう一件を書き戻す。決定記録に、親の種類の調べ方を書いている
- Shown by: test — EX-backup-027 を三つの経路で確かめるテストを書き、RED を確かめる。今はローカルとエージェントが ENOTDIR でセッション全体を "cannot resolve path" で失敗にし、S4 の後の SSH も同じになる。backup_rollback_path.rs のテストに @kotowari[EX-backup-027] の印を、SSH とエージェントのテストに @kotowari[REQ-backup-031] か @kotowari[EX-backup-027] の印を付ける。新しい純粋関数を置いたら、その分岐（親が通常ファイル、親がディレクトリ、親の種類が分からない）の単体テストを engine の mod tests に置く
- Left to the implementer: 新しい操作の名前と、判定を純粋関数に置くか SideIO に置くか
- Stop and hand back if: エージェントの StatFiles の permissions が種類のビットを持たない。または既存のテストが、symlink でない通常ファイルの親を "cannot resolve path" として固定している（そのテストは REQ-backup-031 の改訂で古くなった期待なので、この手順で直し、名前と場所を報告に書く。直すべきか分からなければ止めて戻す）

### S6: 仕様と変更の照合

- Purpose: この計画の要件と例すべてに印の付いたテストがあり、実装者の変更の記録がこのブランチの比較全体を覆うことを確かめる
- Specification: docs/ir/cli/diff-output.md#REQ-cli-078, docs/ir/merge/symlink.md#REQ-merge-007, docs/ir/backup/rollback-path.md#REQ-backup-003, docs/ir/backup/rollback-path.md#REQ-backup-031, docs/ir/backup/rollback-path.md#REQ-backup-033
- Prerequisites: S1〜S5 がコミット済みである
- May change: .kotowari/changes/implementation.yaml, .kotowari/changes/review.yaml（rebase で入った古いものを消すことだけ）, docs/decision/records/2026-10-05-path-validation.md
- Done when: REQ-cli-078・EX-cli-069・REQ-merge-007・EX-merge-013・EX-merge-014・EX-merge-040・REQ-backup-003・REQ-backup-031・EX-backup-027 の kotowari query の tests が空でない。kotowari check --format json が、このブランチで変えたファイルとこれらの ID にエラーを出さない。全テストが通る。実装者の記録をステージした状態で kotowari changes --base <base> --staged --phase implementation --format json が終了コード 0 になる
- Shown by: check — 次の順に実行する: cargo nextest run --all-features、cargo fmt --all --check、cargo clippy --all-targets --all-features -- -D warnings、各 ID の kotowari query <ID> | jq '.items[0].tests | length'、kotowari check --format json、git merge-base main HEAD で決めた base での kotowari changes --base <base> --staged --phase implementation --format json
- Left to the implementer: 実装者の記録のエントリの分け方と ID
- Stop and hand back if: kotowari changes が、この計画の範囲の外のファイルの変更を未記録として出す
