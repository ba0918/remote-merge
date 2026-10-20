# 依存の更新と仕様の突き合わせ

## Context

依存を上げた変更（russh 0.63、toml 1、similar 3、base64 0.23、dirs 7、digest 0.11 の版の sha2・sha1・hmac と、SHA-256 の16進を一か所で作る整理）について、変わったコードと仕様を突き合わせた。
russh は 0.57.1 より後で既定の MAC から SHA-1 を外した（0.60.3 と 0.63.1 の既定は SHA-2 の MAC だけ）ので、製品は既定の MAC を自分で決めるようになったが、仕様は MAC を設定しないときにどのサーバへつながるかを述べていなかった。
しかも製品の既定は ssh_options のあるサーバにしか使われず、ssh_options のないサーバは SHA-1 の MAC を申し出なくなっていた。
ほかの依存は、利用者から見える振る舞いが変わるか、変わるならそれが仕様の範囲にあるかを、動かして確かめた。

Position: 既定の MAC を REQ-ssh-028 として仕様に書き、ssh_options のないサーバにも製品の既定を使うよう直し、契約テストで確かめた。toml・similar・ホスト証明書・そのほかの更新は使い捨ての比較で確かめ、仕様を変えないと決めた（A2〜A5）。

## Agreements

- A1 サーバの mac_algorithms を設定していないとき（ssh_options がないときを含む）、接続は "hmac-sha2-" で始まる MAC を先に、"hmac-sha1-etm@openssh.com" と "hmac-sha1" をこの順で最後に申し出る。これを [REQ-ssh-028](../../ir/ssh/compatibility.md#REQ-ssh-028) として仕様に書き、ssh_options のないサーバへの接続もこの既定を使うよう直す。
  - why: SHA-1 の MAC を申し出ないと、SHA-2 の MAC を持たないサーバ（OpenSSH 5.9 より前。[REQ-ssh-005](../../ir/ssh/fallback.md#REQ-ssh-005) が挙げる本番の CentOS 5 の OpenSSH 4.3 もこれに当たる）へは mac_algorithms を設定しないとつながらず、利用者に見える。russh 0.57.1 の既定（negotiation.rs の HMAC_ORDER）は SHA-2 の四つの後に SHA-1 の二つを並べ、0.63.1 の既定（SAFE_HMAC_ORDER）は SHA-2 の四つだけになった。依存の更新は既定の MAC を製品で決める default_macs（crates/remote-merge-ssh/src/preferred.rs）を足し、並びを 0.57.1 と同じにしたが、client.rs の build_client_config は ssh_options があるときだけ build_preferred を呼んでいたので、ssh_options のないサーバは russh 0.63.1 の既定のまま SHA-1 の MAC を申し出ず、更新前につながっていた古いサーバにつながらなくなっていた（その更新のコミットの説明が避けると述べた退行）。preferred.rs の単体テストは build_preferred を直に呼ぶので、この呼び出し側の抜けを見なかった。[A36](./2026-09-25-spec-migration.md#A36) は旧資料の全アルゴリズムの対応を約束しないとするだけで、実装した一つの既定を書くことを妨げない。[A4](./2026-09-26-reproducible-test-environments.md#A4) は旧 SSH アルゴリズムを旧サーバー互換性の保証の対象にする。暗号が MAC を使わない AEAD（chacha20-poly1305・aes-gcm）のときは、russh は共通の MAC がなくても合意する（0.63.1 の negotiation.rs の select_mac）ので、確かめる偽サーバは暗号も "aes128-ctr" だけにした。既定の後ろに SHA-1 を置くこと自体は依存の更新の実装者が選び、この記録はそれを仕様に書き、すべての接続で使うことを決める。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A2 toml 1 への更新は仕様を変えない。設定ファイルは TOML 1.1.0 の書き方（インラインテーブルの中の改行と末尾のカンマ、"\e" のエスケープ）も読めるようになり、それは [REQ-config-009](../../ir/config/loading.md#REQ-config-009) の「TOML として読める」に入る。解釈できないときの理由の文言の変化は [REQ-config-039](../../ir/config/loading.md#REQ-config-039) と [REQ-config-043](../../ir/config/loading.md#REQ-config-043) の範囲にある。
  - why: REQ-config-009 は TOML の版を定めていない。製品が使う toml 1.1.8 は TOML 1.1.0 を読む（Cargo.lock の toml_parser 1.1.5+spec-1.1.0）。使い捨ての比較（toml 0.8.23 と 1.1.8 に同じ 14 通りの壊れた設定を serde の deny_unknown_fields の型で読ませた）では、0.8 が断った "\e" のエスケープと、改行と末尾のカンマを含むインラインテーブルの二つを 1.1 は読み、残りの 12 通りはどちらも断った。理由の文言は "duplicate key `password` in table `servers.a`" が "duplicate key" に、"expected newline, `#`" が "unexpected key or value, expected newline, `#`" になるように変わったが、書かれた値（"SENTINEL"）を含む理由は新たに出ず、値を除く処理（crates/remote-merge-config/src/parse_error.rs）が頼る "invalid type: " と "invalid value: " の始まりは変わらなかった。REQ-config-043 は理由にキーの名前を含めてよいとするだけで、含めることを求めない。init が値を書くのに使う toml::Value::String の表示は、引用符・バックスラッシュ・改行・制御文字を含む 11 通りの値で 0.8.23 と 1.1.8 が同じ文字列を出した。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A3 similar 3 への更新は仕様を変えず、既定の Myers をそのまま使う実装者の選択を仕様は妨げない。
  - why: 仕様は diff の形（[REQ-cli-058](../../ir/cli/diff-output.md#REQ-cli-058) の前後 3 行の文脈と塊のまとめ方、[REQ-cli-053](../../ir/cli/diff-output.md#REQ-cli-053) の JSON の形）と、`競合` を変更の行の範囲の重なりとすることを決めるが、どの変更列を選ぶかも、変更列が最短であることも決めていない。使い捨ての比較（2.7.0 と 3.2.0 の TextDiff::from_lines に同じ乱数の入力を渡し、削除と追加の行の数を比べた）では、10 行・3 回の編集の 20000 組のうち 944 組、200 行・20 回の 2000 組のうち 195 組で変更列が違ったが、行の数は同じだった。5000 行・1500 回の 50 組のうち 21 組、3000 行・行の種類 10・800 回の 50 組のうち 7 組では、3.2 の Myers が 2.7 より最大 4 行多い最短でない変更列を返した（3.2.0 の CHANGELOG の、重い探索で最短でない分け方を受け入れる Git 流の変更）。3.2 の RawMyers はどの組でも 2.7 と同じ行の数だった。単独の CR を行の区切りとする分け方は変わらない（"new\rsame\r\nend\n" と "old\rsame\r\nend\n" で 2.7 と 3.2 が同じ変更列を出した）ので、[content-encoding-consistency の A1](./2026-10-16-content-encoding-consistency.md#A1) が前提にする元バイトの行範囲との一致は保たれる。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A4 サーバがホスト証明書を出したときに断る分岐は、利用者が見ることのない守りなので、仕様に書かない。
  - why: russh 0.63.1 のクライアントは、証明書の方式を申し出たとき（Preferred の host_key_certificates が空でないとき）だけ証明書として読み、check_server_key に証明書を渡す（client/kex.rs の host_key_is_certificate、client/mod.rs の 1850 行）。製品の build_preferred は host_key_certificates を russh の既定の空のままにし、host_key_algorithms に書いた証明書の方式の名前（"ssh-ed25519-cert-v01@openssh.com" など）は Algorithm::Other として読まれて警告とともに除かれる（使い捨てのテストで、その設定から作った方式の一覧に証明書の名前がなく host_key_certificates が空であることを確かめた）。どの設定でも証明書は check_server_key に届かないので、分岐が変わっても利用者に見える違いはない。ホスト鍵の確かめの仕様（[REQ-ssh-001](../../ir/ssh/host-key.md#REQ-ssh-001) ほか）は変えない。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A5 dirs 7、既定の機能を外した base64 0.23、digest 0.11 の版の sha2・sha1・hmac、SHA-256 の16進を core の一か所で作る整理は、利用者から見える振る舞いを変えないので、仕様を変えない。
  - why: dirs の Linux と macOS の実装（lin.rs・mac.rs）は 6.0.0 と 7.0.0 で差がなく、dirs-sys も同じ 0.5.0 なので、[REQ-config-045](../../ir/config/loading.md#REQ-config-045) の設定の場所は変わらない。base64 0.23.0・0.23.1 の変更は SIMD の engine と定数の追加と誤りの表示で、製品が使う general_purpose::STANDARD の符号化と復号は変わらない。sha2 0.10.9 の `{:x}` の表示と、0.11.1 の要約を一バイトずつ小文字の2桁にしたもの（core の sha256.rs と同じ表し方）は、4 通りの入力で同じ文字列になったので、保存されたバックアップの置き場所の名前とハッシュの表示は変わらない。読み手から求めるハッシュは 64 KiB ずつ読むので、[REQ-compare-010](../../ir/compare/reading.md#REQ-compare-010) と [REQ-cli-095](../../ir/cli/status.md#REQ-cli-095) の、中身を少しずつ読み全体をメモリに持たないことを保つ。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- RED: 直す前のコード（abfbfa7d）で `cargo nextest run --all-features -p remote-merge --test contract -E 'test(/a_server_without_sha2_macs|sha1_macs_are_offered_last/)'` を動かすと、EX-ssh-060 の契約テストは "SSH connection failed to 127.0.0.1: No common Mac algorithm - ours: [\"hmac-sha2-512-etm@openssh.com\", \"hmac-sha2-256-etm@openssh.com\", \"hmac-sha2-512\", \"hmac-sha2-256\"], theirs: [\"hmac-sha1\"]" で、EX-ssh-061 の契約テストは申し出の最後の二つが "hmac-sha2-512" と "hmac-sha2-256" で失敗した。
- GREEN: build_client_config を直した後、同じ二つと既存の ssh_compatibility の二つ（計 4 件）が成功し、`cargo nextest run --all-features -p remote-merge-ssh` の 94 件も成功した。
- 直した後に default_macs から SHA-1 の二つを一時的に外すと二つとも失敗し、SHA-1 の二つを先頭に置くと EX-ssh-061 の契約テストだけが失敗した（どちらも元に戻した）。契約テストは製品の接続を通して default_macs を見ている。
