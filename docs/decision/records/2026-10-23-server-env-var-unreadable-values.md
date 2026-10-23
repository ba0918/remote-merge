# サーバごとの環境変数の値を読めないときの扱い

## Context

パスワードとパスフレーズを渡す環境変数の名前をそろえた変更（1e134ed5）について、名前を作る関数を呼んで環境変数を読むコードを、仕様ともう一度突き合わせた。
どちらのコードも、値が UTF-8 として読めない環境変数を、ない環境変数と同じに扱う（crates/remote-merge-ssh/src/password.rs の resolve_password_with と、crates/remote-merge-ssh/src/client.rs の load_secret_key_with）。
[REQ-config-019](../../ir/config/values.md#REQ-config-019) と [REQ-config-050](../../ir/config/values.md#REQ-config-050) は、環境変数が「空でなく設定されていれば」その値を使うとし、設定されていないものとして扱う値には空の値だけを挙げていて、読めない値をどう扱うかを述べていなかった。
この記録は、IR にその扱いを書き足したことと、その根拠を残す。

Position: 値が UTF-8 として読めない環境変数を設定されていないものとして扱うことを、REQ-config-019 と REQ-config-050 に書き足した（A1）。コードとテストは変えていない。

## Agreements

- A1 auth が "password" のサーバのパスワードを渡す環境変数 REMOTE_MERGE_PASSWORD_<サーバ名> と、auth が "key" のサーバの鍵のパスフレーズを渡す環境変数 REMOTE_MERGE_KEY_PASSPHRASE_<サーバ名> は、値が UTF-8 として読めないときも、空のときと同じく設定されていないものとして扱う。これを [REQ-config-019](../../ir/config/values.md#REQ-config-019) と [REQ-config-050](../../ir/config/values.md#REQ-config-050) に書く。コードは変えない。
  - why: どちらのコードも、std::env::var が値を UTF-8 として読めないときに返す誤り（VarError::NotUnicode）を、環境変数がないときと同じに扱う。パスワードは crates/remote-merge-ssh/src/password.rs の 22-23 行（`if let Ok(val) = read_env(&env_key) && !val.is_empty()`）で、設定の password に回る。この扱いは単体テスト password::tests::unreadable_environment_falls_back_to_config が確かめている。パスフレーズは crates/remote-merge-ssh/src/client.rs の 709 行（`read_env(&env_key).ok().filter(|s| !s.is_empty())`）で、その値を試さずに端末で尋ねる。実行ファイルでも同じ振る舞いを測った（Verification）。REQ-config-019 の元の [2026-09-28-adopt-config-values.md の A6](./2026-09-28-adopt-config-values.md#A6) は実装を仕様として追認したもので、取り込んだ初期状態（85fd7e7d）の resolve_password もすでに `if let Ok(val) = std::env::var(&env_key)` で読めない値を設定の password に回していた。REQ-config-050 は [2026-10-23-server-env-var-names-consistency.md の A2](./2026-10-23-server-env-var-names-consistency.md#A2) が、コードを変えずにコードの振る舞いを書いたものである。どちらも読めない値を使うと決めたものではなく、コードの振る舞いを書く決定だったので、書き足しても既存の IR の意図と矛盾しない。
  - rejected: 読めない値もバイト列のまま使う。SSH のパスワード認証はパスワードを UTF-8 の文字列として送り（RFC 4252 の 8 節）、製品が使う russh 0.64.1 の authenticate_password もパスワードを文字列（Into<String>）でしか受け取らない。パスフレーズも、russh の load_secret_key は Option<&str> でしか受け取らないので、使うには鍵を russh を通さずに開く新しい仕組みが要る。名前をそろえるというこの変更の求めにない新しい機能になる。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Rejected

- R1 値を UTF-8 として読めない環境変数も、バイト列のまま使う（A1 で採らなかった案）。
  - why: SSH のパスワード認証はパスワードを UTF-8 の文字列として送り（RFC 4252 の 8 節）、russh 0.64.1 の authenticate_password も load_secret_key も文字列しか受け取らない。パスフレーズで使うには鍵を russh を通さずに開く新しい仕組みが要り、名前をそろえるというこの変更の求めにない。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- 使い捨ての契約テスト（リポジトリに入れない）を crates/remote-merge/tests/contract/config_values_cli.rs に一時的に足し、`cargo nextest run -p remote-merge --all-features --test contract -E 'test(/zz_probe_non_utf8_env_values/)' --no-capture` で動かした。値には UTF-8 として読めないバイト列 "fixture\xffpass" を使った。
- 設定の password を試験サーバの正しいパスワードにし、REMOTE_MERGE_PASSWORD_DEVELOP にそのバイト列を渡すと、接続できた（status の JSON の "files" にローカルの側にだけ置いたファイルが出た）。標準エラーには "Server 'develop': using plaintext password from config. Key authentication is recommended." が出て、環境変数ではなく設定の password が使われた。
- 鍵をそのバイト列で暗号化し、REMOTE_MERGE_KEY_PASSPHRASE_DEVELOP に同じバイト列を渡して、端末のない新しいセッションで起動した。環境変数のパスフレーズで開けないときの警告 "Passphrase from REMOTE_MERGE_KEY_PASSPHRASE_DEVELOP is incorrect for ..." は出ず、"Could not read the passphrase from the terminal." の後に "Failed to load SSH private key: <HOME>/keys/encrypted" で止まった（終了コード 2）。値を使っていれば鍵を開けたはずなので、値を試さずに尋ねたことが分かる。
- 比べるため、UTF-8 として読める正しくない値 "fixture-wrong" を渡すと、"Passphrase from REMOTE_MERGE_KEY_PASSPHRASE_DEVELOP is incorrect for '<HOME>/keys/encrypted'" が出て、その値を試してから尋ねた。
- 測った後、一時的に足したテストは `git checkout` で戻した。
