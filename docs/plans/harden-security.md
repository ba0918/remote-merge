# 計画: レビューで見つかった四つのセキュリティ上の欠陥を直す

## Goal

利用者は、Agent が既存の symlink の先のファイルを壊さず、SSH のホスト鍵を確認なしに信頼せず、他人が置き換えられる Agent のバイナリを実行しない remote-merge を得る。

## Specification

IR は `docs/ir/` にある。この計画は次を扱う。

- 既存の要件と例（意味は変えない）
  - `docs/ir/merge/symlink.md#REQ-merge-001`、`docs/ir/merge/symlink.md#REQ-merge-003`、`docs/ir/merge/symlink.md#REQ-merge-004`
  - `docs/ir/ssh/host-key.md#REQ-ssh-001`、`docs/ir/ssh/host-key.md#REQ-ssh-002`、例 EX-ssh-001、EX-ssh-002、EX-ssh-004
  - `docs/ir/ssh/fallback.md#REQ-ssh-005`、`docs/ir/ssh/sudo.md#REQ-ssh-009`（Agent を起動できないときの既存の扱いとして使う。REQ-ssh-008 は権限昇格の有無だけを定め、配置先の検査の根拠にはならないため、この計画では証明しない）
- この計画の承認で決まり、S1 で判断の記録と IR に書く規則
  - `docs/decision/records/2026-10-05-harden-security.md` の A1〜A6（内容は「Approach and why」に書く）
- レビューの後に利用者が決め、同じ判断の記録と IR に書いた規則
  - A7（Agent の配置の安全の制約を IR の要件にする。`docs/decision/records/2026-09-25-spec-migration.md` の A31 の範囲を絞る）、A8（A4 を置き換え、リモートでハッシュを計算できないときは警告して実行する）、A9 と A13（配置先へ辿るディレクトリの検査。A13 が A9 を置き換える）、A10（途中の親もモード 700 で作る）、A11（A3 を置き換え、グループかその他が書き込めた自分の配置先を直して置き直す）、A12（sudo=true の配置先の検査と書き込みを sudo で行う）、A14（安全でない配置先を sudo=false で使わないときに警告する）

元の指摘は `.agents/artifacts/quality/round1/ledger.json` の routes-1、ssh_agent-1、ssh_agent-2、ssh_agent-3、ssh_agent-7（ssh_agent-7 は routes-1 と同じ原因）で、再現手順と確かめ方は同じディレクトリの `routes.json` の先頭の項目と `ssh_agent.json` の 1・2・3・7 番目の項目にある。このファイルはリポジトリのワークツリーの外（メインのチェックアウトの `.agents/artifacts/`）にあり、読めなくても計画だけで作業できる。

## Approach and why

四つの欠陥はコードの場所も原因も別なので、欠陥ごとに独立した手順にする。ただし IR に書かれていない規則（known_hosts の読み込みの失敗、失効した鍵、Agent の配置先の検査、版が一致したときのハッシュの照合）は、実装の前に S1 で判断の記録と IR に書き、テストがその要件 ID に印を付けられるようにする。

