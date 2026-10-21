# 設定した SSH の方式を申し出る並び

## Context

[REQ-ssh-006](../../ir/ssh/compatibility.md#REQ-ssh-006) は、kex_algorithms・host_key_algorithms・ciphers・mac_algorithms に書いた名前のうち使えないものを警告して無視することと、すべて無視されるときは書かないときと同じ方式を申し出ることを述べるが、無視されない名前が残るときにどの方式をどの順で申し出るかを述べていなかった。
[ssh-dss を申し出なくしたときの突き合わせ](./2026-10-21-ssh-dss-host-key-config-consistency.md) の Verification が、これを決めていないこととして残した。
コードは書いた名前を書かないときの方式の前に置き、書かないときの方式も続けて申し出る。単体テストはこの並びを確かめていたが、仕様にない規則を確かめていた。

Position: 今のコードの並びを REQ-ssh-006 に書き（A1）、例を [EX-ssh-064](../../ir/ssh/compatibility.md#EX-ssh-064) として足して、接続の申し出を読む契約テストに結んだ。書いた名前だけを申し出る読み方は採らない（R1）。

## Agreements

- A1 四つの設定のそれぞれで、無視されない名前があるときは、その設定の方式として、無視されない名前を書いた順に先に申し出て、その後に、その設定を書かないときに申し出る方式からそれらの名前を除いたものを、その順のまま続けて申し出る。これを [REQ-ssh-006](../../ir/ssh/compatibility.md#REQ-ssh-006) に書き、例を [EX-ssh-064](../../ir/ssh/compatibility.md#EX-ssh-064) として足す。コードは変えない。
  - why: crates/remote-merge-ssh/src/preferred.rs の resolve_kex_names・resolve_key_names・resolve_cipher_names・resolve_mac_names は、無視されない名前を書いた順に集め、その後ろに、書かないときの方式（MAC は default_macs、ほかは russh の既定）のうち集めた名前にないものを順に足す。build_preferred の doc comment も「ユーザー指定のアルゴリズムはデフォルトの先頭に追加される（優先度が最も高くなる）」と書く。kex・cipher・MAC はこれを実装したコミット 6e3d5796 から、host_key_algorithms は取り込みのコミット 85fd7e7d から、この形のまま変わっていない。SSH の方式の取り決めでは、どの種類の方式も、クライアントが申し出た順で最初の、サーバも持つ方式が選ばれる（RFC 4253 の 7.1）。このため、書いた方式はサーバが書かないときの方式も持つときにも選ばれ、書いた方式をサーバが持たないときは続けて申し出た方式でつながる。
  - rejected: 書いた名前だけを申し出る（R1）
  - decided_by: 利用者（勧めを受けた）

## Rejected

- R1 書いた名前だけを申し出て、その設定を書かないときの方式を申し出ない（書いた名前で置き換える）。
  - why: docs/archive/spec.md の「`russh` での実装方針」の概念コード（`kex: vec![kex_algorithms...]`）はこう読めるが、コードはそうしていない。置き換えると、書いた方式をサーバが持たないとき、今はつながる設定でつながらなくなる。また kex_algorithms を置き換えると、鍵交換の既定の一覧にある "ext-info-c" と "kex-strict-c-v00@openssh.com"（Terrapin の攻撃（CVE-2023-48795）への対策の strict kex を申し出る名前）も申し出なくなる（下の Verification で、resolve_kex_names が書いた名前だけを返すよう一時的に変えたときの申し出）。

## Verification

- 契約テスト ssh_compatibility::configured_algorithms_are_offered_first_and_the_unconfigured_offer_follows は、四つの設定を書かないときと EX-ssh-064 の値を書いたときの二つの接続で、クライアントが最初に送る鍵交換の申し出（SSH_MSG_KEXINIT）を読み、書いたときの鍵交換・ホスト鍵・暗号（クライアントからサーバ）・MAC（クライアントからサーバ）の一覧が、無視されない名前の後に、書かないときの一覧からそれらを除いたものが続く並びに等しいことを確かめる。コードを変えない今の実装で、`cargo nextest run --locked --all-features -p remote-merge --test contract -E 'test(/ssh_compatibility::/)'` の 6 件とともに通った。
- このテストが並びに頼ることの確認（使い捨て。コミットに入れない）: resolve_kex_names が書いた名前だけを返すよう一時的に変えると、書いたときの鍵交換の一覧が ["diffie-hellman-group14-sha1", "curve25519-sha256"] だけになって失敗した。書かないときの一覧は "mlkem768x25519-sha256" で始まり "ext-info-c"・"kex-strict-c-v00@openssh.com" で終わる 11 個だった。resolve_mac_names が書いた名前を足さないよう一時的に変えると、MAC の一覧が書かないときと同じ "hmac-sha2-512-etm@openssh.com" で始まる並びになって失敗した。どちらも元に戻した。
- 申し出を読むテストの関数を、MAC だけでなく鍵交換・ホスト鍵・暗号も返すように広げ、[EX-ssh-061](../../ir/ssh/compatibility.md#EX-ssh-061) の契約テストもそれを使う。
