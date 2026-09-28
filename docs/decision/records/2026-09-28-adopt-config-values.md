# 設定の値の検査を kotowari に取り込む際の判断

## Context

設定の読み込みと合成（[設定の読み込みと合成の取り込み](./2026-09-28-adopt-config-loading.md)）に続き、設定に書いた値をどう読み、どの値で止まるかを取り込む。
これまでサーバの値・パーミッションの書き方・走査の上限の範囲・strict_host_key_checking の値・パスワードの取り出し・sudo と agent の組み合わせは IR になく、既存テストも仕分けられていなかった。
旧資料（[旧総合仕様](../../archive/spec.md) の設定ファイルの例、パーミッション文字列のフォーマットと優先順位、SSH フォールバック時の挙動の表、利用者向けの手引き "skills/remote-merge/SKILL.md" の Full reference、既存要件）と、設定を読む全コマンドの起動時の読み込みと status・diff・merge・sync の --max-entries から見える現行実装・テストを突き合わせ、一致するものは現状を仕様として追認し、食い違い・欠落・曖昧さは FLAG として未決のまま残す。
既存要件 [REQ-merge-012](../../ir/merge/permissions.md#REQ-merge-012)、[REQ-ssh-002](../../ir/ssh/host-key.md#REQ-ssh-002)、[REQ-ssh-006](../../ir/ssh/compatibility.md#REQ-ssh-006)、[REQ-ssh-008](../../ir/ssh/sudo.md#REQ-ssh-008)、[REQ-ssh-009](../../ir/ssh/sudo.md#REQ-ssh-009)、[REQ-scan-003](../../ir/scan/limits.md#REQ-scan-003) は実装と一致したため変更しない。ssh_options の中身は ssh の話題で扱う。
要件は 7 件で limits.requirements を超えないため、1 文書にまとめる。
テストの内訳（対象 56 件）: 要件の根拠 43 件、FLAG の挙動のテスト 3 件、実装詳細をなぞるだけ 1 件、残す 9 件。

## Agreements

- A1 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料はサーバの auth を "key" か "password" とし、実装はサーバの値が次のとき "Invalid config value: servers.サーバ名.キー - 理由" のエラーで止まって終了コード 2 を返す: port が 0 のとき理由 "port must be >= 1"、auth が "key" でも "password" でもないとき理由 "invalid auth value: '値' (expected 'key' or 'password')"、root_dir が空のとき理由 "root_dir must not be empty"。port と root_dir の検査は旧資料に記述がなく、実装とテストから追認する。
  - why: 旧資料と実装の一致を確認し、旧資料にない部分は利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A2 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料はパーミッションを "0o664"・"0664"・"664" の形で受け付け、各桁を 0 から 7 とし、0o777 を超える値と setuid・setgid・sticky の桁を拒否し、不正な値は設定の読み込み時にエラーで止めるとする。実装は servers のサーバごとと [defaults] の file_permissions・dir_permissions を "0o" で始まる形か数字だけの形で 8 進数として読み、8 進数の数字以外を含むとき "invalid permissions string: '値' (must contain only octal digits 0-7)"、数字がないとき "invalid permissions string: '値' (empty octal digits)"、0o777 を超えるとき "invalid permissions value: '値' (must be <= 0o777, got 0o8進数)" を理由とし、"Invalid config value: キー名 - 理由" のエラーで止めて終了コード 2 を返す。キー名は "servers.サーバ名.file_permissions" のようにセクションを含む。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A3 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料はパーミッションをサーバの設定、[defaults]、既定値（0o664 と 0o775）の順に選ぶとし、実装は新しく作るファイルとディレクトリの権限に、サーバの file_permissions・dir_permissions があればそれを、なければ [defaults] の値を、それもなければ既定値を使う。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A4 現行実装をレビューなしで仕様とする。実装は設定の max_scan_entries が 1 から 1,000,000 の外のとき "Invalid config value: max_scan_entries - max_scan_entries must be between 1 and 1,000,000, got 値"、badge_scan_max_files が 1 から 10,000 の外のとき "Invalid config value: badge_scan_max_files - badge_scan_max_files must be between 1 and 10,000, got 値" のエラーで止まり、status・diff・merge・sync の --max-entries が 1 から 1,000,000 の外のとき "max_scan_entries must be between 1 and 1,000,000, got 値" のエラーで止まり、いずれも終了コード 2 を返す。
  - why: 旧資料に記述はないが利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A5 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は strict_host_key_checking を "yes"・"no"・"ask" とし、実装は大文字と小文字を区別せずに "ask" を ask、"yes" と "true" を yes、"no" と "false" を no として読み、それ以外の値は "Unknown strict_host_key_checking value: '値', falling back to 'ask'" の警告を出して ask として扱う。
  - why: 旧資料と実装の一致を確認し、旧資料にない値の扱いは利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A6 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料はパスワードを環境変数 REMOTE_MERGE_PASSWORD_<サーバ名> で渡すとし、実装は auth が "password" のサーバのパスワードに、サーバ名を大文字にした環境変数 REMOTE_MERGE_PASSWORD_<サーバ名> が空でなく設定されていればそれを設定の password より優先して使い、空の環境変数は設定されていないものとして扱う。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A7 旧資料と現行実装が一致するため、レビューなしで仕様とする。利用者向けの手引き "skills/remote-merge/SKILL.md" は key の既定値を "~/.ssh/id_rsa" とし、実装は auth が "key" のサーバで key を省くと "~/.ssh/id_rsa" を使い、key が "~/" で始まるときその部分を利用者のホームディレクトリに置き換える。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A8 未決の FLAG として残す。[旧総合仕様](../../archive/spec.md) のパーミッション文字列のフォーマットは 3 桁の形（"0o664"・"0664"・"664"）だけを挙げるが、実装は "64" や "7" のような短い数字の並びも 8 進数として受け付けて、0o777 以下なら権限として使う。3 桁でない形を拒むべきかが旧資料から読み取れない。
  - why: 読んだ後も旧資料と実装が一致するか食い違うかの確信が持てず、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A9 未決の FLAG として残す。実装は auth が "key" のサーバに password が書かれているとき "servers.サーバ名: password is set but auth is 'key' — password will be ignored" の警告を出し、その password を認証に使わない。旧資料に記述がなく、警告を確かめるテストもない。
  - why: 利用者に見える挙動に仕様とテストが欠けており、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A10 未決の FLAG として残す。[旧総合仕様](../../archive/spec.md) の設定ファイルの例はパスワードを設定ファイルに書かず、接続時のプロンプトか環境変数で渡すとするが、実装は設定の password を平文のまま受け付けて使うときに "Server 'サーバ名': using plaintext password from config. Key authentication is recommended." の警告を出すだけで、接続時にパスワードを尋ねることはなく、環境変数にも設定にもパスワードがなければ "SSH authentication failed (user: ユーザ名@ホスト)" の認証エラーにする。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A11 未決の FLAG として残す。[旧総合仕様](../../archive/spec.md) の SSH フォールバック時の挙動の節は sudo = true と agent.enabled = false の組み合わせを設定の読み込み時（サーバへの接続前）に検出してエラーで止め、Agent の有効化と NOPASSWD の設定を案内するとするが、実装は設定の読み込みでは止めず、そのサーバに接続するときに "sudo = true requires agent to be enabled. Set [agent] enabled = true in your config." で止め、NOPASSWD には触れない。そのため、そのサーバに接続しない操作は止まらない。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