- Agent の symlink（routes-1・ssh_agent-7）: `validate_path` は既存のパスを最後の要素まで正規化するため、既存の symlink がリンク先の実パスに化ける。読み込み・stat・ハッシュは今の振る舞いに頼っているので `validate_path` 自体は変えない。代わりに「親ディレクトリだけを正規化して root_dir の中か確かめ、ファイル名をつなぐ」別の解決関数を作り、`write_file`、`write_file_with_metadata`、`create_symlink_with_metadata` だけがそれを使う。symlink の作り直しはリンクそのものを消して作る（REQ-merge-003）。通常ファイルの書き込みでは、最後の要素が既存の symlink ならエラーにし、リンクもリンク先も変えない。リンクを通常ファイルに置き換えるのは REQ-merge-001 が禁じる種類の変更で、リンク先へ書くのは書き込み先の外を変えるため、残る選択はエラーだけである（A6）。`fs::write` と追記モードの open は渡したパスが symlink なら辿るので、パスの解決を変えるだけでは足りない。親ディレクトリが root_dir の中にあるかの検査は今の `validate_path` と同じに保ち、途中の symlink を辿る振る舞いも変えない（REQ-merge-004 の root_dir の外を指す途中の symlink の扱いはこの計画では扱わない）。なお Agent の書き込みエラーは製品側で Agent を使えないものとして扱われ（`crates/remote-merge/src/runtime/side_io.rs` の `with_agent` 周り）、sudo=false ではその後 SSH の経路で書き直されてリンク先へ届き得る（Out of scope の SSH 経路の問題）、sudo=true では以後の操作が止まる。この計画はこの結果を受け入れ、A6 に書く。
- 鍵種別の違い（ssh_agent-1 の前半）: 同じホストが別の種別でだけ登録されているとき、提示された鍵は known_hosts で確かめられていないので、未知の鍵として verifier に回す（A5）。strict=yes や確認手段がない場合は REQ-ssh-001 で止まり、--yes や strict=no では REQ-ssh-002 のとおり承認できる。「拒否する」にはしない。判定を known_hosts の内容と提示された鍵だけを受け取る純粋関数に切り出し、ファイルの読み書きから分ける。
- known_hosts の読み込みの失敗と @revoked（ssh_agent-1 の後半）: IR は未知の鍵しか定めていない。読み込みの失敗を「ファイルなし」と同じにすると、鍵が変わったホストが未知になり --yes で受け入れられる。そこで、ファイルがないこと以外の読み込みの失敗では接続を止め（A1）、`@revoked` で失効とされた鍵は --yes や strict=no でも受け入れない（A2）。A2 が対象にするのは、今の `host_matches` が一致と判定するホストの書き方（平文・カンマ区切り・ハッシュ形式）の行だけで、`*` などのワイルドカードは今の照合と同じく扱わない。同じ鍵が通常の行にもあるときは失効を優先する。
- 承認後の再接続（ssh_agent-2）: 再接続のハンドラは `skip_host_key_check` で何でも受け入れている。利用者が承認した鍵の種別と base64 を再接続のハンドラに渡し、提示された鍵が完全に一致するときだけ受け入れる。テスト専用の `connect_insecure` が使う `skip_host_key_check` は残す。
- Agent の配置先（ssh_agent-3）: いまは `--version` の実行（`check_version_command`）がどの検査よりも先に配置先のバイナリを動かすので、検査を版の確認より前に置くことが要点である。配置先のディレクトリがなければモード 700 で作り、途中の親もなければモード 700 で作る（A10。sudo=true では既存の配置コマンドと同じく `sudo` を付けて作る）。ディレクトリが symlink のとき、所有者が SSH のセッションの `id -u`（sudo=true のときは 0、つまり root）でないときは、その中のものを何も実行しない（A11）。所有者が一致してグループかその他が書き込めるときは、配置先をモード 700 に直し、中の Agent のバイナリと書き込み途中のファイルを実行せずに除き、版の確認をせずに置き直して、直した配置先のパスを示す警告を出す（A11）。配置先へ辿るディレクトリ（書かれたパスの各段、解決した実際のパスの各段、symlink の連なりの途中のリンクのリンク先とその各段）に、グループかその他が書き込めて sticky bit のないもの、または所有者が root でもセッションの uid（sudo=true では root）でもないものがあれば、中のものを何も実行しない（A13）。設定のユーザー名とは比べない（テスト用 SSH サーバのシェルは設定の `testuser` ではなく開発者の uid で動く）。この検査は独立したコマンドにし、`build_pre_write_command` には入れない。テスト用 SSH サーバは `test -L` と `echo SYMLINK` の両方を含むコマンドに偽の失敗を返す（`crates/remote-merge/tests/contract/ssh_server.rs` の exec の処理）ので、新しい検査のコマンドはこの二つを同時に含めない。同じ理由で、テスト用 SSH サーバでは Agent の置き直しが完了しないので、置き直されたことは Docker の E2E で確かめる。sudo=true のサーバでは、配置先の作成と検査、版の確認とハッシュの計算、書き込みと書き込み後の照合を sudo で行う（A12）。版が一致しても、ローカルの Agent のバイナリを解決できたときは SHA-256 を照合し、一致しなければ版が違うときと同じ配置の経路で置き直す。リモートでハッシュを計算できないときと、ローカルのバイナリを解決できないとき（リモートのターゲット用のバイナリが手元にない、100MB の上限を超えるなど）は照合できないので、配置先の検査を通ったことだけを根拠に実行し、ハッシュを計算できないときはそのことを接続ごとに一度警告する（A8）。解決できないときに実行を止めると、利用者が手で置いた Agent が使えなくなり、Agent を使う既存の契約テスト（ローカルが gnu、リモートの判定が musl、debug ビルドが 100MB を超える）も Agent を起動しなくなるためである。拒否したときは「Agent を起動できない」既存の経路に乗せる。sudo=false なら SSH の経路で続け（REQ-ssh-005）、そのとき安全でない配置先のパスを示す警告を出す（A14）。sudo=true なら止まる（`try_start_agent` の既存の扱い、REQ-ssh-009）。新しい終了コードは決めない。

