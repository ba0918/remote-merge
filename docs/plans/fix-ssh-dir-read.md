# Plan: SSH でディレクトリが空のファイルとして読める不具合を直す

## Goal

SSH の側の一件ずつの読み込みでディレクトリがエラーになり、diff が中身の空なディレクトリと include の外のディレクトリを差分なしと示し、SSH の側だけ参照先がない symlink をエラーにし、通常ファイルとディレクトリ symlink の組をどちらの向きでも配下の子パスで示す。

## Specification

IR は `docs/ir/`。この計画が扱う要件と例:

- `docs/ir/cli/directory-paths.md#REQ-cli-025`（ディレクトリを末尾の "/" の有無によらず同じに比べる。中身の空なディレクトリの既存テストが守る）
- `docs/ir/cli/symlink-diff.md#REQ-cli-022`（参照先がないか読めない symlink はエラー終了し、空ファイルと同一扱いしない）
- `docs/ir/cli/symlink-diff.md#REQ-cli-021` と EX-cli-061（ディレクトリ symlink と通常ファイルの組で配下を示す。EX-cli-061 は左が symlink の向きで、この計画は逆の向きを REQ-cli-021 で確かめる）
- `docs/ir/config/filters.md#REQ-config-004` と `docs/ir/cli/directory-paths.md`（include の外のディレクトリの diff。S3 で要件を足す）
- 閉じる問題記録: `docs/ir/cli/FLAGS.md` の FLAG-cli-033、FLAG-cli-044、FLAG-cli-045

FLAG-cli-033 の答えは利用者が 2026-10-05 に決めた: include の外のディレクトリを指定した diff は、配下を include で絞った結果に比べるファイルがないものとして、差分なし（"0 file(s) with changes"）・終了コード 0 にする。全経路をこれに揃える。

## Approach and why

SSH の読み込み `read_file_bytes`（`crates/remote-merge-ssh/src/client.rs`）は `openssl base64 -in <path>` を `exec_strict` で走らせる。openssl はディレクトリ（とディレクトリを指す symlink）を読むと "Is a directory" を出すのに終了コード 0 で空を返すため、SSH の側だけディレクトリが空のファイルとして読める（OpenSSL 3.0.13 で確かめた。存在しないファイル、参照先のない symlink、途中が通常ファイルのパスは終了コード 1）。ローカルとエージェントの読み込みはディレクトリでエラーになるので、SSH をそれに揃える。コマンドを組み立てる関数を切り出し、`openssl base64 -in P && test ! -d P` の形にする。openssl を先に置くのは、openssl 自身の "Can't open" などのエラーをそのまま出すためで、もう一つ、契約テストの SSH サーバ（`crates/remote-merge/tests/contract/ssh_server.rs`）が `openssl base64 -in` で始まるコマンドを読み込みとみなしているためでもある。

この修正だけを入れると、`crates/remote-merge/tests/cli_diff.rs` の `empty_directory_has_same_no_diff_result_with_or_without_slash`（REQ-cli-025）が "not found on either side" と終了コード 2 で落ちる（確かめた）。diff のパスを指定した走査（`crates/remote-merge/src/cli/diff.rs` の、パスごとに `inspect_path`・`fetch_children`・`read_file_bytes_tolerant` を呼ぶループ）は、左右とも子がないパスの存在を読み込みの成否で決めており、空のディレクトリはこれまで SSH の側の誤った読み込みでだけ「ある」とされていた。

そこで diff は、ディレクトリであることを子の一覧の取得（`fetch_children`）の成否で判定する。左右とも一覧が取れて（ディレクトリで）どちらも子がないとき、そのパスを差分のない既存のディレクトリとして扱う。`fetch_children` は include で絞った一覧を返すので、include の外のディレクトリもこの扱いになり、それが利用者の決めた FLAG-cli-033 の答えと一致する。片側だけがディレクトリの組は今の読み込みによる判定のまま残す。

