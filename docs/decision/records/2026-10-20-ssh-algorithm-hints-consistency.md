# 方式が合わないときの設定のヒントと仕様の突き合わせ

## Context

SSH の方式が合わず接続に失敗したときのヒントを、誤りの文字列でなく russh の誤りの種類から作り、例にサーバが申し出た方式のうちこのツールが使えるものを並べる変更（44a26f7f）について、変わったコードと仕様を突き合わせた。
仕様の [REQ-ssh-007](../../ir/ssh/compatibility.md#REQ-ssh-007) は「理由と変更できる設定のヒントを表示する」とだけ述べ、ヒントに何を並べるかも、ヒントを出さない場合も述べていなかった。
しかも「このツールが使える」の判定は、ホスト鍵の "ssh-dss"（設定しても署名を確かめられずつながらない）と鍵交換の "none"（接続を守らない）を例に並べていた。

Position: ヒントの形と並べる方式（A1）、ヒントを出さない場合（A2）を REQ-ssh-007 に書き、"ssh-dss"（A3）と鍵交換の "none"（A4）を例から外すようコードを直した。暗号が合わないときの例を [EX-ssh-062](../../ir/ssh/compatibility.md#EX-ssh-062) として足し、その契約テストに結んだ。

## Agreements

- A1 方式が合わないときのヒントには、"[servers.<name>.ssh_options]" の行と、その下に合わなかった方式の設定（鍵交換は kex_algorithms、ホスト鍵は host_key_algorithms、暗号は ciphers、MAC は mac_algorithms）を書いた TOML の一行からなる設定の例を含め、値にはサーバが申し出た方式のうちこのツールが使えるものを申し出の順に並べる。これを [REQ-ssh-007](../../ir/ssh/compatibility.md#REQ-ssh-007) に書き、暗号の例を [EX-ssh-062](../../ir/ssh/compatibility.md#EX-ssh-062) として足す。
  - why: 変更前の例は決まった一覧で、並べた方式はどれも既定に含まれるものだったので、合わないサーバの助けにならなかった。変更後の例は、CentOS 5 の OpenSSH 4.3p2（testenv-centos5 のイメージ）に既定の設定でつなぐと `kex_algorithms = ["diffie-hellman-group-exchange-sha1", "diffie-hellman-group14-sha1", "diffie-hellman-group1-sha1"]` を示し、この一行をそのまま設定すると鍵交換が成り立って認証まで進んだ（三つのどれか一つだけを設定しても同じ）。設定の名前は [TBL-config-004](../../ir/config/loading.md#TBL-config-004) の ssh_options の四つのキーで、[REQ-ssh-006](../../ir/ssh/compatibility.md#REQ-ssh-006) の設定にそのまま書ける。ヒントの単体テストと契約テストが `ciphers = ["aes128-cbc"]` のような一行を確かめるので、その形を仕様に書いて約束にする。並べる方式をこのツールが実装しているものに限ることは、実装されていない方式を保証として記さない [A36](./2026-09-25-spec-migration.md#A36) に沿う。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A2 サーバが申し出た方式にこのツールが使えるものがないとき、また合わなかったのが圧縮の方式のときは、ヒントを出さず理由だけを表示する。この例外を [REQ-ssh-007](../../ir/ssh/compatibility.md#REQ-ssh-007) に書く。
  - why: REQ-ssh-007 が約束するのは変更できる設定のヒントで、この二つの場合には変えてつながる設定がない。このツールが知らない方式の名前は、設定に書いても警告とともに無視される（crates/remote-merge-ssh/src/preferred.rs の resolve_kex_names ほかと、その単体テスト test_build_preferred_unknown_kex_ignored）。圧縮の方式は [TBL-config-004](../../ir/config/loading.md#TBL-config-004) の ssh_options にキーがない。それでも決まった一覧を出すと、つながらない設定を勧め、[A36](./2026-09-25-spec-migration.md#A36) が避ける、実装されていない方式を保証として記すことになる。理由（russh の "No common … algorithm - ours: …, theirs: …"。双方が申し出た方式の一覧を含む）は出るので、A36 の why の「診断可能な接続を保つ」は保たれる。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A3 ホスト鍵の方式 "ssh-dss" は、このツールが使える方式に数えず、ヒントに並べない。
  - why: 製品が使う ssh-key 0.7.0-rc.11 は dsa の機能なしで作られ（`cargo tree -e features -i ssh-key --workspace` に dsa がない）、KeyData の署名の確かめの DSA の分岐は `#[cfg(feature = "dsa")]` で外れる。russh は "ssh-dss" を Algorithm::Dsa として読むので、変更の判定（Algorithm::Other でないこと）では使える方式に数えていた。ホスト鍵を DSA だけにした OpenSSH 4.3p2 の sshd に、鍵交換だけを設定してつなぐと、理由 "No common Key algorithm - ours: [... "ssh-rsa"], theirs: ["ssh-dss"]" とともに `host_key_algorithms = ["ssh-dss"]` を勧め、その設定でつなぐと "Wrong server signature" で失敗した（ホスト鍵が RSA と DSA の sshd に "ssh-dss" を先に設定しても同じ）。既定のホスト鍵の方式は "ssh-rsa" まで含むので、古い OpenSSH でホスト鍵が合わないのはホスト鍵が DSA だけのときに限られ、ヒントはその唯一の場合に誤っていた。直した後は、同じ sshd に理由だけを示した。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A4 鍵交換・暗号・MAC の "none"（暗号の内部の名前 "clear" を含む）は、どれもヒントに並べない。
  - why: 暗号の none は中身が平文で流れ、MAC の none は改ざんを検知できない（変更の実装者が選び、コードのコメントに書いた理由）。鍵交換の none も russh は方式として知っていて（kex の KEXES）、選ぶと鍵交換を飛ばし、空の値から鍵を作り、ホスト鍵の署名を確かめない（russh 0.63.1 の client/kex.rs の skip_exchange の分岐）ので、同じ理由が当たるが、変更は鍵交換の none を除いていなかった（単体テストで `kex_algorithms = ["none"]` を勧めることを確かめた）。申し出の一覧は鍵交換の前に守られずに届くので、間にいる者が書き換えられる。none を勧めると、守られていないネットワーク越しの応答から、会話を読まれコマンドを差し込まれうる設定へ利用者を導くことになり、[A11](./2026-10-12-foundation.md#A11) が直す対象とする、信頼しない入力からのコマンド注入に当たる。[REQ-ssh-005](../../ir/ssh/fallback.md#REQ-ssh-005) が挙げる最も古いサーバの OpenSSH 4.3p2 が申し出る鍵交換・暗号・MAC に none はない（KEXINIT を読んで確かめた）ので、外しても支える範囲のサーバのヒントは変わらない。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- RED: "ssh-dss" を外す前のコードで、ヒントの単体テスト no_hint_when_the_servers_only_host_key_is_dsa は `host_key_algorithms = ["ssh-dss"]` のヒントが返って失敗した。鍵交換の none を外す前のコードで、no_hint_when_the_only_key_exchange_offered_is_none は `kex_algorithms = ["none"]` のヒントが返って失敗した。
- GREEN: 直した後、`cargo nextest run --locked -p remote-merge-ssh -E 'test(/hint::/)'` の 9 件が成功した。製品をつくり直してホスト鍵を DSA だけにした sshd につなぐと、ヒントのない理由だけが出た。
- 使い捨ての測定（リポジトリに入れない）: testenv-centos5 のイメージで、ホスト鍵を DSA だけにした sshd と、RSA と DSA の sshd を立て、製品の status で接続の失敗の表示を読んだ。KEXINIT は sshd に版の文字列を送って最初の packet の方式の一覧を読んだ。
- この突き合わせの範囲の外で見つけたこと（決めていない）: host_key_algorithms に "ssh-dss" を書くと、設定は警告なしに受け入れられ、"Wrong server signature" で失敗する（preferred.rs の resolve_key_names は Algorithm::Other だけを除く）。
