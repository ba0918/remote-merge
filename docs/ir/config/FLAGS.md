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

### FLAG-config-016: 古い find のサーバで exclude の下の失敗が走査を止める
- kind: gap
- related: REQ-config-021, REQ-config-022
- source: docs/decision/records/2026-10-11-scan-route-parity-consistency-3.md#A2

SSH の経路の走査は、find のパターンで同じ意味に書けない exclude のパターン（"a/?"・"**/b"・"[b]" のように "?"・"["・"**" を含むもの）に当たるディレクトリの下へ降り、その下の循環する symlink や読めないディレクトリについて find が出す診断が GNU findutils の C ロケールの文言（4.5.11・4.9.0・4.10.0 で確かめた形）のときだけ、走査を止めない。findutils 4.4.2 はパスの引用を "`" で始め、4.2.27 はパスを引用せず循環を別の文言で出す（どちらもコンテナで確かめた）ため、こうした find のサーバでは SSH の経路だけが走査をエラーにする。診断のパスの逃がしを戻したバイトが UTF-8 として読めないときも同じになる。刈り込めるパターン（文字と単独の "*" だけの名前のパターンと、末尾の "/**" を除いて glob の文字を含まないパスのパターン）は、4.2.27 と 4.4.2 でも find がその下へ降りないので、この差は起きない。