この判定には、SSH の一覧の取得がディレクトリでないパスで失敗することが要る。ローカルの `fetch_children`（`crates/remote-merge-engine/src/local_io.rs`）は通常ファイル・存在しないパス・参照先のない symlink でエラーを返すが、SSH の `fetch_remote_children`（`crates/remote-merge/src/runtime/remote_io.rs`）は `client.list_dir` を通り、`find` を終了コードを見ない `exec` で走らせるため、それらで空の一覧を返す（`find P/ -maxdepth 1 -mindepth 1` 自体は通常ファイル・参照先のない link・存在しないパスで終了コード 1 を返す）。これを、ディレクトリでなければエラーを返すようにローカルと揃える。リモートの `fetch_children`（`crates/remote-merge/src/runtime/target_io.rs`）はエージェントを通らず常にこの SSH の経路を使う。`client.list_dir` はリモートのルートの一覧（`crates/remote-merge/src/runtime/core.rs` の `fetch_remote_tree`）にも使われているので、そちらの振る舞いは変えない。厳格にしないと、SSH 同士の比較で内容の違う通常ファイルが両側とも「子のないディレクトリ」と判定され、差分が隠れる。

一覧の取得を厳格にすると、diff の symlink の分岐（`diff.rs` の、読み込みが失敗した側だけ `fetch_children(...).ok()` で子を取る箇所）で、SSH の側の参照先がない symlink の子の一覧が `Some(空)` でなく `None` になり、"symlink target unreadable" のエラー（終了コード 2）に進む。これが REQ-cli-022 の求める振る舞いで、FLAG-cli-044（SSH の側だけ参照先がない symlink がエラーにならず空として比べられる）の原因はこの一覧の取得の終了コードの無視と読める。同じ理由で、SSH の側の symlink の参照先が大きすぎるか読めないファイルのときも、今の「子のないディレクトリ」としての扱いからエラー終了に変わる。これも REQ-cli-022 の「読めない参照先を空ファイルと同一扱いしない」に合う。

`TargetPath` にディレクトリの種類を足す方法は採らない。エージェントの通信の型（`crates/remote-merge-protocol`）が変わり、版を上げてエージェントを配備し直す必要があるため。

逆向きの組（左がローカルの通常ファイル `shared`、右が SSH のディレクトリ symlink `shared`）は、SSH の読み込みの修正の後、"Not a directory (os error 20)" で終了コード 2 になる（確かめた）。右の子 `shared/child.txt` を左で調べる `diff.rs` の `path_escapes_root` → `core.inspect_path(...)?`（`--follow-external-links` のときは `link_target_for_diff` の `inspect_path`）で、ローカルの `inspect_local_path` が `symlink_metadata` の NotADirectory を NotFound と区別してエラーにするためと読める。`inspect_path` はバックアップの保存（`runtime/core.rs`）、存在の確認と親ディレクトリの確認（`runtime/side_io.rs`）、ロールバックの復元からも呼ばれ、途中が通常ファイルのパスの扱いを変えるとそれらの振る舞いも変わるため、ローカルとエージェントの共有のパスの検査は変えない。代わりに diff がディレクトリを子のパスに展開するとき、ディレクトリでないと分かっている側にはその子がないものとし、その側の子のパスを調べも読みもしない。ディレクトリでないと分かっている側は、子の一覧が取れず中身を通常ファイルとして読めた側、参照先を読めない symlink でそのパスの誤り（終了コード 2）を既に記録した側、既にないものとした側の下の子パスに限る。子の一覧の取得に失敗しただけの側（読む権限のないディレクトリ、SSH の一覧の取得の失敗）はないものとせず、これまでどおりその側でも子を調べて読む。エラーの種類や文言には頼らないので、経路によらず同じに働く。

ディレクトリを子の一覧で判定するため、ローカルの `fetch_children`（`crates/remote-merge-engine/src/local_io.rs`）は利用者の入力したパスで子を一覧する。ローカルの他の操作と同じく、一覧の前にパスが root_dir の内側にあるかを確かめ、絶対パスや `..` で root_dir の外のディレクトリを一覧しない。

## Scope of change

