# サーバごとの環境変数の名前と仕様の突き合わせ

## Context

パスワードとパスフレーズを渡す環境変数の名前を一つの関数（crates/remote-merge-ssh/src/server_env.rs の server_env_name）で作り、どちらもサーバ名を大文字にして "-" と "." を "_" にするようにした変更（1e134ed5）について、変わったコードと仕様を突き合わせた。
[REQ-config-019](../../ir/config/values.md#REQ-config-019) は、パスワードの環境変数の名前をサーバ名を大文字にしただけのものとしていて、変更後のコードと食い違っていた。パスフレーズの環境変数は IR のどこにもなかった。
この記録は、IR をどちらに合わせたか、IR に何を足し何を足さなかったかと、その根拠を残す。

Position: REQ-config-019 の名前の作り方を変更後のコードに合わせ、[2026-09-28-adopt-config-values.md の A6](./2026-09-28-adopt-config-values.md#A6) の名前の作り方の部分をこの記録の A1 で置き換えた（A1）。パスフレーズの環境変数を [REQ-config-050](../../ir/config/values.md#REQ-config-050) として足し（A2）、尋ねる回数と文言は足さなかった（A3）。どちらの名前も "-" と "." を含むサーバ名で確かめる契約テストを足した（A4）。

## Agreements

- A1 auth が "password" のサーバのパスワードを渡す環境変数 REMOTE_MERGE_PASSWORD_<サーバ名> の <サーバ名> は、サーバ名を大文字にし、"-" と "." を "_" にしたものとする（サーバ "web-1.prod" なら REMOTE_MERGE_PASSWORD_WEB_1_PROD）。これを [REQ-config-019](../../ir/config/values.md#REQ-config-019) に書き、[2026-09-28-adopt-config-values.md の A6](./2026-09-28-adopt-config-values.md#A6) のうち名前の作り方（サーバ名を大文字にするだけ）を置き換える。空でない環境変数を設定の password より優先することと、空の環境変数を設定されていないものとして扱うことは A6 のまま変えない。コードは変えない。
  - why: 2026-10-23 に利用者は、パスワードとパスフレーズの環境変数の名前を、どちらも「大文字にして - と . を _ にする」にそろえると決めた。いまは利用者がいないので、名前を変えても困る人はいないことを理由に挙げた。A6 は旧資料と実装の一致を確かめた現状追認で、"-" や "." を残す理由を述べていない。シェルの変数名に使えるのは英字・数字・"_" だけなので、"-" や "." を残した名前はシェルの export では設定できない（sh で `export REMOTE_MERGE_PASSWORD_WEB-1.PROD=x` は "bad variable name" で失敗する）。変更後のコードは、crates/remote-merge-ssh/src/password.rs の resolve_password_with（21 行）で名前を server_env_name から作り、この決定のとおりに読む。古かったのは IR だけである。
  - decided_by: 利用者（2026-10-23、推奨を採った）。IR への書き方は consistency phase（実装と仕様の整合確認役）
- A2 auth が "key" のサーバの鍵がパスフレーズで保護されているときに読む環境変数 REMOTE_MERGE_KEY_PASSPHRASE_<サーバ名> を、[REQ-config-050](../../ir/config/values.md#REQ-config-050) として IR に足す。<サーバ名> は A1 と同じ作り方とする。環境変数が空でなく設定されていれば、パスフレーズを尋ねる前にその値で鍵を開き、開けたときは尋ねない。環境変数が空か、ないか、その値で開けないときは端末で尋ね、開けるパスフレーズを得られないときは [REQ-config-020](../../ir/config/values.md#REQ-config-020) の鍵ファイルを読めないときと同じ "Failed to load SSH private key: パス" のエラーで止まる。コードは変えない。
  - why: この環境変数は、配布する手引き "skills/remote-merge/SKILL.md" の Environment Variables の節が、端末で尋ねられない CI のような場所で鍵のパスフレーズを渡す手段として利用者に案内している。名前が変われば利用者が設定した値が読まれなくなるので、IR が持つ「変われば利用者に違いが見える」振る舞いに当たる。それでも IR のどこにもなく、変更前の名前（"-" だけを "_" にし、"." を残す）は手引きの「- と . を _ にする」と食い違っていた。2026-10-23 に利用者も、パスフレーズの規則を IR に足すことを求めた。文は変更後のコードから読んだ。crates/remote-merge-ssh/src/client.rs の load_secret_key_with（684-769 行）は、まずパスフレーズなしで鍵を開き、暗号化された鍵なら passphrase_env_key（crates/remote-merge-ssh/src/passphrase_provider.rs。名前は server_env_name で作る）の名前の環境変数を、空でなければ試す。開けなければ PassphraseProvider に尋ね、得られなければ鍵を読めない誤り（"Failed to load SSH private key: パス"）を返す。製品が渡す PassphraseProvider（crates/remote-merge/src/ssh/passphrase_provider.rs の CliPassphraseProvider）は端末から尋ねる。空の環境変数を試さないことは REQ-config-019 のパスワードと同じ扱いで、試しても空のパスフレーズでは暗号化された鍵を開けないので、尋ねる流れは変わらない。
  - decided_by: consistency phase（実装と仕様の整合確認役）。IR に足すことは利用者が求めた
- A3 パスフレーズを尋ねる回数（いまは 3 回）、尋ねるときの文言（いまは "Enter passphrase for key '<パス>': "）と、環境変数のパスフレーズや尋ねて得たパスフレーズで開けなかったとき・端末から読めなかったときの警告の文言は、[REQ-config-050](../../ir/config/values.md#REQ-config-050) に書かない。
  - why: この変更はこれらに触れておらず、利用者の決定の対象でもない。REQ-config-050 は端末で尋ねることと、得られないときに止まることまでを述べ、回数と文言を決めないので、いまのコードと矛盾しない。決めるときは、尋ねる流れを扱う変更で利用者に諮る。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A4 "-" と "." を含むサーバ名 "web-1.prod" で、[REQ-config-019](../../ir/config/values.md#REQ-config-019) と [REQ-config-050](../../ir/config/values.md#REQ-config-050) の環境変数の名前を実行ファイルを通して確かめる契約テストを、crates/remote-merge/tests/contract/config_values_cli.rs に足す。どちらも、A1 の名前が読まれることと、名前だけを変えた古い作り方の名前が読まれないことを一つのテストで比べる。パスフレーズのテストは、起動するプロセスを libc の setsid で新しいセッションにして端末を持たせない。
  - why: これまでの REQ-config-019 の契約テストはサーバ名 develop だけを使い、大文字にするだけの名前と A1 の名前がどちらも REMOTE_MERGE_PASSWORD_DEVELOP になるので、A1 を確かめられなかった。ssh の crate の単体テストは名前を作る関数を確かめるが、このプロジェクトで kotowari の印を読むのは契約テストだけである。製品はパスフレーズを端末（rpassword 7.5.4 の既定の入出力は "/dev/tty"）から尋ねるので、端末のある場所でテストを動かすと、名前を読み違えたときに入力を待って止まる。新しいセッションには端末がなく、尋ねても答えを得られないことが決まるので、読み違えたときは鍵を読めないエラーで落ちる。試験サーバは鍵による認証を受け付けないので、正しい名前のときに確かめられるのは、鍵を開けて鍵を読めないエラーにならないことまでである。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- 変更前の名前での測定（リポジトリに入れない）: crates/remote-merge-ssh/src/server_env.rs の server_env_name をサーバ名を大文字にするだけの形（A6 の名前）に一時的に戻し、`cargo nextest run -p remote-merge --all-features --test contract -E 'test(/config_values_cli::password_env_var_name/)'` を動かした。REMOTE_MERGE_PASSWORD_WEB_1_PROD を渡しても読まれず、設定の正しくない password で `{"error":"SSH authentication failed (user: fixture-user@127.0.0.1)"}` になって失敗した。
- crates/remote-merge-ssh/src/passphrase_provider.rs の passphrase_env_key を変更前の形（"-" だけを "_" にして大文字にする）に一時的に戻し、`cargo nextest run -p remote-merge --all-features --test contract -E 'test(/config_values_cli::key_passphrase/)'` を動かした。REMOTE_MERGE_KEY_PASSPHRASE_WEB_1_PROD は読まれず、"Could not read the passphrase from the terminal." の警告の後に "Failed to load SSH private key: <HOME>/keys/encrypted" で止まって失敗した。
- どちらも戻した後（59b4e6c7）は成功する。正しい名前のパスフレーズのときの出力は "SSH authentication failed (user: fixture-user@127.0.0.1)" で、鍵を開けた後の鍵による認証で止まっていることを確かめた。
- シェルの変数名: `sh -c 'export REMOTE_MERGE_PASSWORD_WEB-1.PROD=x'` は "export: REMOTE_MERGE_PASSWORD_WEB-1.PROD: bad variable name"、bash では "not a valid identifier" で失敗する。