## Scope of change

- `docs/decision/records/2026-10-05-harden-security.md`（新規）、`docs/decision/records/2026-09-25-spec-migration.md`（A31 から A7 への参照の追加だけ）
- `docs/ir/ssh/host-key.md`（A1・A2 の要件と例の追加だけ）、`docs/ir/ssh/deploy.md`（新規。A7〜A14 の配置先の要件と例）
- `crates/remote-merge-agent/src/agent/file_io.rs`、`crates/remote-merge-agent/src/agent/dispatch.rs`（テストの追加だけ）
- `crates/remote-merge-ssh/src/known_hosts.rs`、`crates/remote-merge-ssh/src/known_hosts_io.rs`、`crates/remote-merge-ssh/src/client.rs`（A12 の sudo の書き込みの追加を含む）
- `crates/remote-merge-agent/src/agent/deploy/`（`deploy_dir.rs`、`transfer.rs`、`verify.rs`、`mod.rs`）、`crates/remote-merge/src/agent/deploy/`（製品側の再公開・wrapper・テスト）、`crates/remote-merge/src/runtime/core.rs`
  - `start_agent_via_ssh` と `deploy_agent_binary` の周りだけ（A14 の警告もここで出す）。`try_start_agent` の失敗時の扱いは変えない
- `crates/remote-merge/tests/contract/ssh_host_key.rs`、`crates/remote-merge/tests/contract/ssh_server.rs`（ホスト鍵の公開鍵を返すアクセサの追加だけ。既存の偽の応答は変えない）、`crates/remote-merge/tests/contract/ssh_fallback.rs`、`crates/remote-merge/tests/contract/ssh_external_link.rs`、新しい契約テストのファイルを足す場合は `crates/remote-merge/tests/contract.rs` への登録、Agent を使う既存の契約テストの準備（`crates/remote-merge/tests/contract/scan_listing_cli.rs` を含む）
- `tests/container-e2e/tests/sudo.rs`（配置先の E2E の追加）
- `.kotowari/changes/implementation.yaml`、`.kotowari/changes/review.yaml`

## Step order and prerequisites

S1 を最初に行う。S4・S6・S7 のテストは S1 で書く要件 ID に印を付けるからである。S3 の後に S4 を行う。S4 は S3 で切り出した判定関数に場合を足す。S6 の後に S7 を行う。どちらも `start_agent_via_ssh` の同じ流れを変え、S7 のハッシュの照合は S6 の検査を通ったディレクトリでだけ意味を持つ。S2 と S5 はほかの手順に依存しない。S8 は最後に行う。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | （判断の記録と IR の追加。A1〜A4 の要件を作る） | — |
| S2 | REQ-merge-001、REQ-merge-003（印なしの単体テスト） | — |
| S3 | REQ-ssh-001（契約テストの印） | — |
| S4 | S1 で A1・A2 から作る要件（契約テストの印） | — |
| S5 | REQ-ssh-001（印なしの単体テスト） | — |
| S6 | S1 で A3 から作る要件（契約テストの印） | — |
| S7 | S1 で A4 から作る要件（契約テストの印） | — |
| S8 | （変更の照合と最後の検査） | — |

