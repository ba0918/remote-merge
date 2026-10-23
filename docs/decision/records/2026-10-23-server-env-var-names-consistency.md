# サーバごとの環境変数の名前と仕様の突き合わせ

## Context

パスワードとパスフレーズを渡す環境変数の名前を一つの関数（crates/remote-merge-ssh/src/server_env.rs の server_env_name）で作り、どちらもサーバ名を大文字にして "-" と "." を "_" にするようにした変更（1e134ed5）について、変わったコードと仕様を突き合わせた。
[REQ-config-019](../../ir/config/values.md#REQ-config-019) は、パスワードの環境変数の名前をサーバ名を大文字にしただけのものとしていて、変更後のコードと食い違っていた。
この記録は、IR をどちらに合わせたかと、その根拠を残す。

Position: REQ-config-019 の名前の作り方を変更後のコードに合わせ、[2026-09-28-adopt-config-values.md の A6](./2026-09-28-adopt-config-values.md#A6) の名前の作り方の部分をこの記録の A1 で置き換えた（A1）。

## Agreements

- A1 auth が "password" のサーバのパスワードを渡す環境変数 REMOTE_MERGE_PASSWORD_<サーバ名> の <サーバ名> は、サーバ名を大文字にし、"-" と "." を "_" にしたものとする（サーバ "web-1.prod" なら REMOTE_MERGE_PASSWORD_WEB_1_PROD）。これを [REQ-config-019](../../ir/config/values.md#REQ-config-019) に書き、[2026-09-28-adopt-config-values.md の A6](./2026-09-28-adopt-config-values.md#A6) のうち名前の作り方（サーバ名を大文字にするだけ）を置き換える。空でない環境変数を設定の password より優先することと、空の環境変数を設定されていないものとして扱うことは A6 のまま変えない。コードは変えない。
  - why: 2026-10-23 に利用者は、パスワードとパスフレーズの環境変数の名前を、どちらも「大文字にして - と . を _ にする」にそろえると決めた。いまは利用者がいないので、名前を変えても困る人はいないことを理由に挙げた。A6 は旧資料と実装の一致を確かめた現状追認で、"-" や "." を残す理由を述べていない。シェルの変数名に使えるのは英字・数字・"_" だけなので、"-" や "." を残した名前はシェルの export では設定できない（sh で `export REMOTE_MERGE_PASSWORD_WEB-1.PROD=x` は "bad variable name" で失敗する）。変更後のコードは、crates/remote-merge-ssh/src/password.rs の resolve_password_with（21 行）で名前を server_env_name から作り、この決定のとおりに読む。古かったのは IR だけである。
  - decided_by: 利用者（2026-10-23、推奨を採った）。IR への書き方は consistency phase（実装と仕様の整合確認役）
