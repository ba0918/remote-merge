# Unix 以外に対応しないことと仕様の突き合わせ

## Context

利用者は 2026-10-23 に、Windows に対応する予定がないなら確かめようのない Unix 以外の分岐ごと消す、という推奨を採った。
この決定を受けた変更（d4f1678a・f8802f44・f9041da6）は、8 ファイルにあった Unix 以外の代わりの道を消し、Unix 以外のターゲットでは remote-merge-core のコンパイルを compile_error! で止め、Windows 向けのインストールスクリプトと梱包を消し、README で WSL を案内した。
一方で [REQ-paths-006](../../ir/paths/resolution.md#REQ-paths-006) は、出典の [2026-10-06-path-resolution.md の A54](./2026-10-06-path-resolution.md#A54) に従って Unix 以外の環境での検索権限の扱いを述べ、消えたコードの振る舞いを書いたままだった。
この記録は、IR をどちらに合わせたかと、対応するプラットフォームを IR に書かない理由を残す。

Position: REQ-paths-006 から Unix 以外の環境の一文と A54 の出典を外し、A54 をこの記録の A1 で置き換えた（A1）。対応するプラットフォームは IR に書かない（A2）。コードは変えない。

## Agreements

- A1 remote-merge は Unix（Linux と macOS）でだけビルドして動かし、Unix 以外の環境の代わりの道を持たない。[REQ-paths-006](../../ir/paths/resolution.md#REQ-paths-006) から「Unix 以外の環境では、OS が権限のエラーを返したときだけこの理由で止める。」の一文を外し、出典から [2026-10-06-path-resolution.md の A54](./2026-10-06-path-resolution.md#A54) を外して、A54 をこの決定で置き換える。コードは変えない。
  - why: 根拠は三つある。一つ目に、利用者が 2026-10-23 に、Windows を続ける案ではなく、予定がないなら確かめようのない分岐ごと消す推奨を採った（「予定がないなら：確かめようのない分岐ごと消す」という推奨に「推奨で」と答えた）。二つ目に、A54 が求めた「Windows 向けのターゲットで cargo check が通ること」は、この変更の前から満たされていなかった。変更前（b8dde42a）は Windows 向けのターゲットで remote-merge-core のコンパイルが crates/remote-merge-core/src/scan/machine_fs.rs の 120 行（Unix 以外の mtime_of が呼ぶ、存在しない `crate::listing`）で失敗し（下の Verification の測定）、Unix 以外の分岐はどの CI でもコンパイルされていなかった。REQ-paths-006 の一文が述べる環境では製品をそもそも作れず、一文は実現されていなかった。三つ目に、A54 の理由の前提だった「README は Windows のインストールを案内しており」は、変更の前から成り立っていなかった。変更前の README は「Windows はまだ配布していません（バイナリの配布は Linux と macOS だけです。`scripts/install.ps1` は Windows 対応のときに使います）」と書き、Windows のインストールを案内せず、将来の対応を予告していただけだった。この変更が README を「Windows には対応していません」に書き換えたのは一つ目の決定に合わせたもので、置き換えの根拠には数えない。変更後は同じ確かめが最初に "remote-merge builds only on Unix (Linux and macOS)" で止まり、Unix 以外の環境で動く製品がないので、IR に Unix 以外の振る舞いを書く対象がない。Unix の上の振る舞いは変わらない。消したのは cfg(not(unix)) の付いた項目・ブロック・文・match の腕で、二つの道を選ぶだけだった cfg(unix) のブロックと文は印だけを外して中身を残したので、Unix でコンパイルされる中身は同じである（cfg(unix) の印を外したのは crates/remote-merge-engine/src/backup_store.rs の create_dir_owner_only、crates/remote-merge-core/src/scan/machine_fs.rs の why_unreached、crates/remote-merge/src/main.rs の agent の分岐で、crates/remote-merge-core/src/path_resolution/os.rs の entry_kind では cfg(not(unix)) の match の腕だけを消した）。REQ-paths-006 の残りの文（実効 uid で検索する権限を持たなければ "cannot resolve path: permission denied" で止める）は Unix の can_search のとおりで変えない。
  - rejected: Windows を続ける（Unix 以外の分岐のコンパイルの誤りを直し、CI に Windows 向けのターゲットの cargo check を足す）。利用者が採らなかった。
  - decided_by: consistency phase（実装と仕様の整合確認役）。Unix 以外に対応しないことは、利用者の 2026-10-23 の決定（推奨を採った）による
- A2 remote-merge がどのプラットフォームに対応するか（Windows に対応しないこと、Unix 以外のターゲットでコンパイルを止めること）は IR に書かない。利用者には README のインストールの節と CHANGELOG、scripts/install.sh の "unsupported OS: ... (use WSL on Windows)" で示し、開発者には crates/remote-merge-core/src/lib.rs の compile_error! とその上の注釈で示す。
  - why: IR が持つのは、製品を入れて使う人がコマンド・設定・出力・配布物を通して見る振る舞いである。対応するプラットフォームを決めているのはビルドの構成（cfg(unix) と compile_error!）とリリースの手順（.github/workflows/release.yml の配布のターゲット）で、どちらも IR の外に置くものとされている。この変更で製品を使える人に見える違いはない。Linux と macOS の振る舞いは変わらず（A1）、Windows では変更の前も後も製品をビルドできず（A1 の測定）、Windows 向けの配布物は v0.1.0（2026-03-12）に x86_64-pc-windows-msvc の zip を出したのを最後に、v0.2.0 から変更前の最新の v0.3.3 までの 18 の版では出していない（下の Verification の測定）。変更前の README も「Windows はまだ配布していません」と書いていた。IR には Linux と macOS の上の振る舞い（[REQ-config-045](../../ir/config/loading.md#REQ-config-045) の設定の場所、[REQ-paths-008](../../ir/paths/routes.md#REQ-paths-008) の macOS の PATH_MAX）を述べる項目はあるが、どのプラットフォームに対応するかを決める項目はない。[REQ-ssh-005](../../ir/ssh/fallback.md#REQ-ssh-005) の Linux の x86_64 と macOS は、エージェントを置けるサーバと置けずに止めるサーバを分ける実行時の振る舞いで、この変更は release.yml のその三つのターゲット（x86_64-unknown-linux-musl・x86_64-apple-darwin・aarch64-apple-darwin）を変えていない。IR に入れてよいか迷うものは IR に入れないという既定に従い、この分け方は利用者が次に作業を見るときに示す。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- 変更前の測定（リポジトリに入れない）: `git archive b8dde42a` で取り出した木で `cargo check --target x86_64-pc-windows-gnu -p remote-merge-core` を動かすと、`error[E0433]: cannot find listing in the crate root` が crates/remote-merge-core/src/scan/machine_fs.rs:120:18 で出て失敗した。
- 変更後の測定（f9041da6、リポジトリに入れない）: 同じコマンドの最初の誤りは `error: remote-merge builds only on Unix (Linux and macOS)` で、続いて消した Unix 以外の項目がないことによる誤り（`error[E0046]: not all trait items implemented, missing: can_search, read_link` など）が出て失敗した。
- どちらの測定も作業ツリーの外の target ディレクトリ（CARGO_TARGET_DIR）で行った。
- 配布物の測定（リポジトリに入れない）: `gh release list` に出る v0.1.0 から v0.3.3 までの 19 の版それぞれで `gh release view <版> --json assets` を動かすと、Windows 向けの配布物（remote-merge-x86_64-pc-windows-msvc.zip）は v0.1.0 にだけあり、v0.2.0 以降の版はどれも Linux と macOS 向けの tar.gz と SHA256SUMS.txt だけだった。
- 突き合わせの後、Linux で `cargo nextest run --workspace --all-features` の 2143 件がすべて通った。これは Unix の上で壊れていないことの確かめで、A1 の根拠にはしていない。

## Revisions

- 2026-10-23: 整合の段の二度目の確かめで、A2 の why の「Windows 向けの配布物は一度も出していない」が、v0.1.0 の配布物に Windows 向けの zip があることと食い違うと分かったので、測った事実（v0.1.0 を最後に出していない）に直し、Verification に配布物の測定を足した。A2 の結論は、対応するプラットフォームをビルドの構成とリリースの手順が決めていることと、それを決める IR の項目がないことに拠っていて、変わらない。あわせて A1 の why の、消したもの（cfg(not(unix)) の付いたもの）と印だけを外したもの（cfg(unix) のブロックと文）の書き分けを、差分のとおりに直した。
- 2026-10-23: A1 の why の三つ目の根拠が、この変更自身が書き換えた後の README を挙げていて、置き換えの根拠として循環していたので、変更前の README がすでに Windows のインストールを案内していなかったという事実に直した。A1 の結論は一つ目と二つ目の根拠に拠っていて、変わらない。