S2 と S5 のテストは Agent crate と SSH crate の単体テストで、`.kotowari/config.yaml` の `tests.files` に含まれないため印を付けない。そのため S8 の後も `kotowari query REQ-merge-003` と `kotowari query REQ-merge-004` の `tests` は空のままで、それを失敗としない。REQ-ssh-001 は S3 の契約テストで印を得る。EX-ssh-001・EX-ssh-004・EX-ssh-009・REQ-ssh-002・REQ-ssh-005・REQ-ssh-009 は既存のテストの印のままで、この計画は振る舞いを変えないことを既存のテストが通ることで示す。

## Left to the implementer

- 新しい関数・型・列挙の名前と置き場所。1ファイルが長くなる場合の補助関数への切り出し
- 配置先の検査を行うシェルの書き方。ただし POSIX の sh で動き、GNU 独自の機能（`find -perm /mode`、`stat -c` など）に頼らないこと。古い OpenSSH・Linux の環境（`bench/legacy-ssh/`）でも動く必要がある
- 判断の記録と新しい要件の文面。意味は「Approach and why」と各手順のとおりにする
- 単体テストを `file_io.rs` と `dispatch.rs` のどちらの段で書くか

## Stop conditions

- 既存のテストのうち、Agent がリンク先へ書き込むこと、または `validate_path` がリンク先を返すことを期待するものが見つかった
- 判断の記録や IR に、A1〜A6 と食い違う既存の決定があった
- `kotowari check` が、S1 で足した要件について、テストの印がないこと以外の指摘を出し、文面の修正で消えない
- この計画の承認の時点で利用者が A1〜A4 のどれかを採らなかった。その規則を扱う手順（S4、S6、S7）とそれに対応する S1 の部分を行わずに返す
- Docker がない環境で `scripts/run-container-e2e.sh` を実行できない。この場合は止めずに、未確認として報告に書く

## Test command

```sh
cargo nextest run --all-features
```

契約テストの一部（`ssh_host_key.rs` のテスト用 SSH サーバを使うもの）は `test-utils` feature がないと動かないため、`--all-features` を付ける（`lefthook.yml` の pre-push と同じ）。整形と lint は `cargo fmt --all --check` と `cargo clippy --all-targets --all-features -- -D warnings`。

## Out of scope

- SSH 経路の `rm -f` が失敗を黙って捨てる問題（別の指摘）と、SSH 経路の書き込み（`openssl base64 -d -out`）が symlink を辿ること
- `stat_file` と `read_file_chunked` の symlink の扱い
- `ReplaceSymlinkWithFile` などのマージの流れと REQ-merge-001 の関係
- sudo=true での新規配置が書き込めない問題、配置先のバイナリ自体が symlink であることの拒否
- 鍵が変わったホストの扱いを IR に書くこと（今の拒否の振る舞いは変えない）

## Steps

### S1: 判断の記録と IR に新しい規則を書く

