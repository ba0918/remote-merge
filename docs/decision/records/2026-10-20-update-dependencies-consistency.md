# 依存の更新と仕様の突き合わせ

## Context

依存を上げた変更（russh 0.63、toml 1、similar 3、base64 0.23、dirs 7、digest 0.11 の版の sha2・sha1・hmac と、SHA-256 の16進を一か所で作る整理）について、変わったコードと仕様を突き合わせた。
russh は 0.60.2 で既定の MAC から SHA-1 を外したので、製品は既定の MAC を自分で決めるようになったが、仕様は MAC を設定しないときにどのサーバへつながるかを述べていなかった。
しかも製品の既定は ssh_options のあるサーバにしか使われず、ssh_options のないサーバは SHA-1 の MAC を申し出なくなっていた。
ほかの依存は、利用者から見える振る舞いが変わるか、変わるならそれが仕様の範囲にあるかを、動かして確かめた。

Position: 既定の MAC を REQ-ssh-028 として仕様に書き、ssh_options のないサーバにも製品の既定を使うよう直し、契約テストで確かめた。

## Agreements

- A1 サーバの mac_algorithms を設定していないとき（ssh_options がないときを含む）、接続は "hmac-sha2-" で始まる MAC を先に、"hmac-sha1-etm@openssh.com" と "hmac-sha1" をこの順で最後に申し出る。これを [REQ-ssh-028](../../ir/ssh/compatibility.md#REQ-ssh-028) として仕様に書き、ssh_options のないサーバへの接続もこの既定を使うよう直す。
  - why: SHA-1 の MAC を申し出ないと、SHA-2 の MAC を持たないサーバ（OpenSSH 5.9 より前。[REQ-ssh-005](../../ir/ssh/fallback.md#REQ-ssh-005) が挙げる本番の CentOS 5 の OpenSSH 4.3 もこれに当たる）へは mac_algorithms を設定しないとつながらず、利用者に見える。russh 0.57.1 の既定（negotiation.rs の HMAC_ORDER）は SHA-2 の四つの後に SHA-1 の二つを並べ、0.63.1 の既定（SAFE_HMAC_ORDER）は SHA-2 の四つだけになった。依存の更新は既定の MAC を製品で決める default_macs（crates/remote-merge-ssh/src/preferred.rs）を足し、並びを 0.57.1 と同じにしたが、client.rs の build_client_config は ssh_options があるときだけ build_preferred を呼んでいたので、ssh_options のないサーバは russh 0.63.1 の既定のまま SHA-1 の MAC を申し出ず、更新前につながっていた古いサーバにつながらなくなっていた（その更新のコミットの説明が避けると述べた退行）。preferred.rs の単体テストは build_preferred を直に呼ぶので、この呼び出し側の抜けを見なかった。[A36](./2026-09-25-spec-migration.md#A36) は旧資料の全アルゴリズムの対応を約束しないとするだけで、実装した一つの既定を書くことを妨げない。[A4](./2026-09-26-reproducible-test-environments.md#A4) は旧 SSH アルゴリズムを旧サーバー互換性の保証の対象にする。暗号が MAC を使わない AEAD（chacha20-poly1305・aes-gcm）のときは、russh は共通の MAC がなくても合意する（0.63.1 の negotiation.rs の select_mac）ので、確かめる偽サーバは暗号も "aes128-ctr" だけにした。既定の後ろに SHA-1 を置くこと自体は依存の更新の実装者が選び、この記録はそれを仕様に書き、すべての接続で使うことを決める。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- RED: 直す前のコード（abfbfa7d）で `cargo nextest run --all-features -p remote-merge --test contract -E 'test(/a_server_without_sha2_macs|sha1_macs_are_offered_last/)'` を動かすと、EX-ssh-060 の契約テストは "SSH connection failed to 127.0.0.1: No common Mac algorithm - ours: [\"hmac-sha2-512-etm@openssh.com\", \"hmac-sha2-256-etm@openssh.com\", \"hmac-sha2-512\", \"hmac-sha2-256\"], theirs: [\"hmac-sha1\"]" で、EX-ssh-061 の契約テストは申し出の最後の二つが "hmac-sha2-512" と "hmac-sha2-256" で失敗した。
- GREEN: build_client_config を直した後、同じ二つと既存の ssh_compatibility の二つ（計 4 件）が成功し、`cargo nextest run --all-features -p remote-merge-ssh` の 94 件も成功した。
- 直した後に default_macs から SHA-1 の二つを一時的に外すと二つとも失敗し、SHA-1 の二つを先頭に置くと EX-ssh-061 の契約テストだけが失敗した（どちらも元に戻した）。契約テストは製品の接続を通して default_macs を見ている。
