# 古い SSH サーバへの接続

サーバごとのアルゴリズム指定、MAC を設定しないときに申し出る既定の MAC、接続に失敗したときの案内。

## Requirements

### REQ-ssh-006: サーバごとに接続方式を指定できる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A36, docs/decision/records/2026-10-21-ssh-dss-host-key-config-consistency.md#A1, docs/decision/records/2026-10-21-ssh-dss-host-key-config-consistency.md#A2
- verification: unit

利用可能な SSH アルゴリズムをサーバごとに明示指定し、その設定を接続に適用できる。
kex_algorithms・host_key_algorithms・ciphers・mac_algorithms に書いた名前のうち、このツールが実装していない方式の名前（host_key_algorithms ではホスト鍵の証明書の方式の名前を含む）と、host_key_algorithms の "ssh-dss"（このツールは DSA のホスト鍵の署名を確かめられない）は、そのサーバに接続するとき、名前ごとにその名前を含む警告を標準エラーに出して無視し、接続に申し出ない。
一つの設定に書いた名前がすべて無視されるときは、その設定を書かないときと同じ方式を申し出る。
警告の文言は IR で決めず、実装者が英語で書き、テストはその文言を固定しない。

### REQ-ssh-007: 接続失敗の理由を示す
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A36, docs/decision/records/2026-10-20-ssh-algorithm-hints-consistency.md#A1, docs/decision/records/2026-10-20-ssh-algorithm-hints-consistency.md#A2, docs/decision/records/2026-10-20-ssh-algorithm-hints-consistency.md#A3, docs/decision/records/2026-10-20-ssh-algorithm-hints-consistency.md#A4, docs/decision/records/2026-10-20-ssh-algorithm-hints-consistency-2.md#A2
- verification: unit

暗号方式が合わず SSH 接続に失敗したとき、理由と変更できる設定のヒントを表示する。
ヒントは、"[servers.<name>.ssh_options]" の行と、その下に合わなかった方式の設定を書いた TOML の一行からなる設定の例を含む。設定の名前は、鍵交換なら kex_algorithms、ホスト鍵なら host_key_algorithms、暗号なら ciphers、MAC なら mac_algorithms で、値はサーバが申し出た方式のうちこのツールが使えるものの名前を、申し出の順に並べた文字列の配列である。
その一行は、設定の名前に " = [" を続け、名前をそれぞれ二重引用符で囲んで ", " で区切って並べ、"]" で閉じる。
このツールが使える方式は、その設定に書いたときに無視されずに接続に使われ、選ばれても接続が守られて成り立つ方式である。鍵交換・暗号・MAC の "none" と、このツールが署名を確かめられないホスト鍵の方式 "ssh-dss" は含めない。
サーバが申し出た方式にこのツールが使えるものがないとき、また合わなかったのが圧縮の方式のとき（ssh_options に圧縮の設定はない）は、ヒントを出さず理由だけを表示する。

### REQ-ssh-028: MAC を設定しないときは SHA-1 の MAC を最後に申し出る
- kind: state_driven
- source: docs/decision/records/2026-10-20-update-dependencies-consistency.md#A1
- verification: unit

サーバの mac_algorithms を設定していないとき（[servers.名前.ssh_options] がないときを含む）、接続は "hmac-sha2-" で始まる MAC を先に、"hmac-sha1-etm@openssh.com" と "hmac-sha1" をこの順で最後に申し出る。
SHA-2 の MAC を持たず "hmac-sha1" を持つサーバ（OpenSSH 5.9 より前）にも、mac_algorithms を設定せずにつながる。

## Examples

```gherkin
@id=EX-ssh-011 @about=REQ-ssh-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A36
Scenario: 古いサーバだけに互換設定を与える
Given 二つの接続先のうち一方に対応する暗号方式が明示設定されている
When そのサーバに接続する
Then そのサーバに指定した設定が適用される

@id=EX-ssh-012 @about=REQ-ssh-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A36
Scenario: 別のサーバには互換設定がない
Given 二つ目の接続先には暗号方式を明示設定していない
When 二つ目のサーバに接続する
Then 一つ目のサーバだけの設定は適用されない

@id=EX-ssh-063 @about=REQ-ssh-006 @source=docs/decision/records/2026-10-21-ssh-dss-host-key-config-consistency.md#A1
Scenario: 使えない方式の名前だけを設定する
Given 接続先のサーバのホスト鍵は Ed25519 で、そのサーバの host_key_algorithms には "ssh-dss" だけを、kex_algorithms・ciphers・mac_algorithms にはこのツールが実装していない方式の名前を一つずつ書いている
When そのサーバに接続するコマンドを実行する
Then 書いた名前ごとにその名前を含む警告が標準エラーに出て、それらの設定を書かないときと同じくそのサーバにつながる

@id=EX-ssh-013 @about=REQ-ssh-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A36,docs/decision/records/2026-10-20-ssh-algorithm-hints-consistency-2.md#A3
Scenario: 鍵交換が合わない
Given 接続先のサーバの鍵交換方式は "diffie-hellman-group14-sha1" だけで、そのサーバの kex_algorithms は設定していない
When SSH 接続を試す
Then 不一致の理由と、kex_algorithms に "diffie-hellman-group14-sha1" だけを並べた設定の例が表示される

@id=EX-ssh-062 @about=REQ-ssh-007 @source=docs/decision/records/2026-10-20-ssh-algorithm-hints-consistency.md#A1
Scenario: 暗号が合わない
Given 接続先のサーバの暗号は "aes128-cbc" だけで、そのサーバの ciphers は設定していない
When SSH 接続を試す
Then 不一致の理由と、ciphers に "aes128-cbc" だけを並べた設定の例が表示される

@id=EX-ssh-060 @about=REQ-ssh-028 @source=docs/decision/records/2026-10-20-update-dependencies-consistency.md#A1
Scenario: SHA-2 の MAC を持たない古いサーバ
Given 接続先のサーバの MAC は "hmac-sha1" だけで、暗号は MAC を使う "aes128-ctr" だけであり、そのサーバの mac_algorithms は設定していない
When そのサーバに接続してリモートのコマンドを実行する
Then 接続は成り立ちコマンドの出力が返る

@id=EX-ssh-061 @about=REQ-ssh-028 @source=docs/decision/records/2026-10-20-update-dependencies-consistency.md#A1
Scenario: 既定の MAC の並び
Given サーバの mac_algorithms を設定していない
When そのサーバに接続する
Then 接続が申し出る MAC は "hmac-sha2-" で始まる名前が先に並び、最後の二つが "hmac-sha1-etm@openssh.com" と "hmac-sha1" である
```