- Purpose: IR にない四つの規則（A1〜A4）と、既存の要件から導いた二つの選択（A5・A6）を、根拠と決めた役割とともに残し、A1〜A4 を IR の要件にする
- Specification: `docs/ir/ssh/host-key.md#REQ-ssh-001`, `docs/ir/ssh/host-key.md#REQ-ssh-002`, `docs/ir/ssh/fallback.md#REQ-ssh-005`, `docs/ir/ssh/sudo.md#REQ-ssh-009`, `docs/ir/merge/symlink.md#REQ-merge-001`
- Prerequisites: この計画が利用者に承認されコミットされていること。kotowari スキルの IR と判断の記録の場面を読んでいること
- May change: `docs/decision/records/2026-10-05-harden-security.md`, `docs/decision/records/2026-09-25-spec-migration.md`, `docs/ir/ssh/host-key.md`, `docs/ir/ssh/deploy.md`
- Done when: 判断の記録に A1（ファイルがないこと以外の known_hosts の読み込みの失敗で接続を止める）、A2（@revoked の鍵は --yes や strict=no でも受け入れない）、A3（配置先のディレクトリをモード 700 で作り、symlink・ログインユーザー〔sudo=true では root〕以外の所有・グループかその他の書き込みのどれかに当たれば中のものを実行せず、Agent を起動できない既存の扱いに乗せる）、A4（版が一致してもローカルのバイナリと SHA-256 が一致しないかリモートで計算できなければ既存の配置の経路で置き直し、ローカルのバイナリを解決できなければ A3 の検査を通ったことだけを根拠に実行する）、A5（別の種別でだけ登録されたホストの鍵は未知の鍵として扱う。根拠 REQ-ssh-001・REQ-ssh-002）、A6（Agent は最後の要素が既存の symlink のパスへ通常ファイルを書かずエラーにし、symlink の作り直しはリンク自体に行う。そのエラーで製品側が Agent を使えないものとし、sudo=false では SSH の経路へ移り、sudo=true では止まる既存の扱いを受け入れる。根拠 REQ-merge-001・REQ-merge-003）があり、A2・A3・A4 には「Approach and why」に書いた対象の範囲と例外（ワイルドカードを扱わない、失効を優先する、`id -u` と比べる、解決できないときは A3 だけで実行する）も書かれている。A1〜A6 の decided_by は利用者（この計画の承認による、承認の日付）である。レビューの後の利用者の判断で、A7〜A14（「Specification」の一覧）が decided_by を利用者として加わり、A3 は A11、A4 は A8、A9 は A13 に置き換えられ、置き換えが Revisions に書かれている。A31 には A7 への参照が Revisions にある。A1・A2 が `docs/ir/ssh/host-key.md` に、A7〜A14 が新しい `docs/ir/ssh/deploy.md` に、次の空き番号（書いた時点では REQ-ssh-010 以降）の要件として source 付きで書かれ、`kotowari check --format json` がこれらのファイルにテストの印がないこと以外の指摘を出さない
- Shown by: artifact — `docs/decision/records/2026-10-05-harden-security.md` と追加した要件、形式は `kotowari check --format json` と `kotowari query <新しい ID>` で確かめる
- Left to the implementer: 要件と例の文面と分け方（一つの規則を一つ以上の要件にしてよい）、`docs/ir/ssh/deploy.md` の見出し
- Stop and hand back if: 既存の判断の記録が A1〜A6 のどれかと逆のことを決めている、または A3・A4 を書くと REQ-ssh-005 か REQ-ssh-009 の意味を変える必要が出る

### S2: Agent が既存の symlink の先を壊さない

- Purpose: Agent の symlink の作成と通常ファイルの書き込みを、最後の要素の既存の symlink を辿らずリンクそのものに対して行う
- Specification: `docs/ir/merge/symlink.md#REQ-merge-003`, `docs/ir/merge/symlink.md#REQ-merge-001`
- Prerequisites: none
- May change: `crates/remote-merge-agent/src/agent/file_io.rs`, `crates/remote-merge-agent/src/agent/dispatch.rs`
- Done when: root の下に `rw/real.txt`（中身あり）と `ro/link -> ../rw/real.txt`（テストの準備で `std::os::unix::fs::symlink` で直接作る）がある状態で、`ro/link` へリンク先 `other` の symlink を作ると `real.txt` の中身が変わらず `read_link(ro/link)` が `other` になる。同じ状態で `ro/link` へ `write_file` と `write_file_with_metadata` で書くとエラーになり、`real.txt` の中身と `ro/link` のリンク先が変わらない。`validate_path` と Agent の既存のテストは変えずに通る
- Shown by: test — Agent crate の単体テストで、既存の symlink の作り直し、`write_file` と `write_file_with_metadata` の拒否を一つずつ RED → GREEN にする
- Left to the implementer: none
- Stop and hand back if: 既存のテストや呼び出し元（rollback を含む）が、最後の要素が symlink のパスへの通常ファイルの書き込みをリンク先へ届くものとして期待している