- `crates/remote-merge-ssh/src/client.rs`（読み込みのコマンドの組み立て、ディレクトリでなければ失敗する子の一覧の取得、その単体テスト）
- `crates/remote-merge/src/runtime/remote_io.rs`（`fetch_remote_children`）
- `crates/remote-merge/src/cli/diff.rs`（パスを指定した走査のディレクトリの判定と、ディレクトリでないと分かっている側の子パスをないものとする扱い）
- `crates/remote-merge-engine/src/local_io.rs`（`fetch_children` が一覧の前にパスが root_dir の内側にあるかを確かめる）
- `crates/remote-merge/tests/cli_diff.rs`
- `docs/ir/cli/directory-paths.md`（include の外のディレクトリの要件を足す）
- `docs/ir/cli/FLAGS.md`、新しい決定記録 `docs/decision/records/2026-10-05-ssh-directory-read.md`
- `.kotowari/changes/implementation.yaml`（実装者が書く。`review.yaml` は独立したレビューが書き、この計画のステップでは書かない）
- 上のファイルの整形と clippy の指摘の修正

## Step order and prerequisites

S1 → S2 → S3 → S4 → S5 の順に進める。S1 は SSH の読み込みの修正、SSH の一覧の取得の厳格化、diff のディレクトリの判定を一つのコミットにする。読み込みの修正だけでは既存のテストが落ち、判定だけでは SSH 同士の差分が隠れるため、分けると途中のコミットで全体のテストが通らないか、誤った振る舞いが入る。S2 は S1 の後でないと逆向きの組が別の理由（空のファイルとしての読み込み）で失敗し、直したことを確かめられない。S3 は S1 の振る舞いを要件として書く。S4 は S1〜S3 の後の振る舞いで問題記録を閉じる。S5 は全体の確認と変更の記録。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-cli-025, REQ-cli-022 | なし（REQ-cli-025 の既存テスト `empty_directory_has_same_no_diff_result_with_or_without_slash` が守る） |
| S2 | REQ-cli-021 | なし（EX-cli-061 は既存のテストのまま通ること） |
| S3 | S3 で足す include の外のディレクトリの要件, REQ-cli-025 | なし |
| S4 | なし（問題記録と決定記録） | なし |
| S5 | REQ-cli-025, REQ-cli-022, REQ-cli-021, S3 で足す要件 | EX-cli-061 |

## Left to the implementer

- SSH の子の一覧の取得をディレクトリでなければ失敗させる手段（`list_dir` とは別の関数にするか、引数で切り替えるか、終了コードの見方、コマンドの形）。`fetch_remote_tree` のルートの一覧の振る舞いは変えない
- diff のディレクトリの判定を書く場所と関数の分け方
- 新しい関数とテストの名前
- S3 で足す要件の ID と文の言い回し（意味は Specification の利用者の決定のとおり）

## Stop conditions

- 既存のテストが、この計画の振る舞いの変更以外の理由で落ちる
- 子の一覧の取得の成否でディレクトリを判定できない経路が見つかる（エージェントの通信の型を変えなければならないなど）
- 片側だけがディレクトリの組など、この計画が扱わない組み合わせの振る舞いを決めないと既存のテストが通らない

## Test command

```sh
cargo nextest run
```

## Out of scope

- 片側だけが空のディレクトリ、空のディレクトリと通常ファイルの組の diff の振る舞い（SSH の修正の副作用で変わりうるが仕様がない。変わったら観察として報告し、テストで固定しない）
- SSH のバッチ読み込み（`crates/remote-merge-ssh/src/batch_read.rs` の `cat ... 2>/dev/null`）がディレクトリや存在しないファイルを空として返すこと。直すのは一件ずつの `read_file_bytes` だけ
- `TargetPath` にディレクトリの種類を足すこと、エージェントの通信の型の変更
- `openssl base64` がディレクトリ以外の読み込みの失敗（I/O エラーなど）で終了コード 0 を返す場合の検出
- `docs/testing/diff-links-test-cleanup.md` の FLAG-cli-044・045 への言及（過去の整理の過程の記録なので書き換えない）

## Steps

### S1: SSH でディレクトリを読むとエラーになり、diff はディレクトリを一覧の取得で判定する

