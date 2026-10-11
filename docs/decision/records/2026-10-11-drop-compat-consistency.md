# 古い版との互換の分岐を捨てた後の、IR とコードの突き合わせで決めたこと

## Context

[2026-10-11-drop-compat.md](./2026-10-11-drop-compat.md) の A1〜A6 を実装した後、変えたコードを IR と読み比べた。
知らないキーをエラーにしたことで、製品がどのキーを知っているかが利用者から見えるようになったが、IR はその一覧を持っていなかった。
また、書き込み先の識別の字面の正規化が、リモートの相対の root_dir の先頭の "." を除いていなかった。
ここでは、試しの実行で確かめた事実をもとに、IR とコードのどちらをどう合わせたかを決める。

Position: A1・A2 で出揃った。

## Agreements

- A1 [REQ-config-039](../../ir/config/loading.md#REQ-config-039) の「製品の知らないキーかセクション」は、[TBL-config-004](../../ir/config/loading.md#TBL-config-004) に並べたセクションとキーのどれでもないものとする。表は今の実装の読み込みの構造体（`crates/remote-merge-config/src/lib.rs` の `RawConfig` から `RawAgentConfig` までの九つ）が受け付けるものをそのまま並べ、REQ-config-039 に `- definition:` として結びつけた。コードは変えない。
  - why: drop-compat の実装で知らないキーがエラーになり、どのキーを書けるかが利用者から見える振る舞いになった。IR は [TBL-config-001](../../ir/config/defaults.md#TBL-config-001) で既定値のあるキーを挙げるだけで、host・user・[servers.名前.ssh_options] とその四つのキーはどの要件にも名前がなく、REQ-config-039 を字面で読むとこれらもエラーになる。既存の IR が名前を挙げるキーはすべて実装が受け付けるので、一覧を足すのは IR と矛盾しない具体化である。試し: 表のすべてのセクションとキー（パスワード認証のサーバを含む）を書いたプロジェクト設定で `remote-merge status --left local --right develop` を動かすと、設定の読み込みを通って "SSH connection failed to 127.0.0.1: Connection refused" で止まった。同じ設定の mac_algorithms を "macs" に書き換えると "Failed to parse config file: ... unknown field `macs`, expected one of `kex_algorithms`, `host_key_algorithms`, `ciphers`, `mac_algorithms`" で止まった（どちらも終了コード 2）。
  - decided_by: 一貫性の段（consistency phase）
- A2 書き込み先の識別の字面の正規化（[REQ-backup-021](../../ir/backup/sessions.md#REQ-backup-021)）は、先頭のものを含めてすべての "." の要素を除く。"." の要素だけからなる root_dir は "." を識別に使う。コードを直し、IR は変えない。
  - why: REQ-backup-021 は "." の要素を除くと書いているが、実装が使う Rust の `Path::components` は相対パスの先頭の "." を残す。試しのプログラムで "./app" は "./app"、"app" は "app" になり、リモートの root_dir を "./srv/app" と書いた設定で保存したセッションが "srv/app" に書き直した設定の rollback --list に出なかった（契約テスト `a_session_stays_listed_after_the_leading_dot_of_a_relative_remote_root_dir_is_removed` が直す前に 0 件で失敗した）。リモートの root_dir は空でなければ相対でも受け付ける（[TBL-config-002](../../ir/config/values.md#TBL-config-002)）ので、利用者が書ける設定である。ローカルの相対の root_dir は絶対パスにつないでから正規化するので、この違いはリモートにだけ出る。"." だけの root_dir を空にせず "." に残すのは、IR が書いていない内部のキーの選び方で、この版より前に "." と書いて保存したセッションのキーを変えないためである（"." と "./" はどちらも "." になり、同じ書き込み先のまま）。
  - decided_by: 一貫性の段（consistency phase）