### S3: 別の種別でだけ登録されたホストの鍵を未知の鍵として確かめる

- Purpose: 同じホストが別の鍵種別でだけ known_hosts にあるとき、提示された鍵を確認なしに受け入れて追記するのをやめ、未知の鍵の扱い（verifier）に回す
- Specification: `docs/ir/ssh/host-key.md#REQ-ssh-001`, `docs/ir/ssh/host-key.md#REQ-ssh-002`
- Prerequisites: none
- May change: `crates/remote-merge-ssh/src/known_hosts.rs`, `crates/remote-merge-ssh/src/known_hosts_io.rs`, `crates/remote-merge/tests/contract/ssh_host_key.rs`
- Done when: known_hosts の内容と提示された鍵から判定する純粋関数が、別の種別でだけ登録されたホストを未知と判定する。テスト用 SSH サーバ（ed25519 の鍵を提示する）に対し、HOME の `.ssh/known_hosts` に `[127.0.0.1]:<port> ssh-rsa <任意の base64>` だけを置き、`strict_host_key_checking = "yes"` で status を実行すると、既存の `unknown_key_is_rejected_when_standard_input_gives_no_answer` と同じ終了コード 2 で止まり、サーバがコマンドを受け取らず（`server.commands()` が空）、known_hosts のバイト列が変わらない
- Shown by: test — 純粋関数の単体テスト（SSH crate、印なし）と、`crates/remote-merge/tests/contract/ssh_host_key.rs` の契約テスト一つ（`// @kotowari[REQ-ssh-001]`）をそれぞれ RED → GREEN にする
- Left to the implementer: 判定関数の戻り値の形
- Stop and hand back if: russh が known_hosts の内容によって提示される鍵の種別を変え、テスト用 SSH サーバでこの状況を作れない

### S4: 読めない known_hosts と失効した鍵で接続を止める

- Purpose: ファイルがないこと以外の known_hosts の読み込みの失敗を「ファイルなし」と同じにせず接続を止め、@revoked で失効とされた鍵を受け入れない
- Specification: `docs/decision/records/2026-10-05-harden-security.md#A1`, `docs/decision/records/2026-10-05-harden-security.md#A2`, `docs/ir/ssh/host-key.md#REQ-ssh-002`
- Prerequisites: S1、S3
- May change: `crates/remote-merge-ssh/src/known_hosts.rs`, `crates/remote-merge-ssh/src/known_hosts_io.rs`, `crates/remote-merge/tests/contract/ssh_host_key.rs`, `crates/remote-merge/tests/contract/ssh_server.rs`
- Done when: known_hosts がない場合は今までどおり未知のホストとして扱われる。known_hosts が UTF-8 として不正な内容のとき、--yes を付けた status は成功せず、サーバがコマンドを受け取らず、known_hosts のバイト列が変わらない。known_hosts にテスト用 SSH サーバのホスト鍵を `@revoked [127.0.0.1]:<port> ssh-ed25519 <その鍵>` として置いたとき（同じ鍵の通常の行が並んでいても）、--yes を付けた status は成功せず、サーバがコマンドを受け取らず、known_hosts が変わらない
- Shown by: test — `crates/remote-merge/tests/contract/ssh_host_key.rs` の契約テスト二つ（S1 で A1・A2 から作った要件 ID の印）をそれぞれ RED → GREEN にする。テスト用 SSH サーバに公開鍵を返すアクセサを足してよい
- Left to the implementer: 読み込みの失敗の伝え方（`Result` を返す形など）。失敗時の文言は既存の接続エラーの形に合わせ、テストで固定しない
- Stop and hand back if: UTF-8 として不正な known_hosts 以外に、テスト環境で再現できる読み込みの失敗が必要になる、または @revoked の行にワイルドカードの書き方を受け付ける必要が出る

### S5: 承認した鍵だけで再接続する