- Purpose: SSH の一件ずつの読み込みがディレクトリでエラーになり、SSH の子の一覧の取得がディレクトリでないパスで失敗し、diff が左右とも子のないディレクトリを差分なしと判定するようにする
- Specification: `docs/ir/cli/directory-paths.md#REQ-cli-025`, `docs/ir/cli/symlink-diff.md#REQ-cli-022`
- Prerequisites: ブランチ fix-ssh-dir-read の作業ツリーがこの計画のコミットだけを main に足した状態であること
- May change: `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge/src/runtime/remote_io.rs`, `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge-engine/src/local_io.rs`, `crates/remote-merge/tests/cli_diff.rs`
- Done when: 生成した読み込みのコマンドを `sh -c` で一時ディレクトリに対して走らせると失敗し、通常ファイルに対しては中身を base64 で返す。左がローカルの通常ファイル、右が SSH の参照先のない symlink の `diff <path> --format json` が終了コード 2 を返し、errors にそのパスを載せる。既存の `empty_directory_has_same_no_diff_result_with_or_without_slash` が、読み込みの修正の後に一度落ちてから diff の修正で通り（落ちたときの出力を報告に貼る）、SSH 同士の内容の違う通常ファイルを差分ありとする既存の `req_cli_062_a_configured_server_as_ref_makes_a_three_way_diff`（`crates/remote-merge/tests/contract/diff_ref_cli.rs`）を含めて全体のテストが通る
- Shown by: test — RED は二つ。`client.rs` の単体テスト（ディレクトリの読み込みが失敗する、通常ファイルの読み込みが中身を返す。`sh -c` で実際のシェルに走らせる既存の `listing_a_directory_link_reads_its_children` と同じ形）と、`cli_diff.rs` に `@kotowari[REQ-cli-022]` を付けた新しいテスト（SSH の側だけ参照先がない symlink がエラー終了する。一覧の取得の厳格化の RED になる）。回帰の守りは既存の二つのテスト。全体を一つのコミットにする
- Left to the implementer: 一覧の取得を失敗させる手段と、diff の判定の書き方（計画の Left to the implementer のとおり）
- Stop and hand back if: 空のディレクトリを差分なしにするのに、片側だけがディレクトリの組の振る舞いを決める必要が出る。新しい REQ-cli-022 のテストが一覧の取得の厳格化の後も通らない

### S2: 通常ファイルとディレクトリ symlink の組をどちらの向きでも配下の子パスで示す

- Purpose: 左がローカルの通常ファイルで右が SSH のディレクトリ symlink の組でも、右の子を入口からの子パスで示すようにする
- Specification: `docs/ir/cli/symlink-diff.md#REQ-cli-021`
- Prerequisites: S1
- May change: `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge/tests/cli_diff.rs`
- Done when: 左にローカルの通常ファイル `shared`、右に SSH の子 `child.txt` を持つディレクトリ symlink `shared` を置いた `diff shared --format json` が終了コード 1 を返し、files に `shared/child.txt` と左の本文を含む。diff はディレクトリでないと分かっている側（Approach のとおり）の子パスだけをないものとし、子の一覧の取得に失敗しただけの側では子を調べて読む。ローカルとエージェントの共有の `inspect_path` は変えない
- Shown by: test — `cli_diff.rs` に `@kotowari[REQ-cli-021]` を付けた新しいテスト `regular_file_against_directory_link_shows_both_kinds_of_content`（既存の `directory_link_against_regular_file_shows_both_kinds_of_content` の左右を入れ替えた形。EX-cli-061 の印は付けない）で RED を示す。参照先のない symlink とディレクトリの組の既存の REQ-cli-022 のテストも通り続ける
- Left to the implementer: none
- Stop and hand back if: 新しいテストが "Not a directory" 以外の理由で落ちる、または通すのに共有の `inspect_path` の振る舞いを変える必要が出る

### S3: include の外のディレクトリの diff を要件にする

