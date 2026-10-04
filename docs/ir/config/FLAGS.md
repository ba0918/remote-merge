# Flags

## Flags

### FLAG-config-001: グローバル設定の場所
- kind: contradiction
- related: REQ-config-008
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A12

旧総合仕様・README・利用者向けの手引きはグローバル設定の場所を "~/.config/remote-merge/config.toml" とするが、実装は OS の設定ディレクトリの下の "remote-merge/config.toml" を読むため、Linux では XDG_CONFIG_HOME があればその下になり、配布している macOS では "~/Library/Application Support/remote-merge/config.toml" になる。

### FLAG-config-002: [local] の root_dir の既定値
- kind: contradiction
- related: REQ-config-013
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A13

利用者向けの手引きは [local] の root_dir の既定値を "." とするが、実装は [local] がグローバル設定にもプロジェクト設定にもないとき "Invalid config value: local - [local] section is required" のエラーで止め、root_dir のない [local] は TOML の読み込みエラーにする（後者は実装を読んで分かったことで未実行）。

### FLAG-config-003: 走査の上限のキーの置き場所と既定値
- kind: contradiction
- related: REQ-config-013, REQ-scan-003
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A14

旧総合仕様は [scan] セクションの max_scan_entries（既定 100,000）と badge_scan_max_files（既定 5,000）とするが、利用者向けの手引きと実装はトップレベルのキーで既定 50,000 と 500 とし、実装はプロジェクト設定・グローバル設定・既定値の順にキーごとに選び、[scan] セクションに書いた値は知らせずに無視する（無視は実装を読んで分かったことで未実行）。

### FLAG-config-004: 相対パスの root_dir の起点
- kind: gap
- related: REQ-config-006, REQ-config-010
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A15

[local] の root_dir が相対パスのとき、実装は設定ファイルの場所ではなく実行時のカレントディレクトリから解決するため、--config で別のディレクトリの設定を指定したりサブディレクトリから実行したりすると指す場所が変わる。rollback --list で root が相対パスのまま示されることは確かめたが、"Path not found" になる事象そのものは再現していない。旧資料に記述がなくテストもない。

### FLAG-config-005: ローカルの root_dir がないときの知らせ方
- kind: gap
- related: REQ-config-010
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A16

[local] の root_dir が存在しないとき、実装はローカルの走査で "Path not found: パス" のエラーだけを出し、root_dir の設定を見直すよう促さない。旧総合仕様はリモートの root_dir だけを扱い、ローカルについては記述がなくテストもない。

### FLAG-config-006: リモートの root_dir がないときの知らせ方
- kind: ambiguity
- related: REQ-config-008
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A17

旧総合仕様はリモートの root_dir が存在しないかアクセス権がないとき接続時にエラーを表示して設定の確認を促すとするが、実装で確かめられたのは SSH の再帰走査が接続後の走査の時点で "Remote root_dir not found: サーバ名:パス" を返すところまでで、エージェントを使う経路と TUI の表示は読み切れていない。

### FLAG-config-007: 知らないキーを黙って無視する
- kind: gap
- related: REQ-config-009
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A18

実装は設定ファイルの知らないキーやセクションを知らせずに無視するため、キー名の書き間違いに利用者が気づけない。旧資料に記述がなくテストもない。

### FLAG-config-008: --config を使わないサブコマンドの警告
- kind: gap
- related: REQ-config-006
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A19

実装は init・logs・events に --config を指定すると "Warning: --config is ignored for the 'サブコマンド名' subcommand" を出して設定を読まずに続ける。旧資料に記述がなくテストもない。

### FLAG-config-010: auth が key のサーバの password の警告
- kind: gap
- related: REQ-config-014
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A9

実装は auth が "key" のサーバに password が書かれているとき "servers.サーバ名: password is set but auth is 'key' — password will be ignored" の警告を出し、その password を認証に使わない。旧資料に記述がなく、警告を確かめるテストもない。

### FLAG-config-011: 設定ファイルの平文のパスワード
- kind: contradiction
- related: REQ-config-019
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A10

旧総合仕様はパスワードを設定ファイルに書かず接続時のプロンプトか環境変数で渡すとするが、実装は設定の password を平文のまま受け付けて使うときに "Server 'サーバ名': using plaintext password from config. Key authentication is recommended." の警告を出すだけで、接続時にパスワードを尋ねることはなく、環境変数にも設定にもパスワードがなければ "SSH authentication failed (user: ユーザ名@ホスト)" の認証エラーにする。

### FLAG-config-012: sudo と agent の組み合わせを検出する時点
- kind: contradiction
- related: REQ-ssh-009
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A11

旧総合仕様は sudo = true と agent.enabled = false の組み合わせを設定の読み込み時（サーバへの接続前）に検出してエラーで止め、Agent の有効化と NOPASSWD の設定を案内するとするが、実装は設定の読み込みでは止めず、そのサーバに接続するときに "sudo = true requires agent to be enabled. Set [agent] enabled = true in your config." で止め、NOPASSWD には触れない。そのため、そのサーバに接続しない操作は止まらない。

### FLAG-config-013: 全て無効な include
- kind: contradiction
- related: REQ-config-004, REQ-config-024
- source: docs/decision/records/2026-09-29-adopt-config-filters.md#A7

旧総合仕様の設定ファイルの例は include を指定したとき一致するパスだけを走査するとするが、実装は include の値が全て無効（絶対パス、".." を含む、glob 文字を含む）なとき警告を出して include を空として扱うため、全てのパスが走査の対象になる（実装を読んで分かったことで未実行）。

### FLAG-config-015: "../" を含む exclude のパターンの警告
- kind: gap
- related: REQ-config-022
- source: docs/decision/records/2026-09-29-adopt-config-filters.md#A9

実装は "../" を含む exclude のパターンを無視し、パスを一つ調べるたびに "Skipping suspicious exclude pattern containing '../': パターン" の警告を出す。旧資料に記述がなく、警告を確かめるテストもない。