- Purpose: 未知の鍵を利用者が承認した直後の再接続で、承認した鍵と完全に一致する鍵だけを受け入れる
- Specification: `docs/ir/ssh/host-key.md#REQ-ssh-001`
- Prerequisites: none
- May change: `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge-ssh/src/known_hosts.rs`
- Done when: 承認した鍵 K（種別と base64）を持たせた再接続用のハンドラの `check_server_key` が、K には受け入れを返し、別の鍵 K′（`PrivateKey::random` で作る）には受け入れを返さない。既存の `unknown_key_is_rejected_when_standard_input_gives_no_answer`（"yes" と答えて接続が続く場合を含む）が通り、承認後の再接続が新しいハンドラで成功することを示す
- Shown by: test — SSH crate の非同期の単体テスト（印なし）で、K′ を受け入れないことを RED → GREEN にする
- Left to the implementer: 承認した鍵をハンドラに持たせる形
- Stop and hand back if: russh の `check_server_key` をテストから直接呼べず、別の鍵を提示するテスト用 SSH サーバが必要になる

### S6: 安全でない配置先の中のものを実行しない

- Purpose: Agent の配置先のディレクトリをモード 700 で作り、symlink・ログインユーザー（sudo=true では root）以外の所有・グループかその他の書き込みのどれかに当たるときは、版の確認を含めて中のものを何も実行しない
- Specification: `docs/decision/records/2026-10-05-harden-security.md#A3`, `docs/ir/ssh/fallback.md#REQ-ssh-005`
- Prerequisites: S1
- May change: `crates/remote-merge-agent/src/agent/deploy/`, `crates/remote-merge/src/agent/deploy/`, `crates/remote-merge/src/runtime/core.rs`, `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge/tests/contract/ssh_fallback.rs`, `crates/remote-merge/tests/contract/ssh_external_link.rs`, `crates/remote-merge/tests/contract/scan_listing_cli.rs`, `crates/remote-merge/tests/contract/ssh_server.rs`, `crates/remote-merge/tests/contract.rs`, `crates/remote-merge/tests/contract/` の新しい契約テストのファイル, `tests/container-e2e/tests/sudo.rs`
- Done when: 検査は `check_version_command` より前に行われる。テスト用 SSH サーバ（`TestServer::filesystem_with_agent`）で、配置先 `<deploy_dir>/remote-merge-testuser` をモード 0777 で先に作り、その中に正しい版を出力し呼ばれると目印のファイルを作るスクリプトを置くと、merge は SSH の経路で成功して書き込み先が更新され、目印のファイルが作られない。配置先がないときの最初の配置でディレクトリのモードが 0700 になる。生成した検査のシェルを実際の `sh` で一時ディレクトリに対して走らせると、0700 で自分の所有なら通り、0777 のもの、symlink のもの、期待する uid と所有者が違うもの（期待する uid を自分以外にしたもの）は拒否される。sudo=true では期待する uid が 0 になる。`ssh_fallback.rs` と `ssh_external_link.rs` の Agent を使う既存のテストは、準備したディレクトリのモードを明示し、Agent が実際に起動したこと（`server.commands()` に ` agent --root ` を含むコマンドがある）を確かめる一行を足したうえで通る
- Shown by: test — 0777 の配置先の契約テスト（S1 で A3 から作った要件 ID の印）、最初の配置のモードの契約テスト（同じ印）、検査のシェルの単体テストをそれぞれ RED → GREEN にする
- Left to the implementer: 検査を版の確認と同じ SSH の実行にまとめるか別にするか、期待する uid をシェルに渡す形。POSIX の sh で書き、`test -L` と `echo SYMLINK` を同時に含めない
- Stop and hand back if: Docker の sudo の E2E（`tests/container-e2e/tests/sudo.rs`、root 所有でモード 755 の事前配置）がこの検査で拒否される、またはテスト用 SSH サーバのシェルで umask などのためにモードを再現できない

### S7: 版が一致してもハッシュを照合してから実行する