- Purpose: 利用者が決めた FLAG-cli-033 の答え（include の外のディレクトリの diff は差分なし・終了コード 0）を要件として書き、テストで確かめる
- Specification: `docs/ir/config/filters.md#REQ-config-004`, `docs/ir/cli/directory-paths.md#REQ-cli-025`
- Prerequisites: S1
- May change: `docs/ir/cli/directory-paths.md`, `docs/decision/records/2026-10-05-ssh-directory-read.md`, `crates/remote-merge/tests/cli_diff.rs`
- Done when: `directory-paths.md` に、include の外の既存ディレクトリを指定した diff は配下を include で絞った結果に比べるファイルがないものとして差分なし・終了コード 0 にするという要件があり、その source が新しい決定記録の合意（decided_by: 利用者、2026-10-05）を指し、左右に include の外のディレクトリ（子のファイルを持つ）を置いた diff が "0 file(s) with changes" と終了コード 0 を返すことを、その要件の ID の印を付けたテストが確かめる
- Shown by: test — `cli_diff.rs` に新しい要件の ID の印を付けたテスト。`kotowari check` が `directory-paths.md` と決定記録に error を出さない
- Left to the implementer: 要件の ID と文の言い回し（意味は変えない）
- Stop and hand back if: S1 の後でもそのテストが通らず、通すのに S1 の判定以外の振る舞い（include の外かの判定など）を足す必要が出る

### S4: FLAG-cli-033・044・045 を閉じる

- Purpose: 直した矛盾と決めた論点の問題記録を閉じ、その根拠を決定記録に残す
- Specification: `docs/ir/cli/symlink-diff.md#REQ-cli-021`, `docs/ir/cli/symlink-diff.md#REQ-cli-022`
- Prerequisites: S1, S2, S3
- May change: `docs/ir/cli/FLAGS.md`, `docs/decision/records/2026-10-05-ssh-directory-read.md`
- Done when: 決定記録に、FLAG-cli-045 を閉じる合意（根拠: コードが両方の向きで REQ-cli-021 を満たすようになった）、FLAG-cli-044 を閉じる合意（根拠: コードが SSH の側でも REQ-cli-022 を満たすようになった）、FLAG-cli-033 を閉じる合意（S3 の要件を指す）があり、どれも why と decided_by（045・044 は AI と利用者の指示「FLAGS を 0 にする」、033 は利用者）を持ち、`FLAGS.md` から三つが除かれている
- Shown by: artifact — `docs/decision/records/2026-10-05-ssh-directory-read.md` と `docs/ir/cli/FLAGS.md`。`kotowari check` がこの二つのファイルに error を出さない。決定記録の形は `docs/decision/records/2026-10-04-cli-diff-flag-resolution.md` にならう
- Left to the implementer: none
- Stop and hand back if: 閉じる三つのほかに、この計画の変更で記述が正しくなくなった FLAG が見つかる

### S5: 全体の確認と変更の記録

- Purpose: この計画の変更がすべての確認を通り、実装者の変更の記録があることを示す
- Specification: `docs/ir/cli/directory-paths.md#REQ-cli-025`, `docs/ir/cli/symlink-diff.md#REQ-cli-022`, `docs/ir/cli/symlink-diff.md#REQ-cli-021`
- Prerequisites: S1, S2, S3, S4
- May change: `.kotowari/changes/implementation.yaml`、および S1〜S3 の May change のファイルの整形と clippy の指摘の修正
- Done when: 下のコマンドがすべて成功し、`kotowari query` で REQ-cli-021・REQ-cli-022・REQ-cli-025 と S3 で足した要件の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、`implementation.yaml` が比較元 a7fd6df6f029e4df3ca1dcb898b4a4ad88785d45 からこのブランチの変更を覆い、FLAG-cli-044・045 の解消を gap（disposition fixed、決定記録と REQ-cli-021・REQ-cli-022 の IR を参照）、FLAG-cli-033 の決定を gap（disposition recorded、決定記録と S3 の要件の IR を参照）として記録している
- Shown by: check — `cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run`、`kotowari check --format json`、`kotowari changes --base a7fd6df6f029e4df3ca1dcb898b4a4ad88785d45 --staged --phase implementation --format json`
- Left to the implementer: none
- Stop and hand back if: `kotowari check` が、このブランチで変えていないファイルの error だけで終了コード 0 にならない（その場合は報告して進める。直さない）
