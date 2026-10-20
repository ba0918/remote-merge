# 設定に書いた使えない SSH の方式の扱いと仕様の突き合わせ

## Context

host_key_algorithms に書いた "ssh-dss" を警告して申し出ないようにした変更（f55d4a39）について、変わったコードと仕様を突き合わせた。
[REQ-ssh-006](../../ir/ssh/compatibility.md#REQ-ssh-006) は「利用可能な SSH アルゴリズムをサーバごとに明示指定し、その設定を接続に適用できる」とだけ述べ、このツールが使えない名前を書いたときにどうなるかを述べていなかった。[REQ-ssh-007](../../ir/ssh/compatibility.md#REQ-ssh-007) は「その設定に書いたときに無視されずに接続に使われ」る方式を使える方式としていて、無視される名前があることを前提にしていた。
変更が足した単体テストは、"ssh-dss" だけを書くとホスト鍵の方式が書かないときと同じになることを確かめるが、その規則は仕様になかった。また、無視するときの警告は診断の記録の target に出ていて、--log-level error では出なかった。
[一度目のヒントの突き合わせ](./2026-10-20-ssh-algorithm-hints-consistency.md) が範囲の外で見つけ、決めていなかった "ssh-dss" を書いたときの扱いを、ここで決める。

Position: 使えない名前を警告して無視することと、すべて無視されるときは書かないときと同じ方式を申し出ることを REQ-ssh-006 に書き（A1）、その警告を記録の細かさによらず標準エラーに出すようコードを直した（A2）。例を [EX-ssh-063](../../ir/ssh/compatibility.md#EX-ssh-063) として足し、その契約テストに結んだ。

## Agreements

- A1 kex_algorithms・host_key_algorithms・ciphers・mac_algorithms に書いた名前のうち、このツールが実装していない方式の名前（host_key_algorithms ではホスト鍵の証明書の方式の名前を含む）と、host_key_algorithms の "ssh-dss" は、そのサーバに接続するとき、名前ごとにその名前を含む警告を標準エラーに出して無視し、接続に申し出ない。一つの設定に書いた名前がすべて無視されるときは、その設定を書かないときと同じ方式を申し出る。警告の文言は IR で決めない。これを [REQ-ssh-006](../../ir/ssh/compatibility.md#REQ-ssh-006) に書き、例を [EX-ssh-063](../../ir/ssh/compatibility.md#EX-ssh-063) として足す。コードは変えない。
  - why: コード（crates/remote-merge-ssh/src/preferred.rs）の resolve_kex_names・resolve_key_names・resolve_cipher_names・resolve_mac_names は、russh が方式として読めない名前を名前ごとに警告して除く。ホスト鍵では russh が知らない名前も Algorithm::Other として読むので、usable_host_key がそれを Unknown として、"ssh-dss"（Algorithm::Dsa）を DsaSignatureUnverifiable として除く。除いた後に名前が残らなければ None を返し、build_preferred はその設定を書かないときの値（MAC は default_macs、ほかは russh の既定）のままにする。変更が足した単体テスト a_dsa_host_key_algorithm_in_config_is_not_offered（@kotowari[REQ-ssh-006]）は、この最後の規則と "ssh-dss" を申し出ないことを確かめるが、REQ-ssh-006 はどちらも述べていなかったので、テストが確かめる規則を仕様が述べていなかった。"ssh-dss" を除く根拠は [一度目の A3](./2026-10-20-ssh-algorithm-hints-consistency.md#A3) の測定（ホスト鍵を DSA だけにした OpenSSH 4.3p2 の sshd に "ssh-dss" を設定すると "Wrong server signature" で失敗する）で、その前提の ssh-key は russh 0.64.1 に上げた後も 0.7.0-rc.11 で dsa の機能なしで作られる（`cargo tree --locked -e features -i ssh-key --workspace` の機能に "ecdsa" はあるが "dsa" はない）。証明書の方式の名前は Algorithm::Other として読まれ、"Unknown host key algorithm" の警告とともに除かれる（[A4 の依存の更新の突き合わせ](./2026-10-20-update-dependencies-consistency.md#A4) の使い捨てのテストと、下の Verification の測定）。REQ-ssh-007 の「その設定に書いたときに無視されずに接続に使われ」と、使える方式から "ssh-dss" を除くことは、無視される名前をこの文が定めることで REQ-ssh-006 と揃う（hint.rs の usable_host_key は preferred::usable_host_key を使うので、二つは同じ判定である）。警告の文言を決めず、テストが名前だけを確かめることは、include の値の警告（[REQ-config-033](../../ir/config/include.md#REQ-config-033)）と同じ扱いである。警告は接続のたびに client の設定を作るとき（crates/remote-merge-ssh/src/client.rs の build_client_config）に出るので、「接続するとき」と書き、回数は決めない。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A2 A1 の警告は利用者に向けた警告として tracing の target `remote_merge::warning` に出し、記録の細かさの指定（--log-level・RUST_LOG）によらず標準エラーに出す。preferred.rs の五つの警告の target を "remote_merge::ssh::preferred" から変える。
  - why: [F51](./2026-10-12-foundation-5.md#F51)・[F65](./2026-10-12-foundation-5.md#F65)・[F89](./2026-10-12-foundation-5.md#F89) は、IR が表示すると決めた利用者に向けた警告をこの target に出し、記録の細かさで消さないと決めている。A1 で REQ-ssh-006 がこの警告を標準エラーに出すと決めたので、同じ規則が当たる。設定に書いた値を使わずに警告する既存の要件（[REQ-config-038](../../ir/config/filters.md#REQ-config-038) の "../" を含む exclude、[REQ-config-029](../../ir/config/values.md#REQ-config-029) の auth が key のときの password、[REQ-config-024](../../ir/config/filters.md#REQ-config-024) の include の値）も、どれもこの target に出ている。変更の前は五つの警告が "remote_merge::ssh::preferred" に出ていて、--log-level error では出なかった（下の Verification の測定）。ホスト鍵だけでなく四つの設定の警告をすべて変えるのは、A1 が四つの設定に同じ規則を書くからで、一部だけ変えると同じ要件の警告が記録の細かさで出たり出なかったりする。診断のファイルは target が "remote_merge" で始まる記録を残す（crates/remote-merge/src/telemetry/file_filter.rs の own_records）ので、target を変えてもファイルに残る。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- 使い捨ての測定（リポジトリに入れない）: 閉じたポートを指すサーバの ssh_options に host_key_algorithms = ["ssh-dss"] を書いたプロジェクト設定で、製品の `status --left local --right develop` を動かした。変更の後で target を変える前は、既定の細かさでは "WARN remote_merge::ssh::preferred: Unsupported host key algorithm in config (DSA signatures cannot be verified), ignoring algorithm=ssh-dss" が出て、--log-level error では出なかった（どちらも接続の失敗 "Connection refused" で終了コード 2）。target を変えた後は、host_key_algorithms = ["ssh-dss", "ssh-ed25519-cert-v01@openssh.com", "rsa-sha2-256"] と kex_algorithms = ["no-such-kex"] を書くと、--log-level error でも off でも "ssh-dss"・"ssh-ed25519-cert-v01@openssh.com"・"no-such-kex" の警告が "WARN remote_merge::warning:" で出て、"rsa-sha2-256" の警告は出なかった。
- RED: 契約テスト ssh_ignored_algorithms::unusable_algorithm_names_are_warned_about_and_the_connection_uses_the_defaults（四つの設定に使えない名前を一つずつ書き、既定の細かさと --log-level error で status を動かす）は、target を変える前は --log-level error で "ssh-dss" の警告がなく失敗した。
- GREEN: target を変えた後、`cargo nextest run --locked --all-features -p remote-merge --test contract -E 'test(/ssh_ignored_algorithms::/)'` で成功した。
- この突き合わせで決めていないこと: 使える名前が残るとき、書いた名前を既定の前に置き、既定の方式も続けて申し出ること（preferred.rs の単体テスト host_key_algorithms_are_prepended_and_unknown_names_are_ignored が確かめる）は、REQ-ssh-006 にまだ書いていない。A1 はすべて無視されるときだけを述べ、この並びを述べない。