- Purpose: 配置先のバイナリの版が一致したときも、ローカルの Agent のバイナリを解決できればそれと SHA-256 を照合し、一致しなければ置き直しの経路に回す
- Specification: `docs/decision/records/2026-10-05-harden-security.md#A4`, `docs/ir/ssh/fallback.md#REQ-ssh-005`
- Prerequisites: S1、S6
- May change: `crates/remote-merge-agent/src/agent/deploy/`, `crates/remote-merge/src/agent/deploy/`, `crates/remote-merge/src/runtime/core.rs`, `crates/remote-merge/tests/contract.rs`, `crates/remote-merge/tests/contract/` の新しい契約テストのファイル
- Done when: テスト用 SSH サーバで、モード 0700 の自分の配置先に、正しい版を出力し `agent` 引数で呼ばれると目印のファイルを作るスクリプトを置き、`REMOTE_MERGE_AGENT_BINARY` に中身の違う小さな実行可能なスタブを指定して merge すると、目印のファイルが作られず（置かれたスクリプトが Agent として起動されない）、merge が成功する（置き直しはテスト用 SSH サーバの偽の応答で失敗し、SSH の経路で続く）。ハッシュの応答の判定は、一致なら実行、不一致と `__UNSUPPORTED__` なら置き直し、ローカルのバイナリを解決できなければ照合なしで実行、を返す。Agent を使う既存の契約テスト（`ssh_fallback.rs` の EX-ssh-010 のテスト、`ssh_external_link.rs`）は、S6 で足した「Agent が起動した」の確認を含めて通る
- Shown by: test — 目印の契約テスト（S1 で A4 から作った要件 ID の印）と、ハッシュの照合の判定の単体テスト（不一致と `__UNSUPPORTED__` の二つ）をそれぞれ RED → GREEN にする
- Left to the implementer: ハッシュを版の確認と同じ SSH の実行で取るか別に取るか
- Stop and hand back if: Agent を使う既存の契約テストで、ローカルのバイナリが解決できてしまい（環境変数や agents ディレクトリの候補による）ハッシュが一致せず Agent が起動しなくなる、または Docker の sudo の E2E で照合が一致しない

### S8: 変更を照合して最後の検査を通す

- Purpose: ブランチ全体の変更を a22ff0b と照合する実装者の記録を作り、独立したレビューの記録と合わせて統合の条件を満たす
- Specification: `docs/ir/merge/symlink.md#REQ-merge-003`, `docs/ir/ssh/host-key.md#REQ-ssh-001`, `docs/ir/ssh/fallback.md#REQ-ssh-005`
- Prerequisites: S1〜S7
- May change: `.kotowari/changes/implementation.yaml`, `.kotowari/changes/review.yaml`
- Done when: 前の変更の `implementation.yaml` と `review.yaml`（基準が別のコミット）が除かれ、比較の基準 `a22ff0bf0551fb6dc2945d776cf18ffc2abe0025` に対する実装者の記録が `implementation.yaml` にある。S2・S3・S5 の項目は既存の要件を根拠とする `existing`、S1・S4・S6・S7 の項目は判断の記録 A1〜A14 のうち該当するものを参照する `new` である。実装と別のレビュー担当が `review.yaml` を作った後、`kotowari check --format json` がこのブランチで変えたファイルとこの計画の ID に誤りを出さず、`kotowari changes --base a22ff0bf0551fb6dc2945d776cf18ffc2abe0025 --head <ブランチの先頭の完全な ID> --phase review --format json` が終了コード 0 になる。S1 で作った要件と REQ-ssh-001 は `kotowari query` で `tests` が空でない。REQ-merge-003 は印のない単体テストで確かめたため `tests` が空でもよい
- Shown by: check — `cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`、`kotowari check --format json`、`kotowari changes --base a22ff0bf0551fb6dc2945d776cf18ffc2abe0025 --head <ブランチの先頭の完全な ID> --phase review --format json`、Docker があれば `scripts/run-container-e2e.sh`
- Left to the implementer: 記録の項目の分け方と ID
- Stop and hand back if: 記録の作成後にコードか IR が変わった（両方の記録を除いて照合し直す）、または `kotowari changes` が `change_record_invalid` のような記録の形以外の理由で止まる
