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

### FLAG-config-005: ローカルの root_dir がないときの知らせ方
- kind: gap
- related: REQ-config-010
- source: docs/decision/records/2026-09-28-adopt-config-loading.md#A16

[local] の root_dir が存在しないとき、実装はローカルの走査で "Path not found: パス" のエラーだけを出し、root_dir の設定を見直すよう促さない。旧総合仕様はリモートの root_dir だけを扱い、ローカルについては記述がなくテストもない。

### FLAG-config-011: 設定ファイルの平文のパスワード
- kind: contradiction
- related: REQ-config-019
- source: docs/decision/records/2026-09-28-adopt-config-values.md#A10

旧総合仕様はパスワードを設定ファイルに書かず接続時のプロンプトか環境変数で渡すとするが、実装は設定の password を平文のまま受け付けて使うときに "Server 'サーバ名': using plaintext password from config. Key authentication is recommended." の警告を出すだけで、接続時にパスワードを尋ねることはなく、環境変数にも設定にもパスワードがなければ "SSH authentication failed (user: ユーザ名@ホスト)" の認証エラーにする。
