# 方式が合わないときの設定のヒントと仕様の二度目の突き合わせ

## Context

[一度目の突き合わせ](./2026-10-20-ssh-algorithm-hints-consistency.md) で、SSH の方式が合わず接続に失敗したときのヒントの形と、ヒントを出さない場合を [REQ-ssh-007](../../ir/ssh/compatibility.md#REQ-ssh-007) に書き、ホスト鍵の "ssh-dss" と鍵交換の "none" を例から外した。
その後の範囲（9388a2d8 から 3f572f38）のコードと仕様を読み直し、記録した根拠が決定を支えるかも確かめた。
一度目の A1 の why は、変更前のヒントを誤って述べていた。
また、テストが固定するヒントの一行の書き方を、REQ-ssh-007 は述べていなかった。

Position: A1 で一度目の A1 の根拠を直し、A2 でヒントの一行の書き方を REQ-ssh-007 に書いた。

## Agreements

- A1 [一度目の A1](./2026-10-20-ssh-algorithm-hints-consistency.md#A1) の決定は変えず、その why の初めの一文（変更前の例は決まった一覧で、並べた方式はどれも既定に含まれるものだったので、合わないサーバの助けにならなかった）を次の文に読み替える: 変更前は、誤りの文言が "kex" を含むときだけ鍵交換の決まった一覧 "curve25519-sha256"・"diffie-hellman-group14-sha256"・"diffie-hellman-group14-sha1" を示し、暗号・ホスト鍵・MAC が合わないときはヒントを出さなかった。一覧のうち "diffie-hellman-group14-sha1" は既定に含まれないので、それを申し出るサーバには役に立ったが、それを申し出ないサーバには既定にある方式しか勧めなかった。
  - why: 範囲の基点（9388a2d8）の crates/remote-merge-ssh/src/hint.rs は、決まった一覧を誤りの文字列の語で選んでいた（鍵交換は "no matching key exchange" か "kex"、ホスト鍵は "no matching host key" か "host key algorithm"、暗号は "no matching cipher" か "encryption"、MAC は "no matching mac" か "message authentication code"）。russh 0.63.1 の方式が合わないときの文言は "No common {kind:?} algorithm - ours: …, theirs: …"（src/lib_inner.rs の NoCommonAlgo）で、種類は "Kex"・"Key"・"Cipher"・"Mac"・"Compression" と出るので、当たるのは鍵交換の "kex" だけである。russh の既定の鍵交換（src/negotiation.rs の SAFE_KEX_ORDER。製品の build_preferred は kex_algorithms を設定しないときは既定のまま使う）は "diffie-hellman-group14-sha1" を含まない。使い捨ての測定（リポジトリに入れない）: 基点のコードを git archive で取り出し、3f572f38 の契約テストの偽サーバと SSH の互換の契約テストを入れて `cargo nextest run --locked --all-features -p remote-merge --test contract -E 'test(/ssh_compatibility::an_incompatible_/)'` を動かすと、鍵交換が "diffie-hellman-group14-sha1" だけの偽サーバには理由 "No common Kex algorithm - ours: ["mlkem768x25519-sha256", …, "diffie-hellman-group14-sha256"], theirs: ["diffie-hellman-group14-sha1"]" とともに `kex_algorithms = ["curve25519-sha256", "diffie-hellman-group14-sha256", "diffie-hellman-group14-sha1"]` を示し、暗号が "aes128-cbc" だけの偽サーバには理由 "No common Cipher algorithm - ours: [...], theirs: ["aes128-cbc"]" だけを示してヒントを出さなかった。一度目の A1 の why が CentOS 5 の OpenSSH 4.3p2 について測った鍵交換の申し出にも "diffie-hellman-group14-sha1" があるので、変更前のヒントはそのサーバにも役に立った。一度目の A1 の決定は why の残り（申し出の順に使えるものを並べたヒントがそのサーバでそのままつながったこと、設定の名前が ssh_options の四つのキーであること、テストが確かめる形を約束にすること、A36）で支えられるので、変えなくてよい。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A2 ヒントの設定の一行は、設定の名前に " = [" を続け、名前をそれぞれ二重引用符で囲んで ", " で区切って並べ、"]" で閉じる形とし、これを [REQ-ssh-007](../../ir/ssh/compatibility.md#REQ-ssh-007) に書く。コードは変えない。
  - why: コード（crates/remote-merge-ssh/src/hint.rs の ssh_algorithm_hint）は、名前を `format!("{name:?}")` で二重引用符に囲み、`", "` でつないで `{setting} = [...]` に入れる。[EX-ssh-062](../../ir/ssh/compatibility.md#EX-ssh-062) の契約テスト（crates/remote-merge/tests/contract/ssh_compatibility.rs の an_incompatible_cipher_reports_the_reason_and_a_configuration_hint）とヒントの単体テストは、この形の一行（`ciphers = ["aes128-cbc"]`）をそのまま確かめる。REQ-ssh-007 は「TOML の一行」「文字列の配列」とだけ書いていたので、テストが固定する書き方を仕様が約束していなかった（同じ値は `ciphers=['aes128-cbc']` とも書ける）。[一度目の A1](./2026-10-20-ssh-algorithm-hints-consistency.md#A1) は、テストが確かめるこの形の一行を仕様に書いて約束にすると決めていたので、書き方を足すのはその具体化で、TOML の一行で文字列の配列という既存の文と矛盾しない。並べる名前は、russh が名前を知っている方式（鍵交換・暗号・MAC は名前の一覧、ホスト鍵は ssh-key の決まった名前）に限られ、どれも英小文字・数字・"-"・"@"・"." だけなので、`{:?}` の書き出しは TOML の基本文字列と同じになる。
  - decided_by: consistency phase（実装と仕様の整合確認役）
