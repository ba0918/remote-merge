# 走査と子の一覧の経路差をそろえた単位で、exclude の下の失敗で SSH の走査が止まる場合の指し先を広げる

## Context

走査と子の一覧の exclude と symlink の扱いを三つの経路でそろえる単位（計画 scan-route-parity）で、[REQ-config-021](../../ir/config/filters.md#REQ-config-021) と [REQ-config-022](../../ir/config/filters.md#REQ-config-022) は、どの経路の走査も除外したディレクトリの下の循環や読めないディレクトリでは止めないとし、SSH の経路の例外を「古い find は FLAG-config-016」とだけ指していた。
一方、[FLAG-config-016](../../ir/config/FLAGS.md) の本文は、診断のパスの逃がしを戻したバイトが UTF-8 として読めないときも SSH の経路だけが走査を止めると書いている。
要件の文だけを読むと、SSH の経路の例外は古い find だけに読め、FLAG の本文と食い違っていた。

Position: A1 を反映した。

## Agreements

- A1 REQ-config-021 と REQ-config-022 の例外の指し先を「SSH の経路で走査を止める場合は FLAG-config-016」に改め、FLAG-config-016 の見出しを「古い find のサーバや UTF-8 でない名前で exclude の下の失敗が走査を止める」にする。コードは変えない。
  - why: GNU find 4.9.0 で、一般の利用者（uid 1000）として "r/a/b/x\377"（0xff を含む名前）を mode 000 にし、`LC_ALL=C /usr/bin/find -L <dir>/r -mindepth 1 ! -xtype l -printf ''` を動かすと、`'<dir>/r/a/b/x\377': Permission denied` と終了コード 1 が出た。`crates/remote-merge-ssh/src/find_diagnostics.rs` の `quoted` は、逃がしを戻したバイトが UTF-8 として読めなければ None を返し、その行を見分けないので、exclude "a/?" でも SSH の走査は失敗する（同じファイルの単体テスト `paths_that_are_not_utf8_or_unknown_escapes_are_not_accepted` が `\377` で確かめている）。ローカルとエージェントの経路は除外したディレクトリの下へ降りないので、この場合も一覧を返す。SSH の経路の find の標準出力は `client.rs` で `from_utf8_lossy` に戻すので、UTF-8 でない名前があるだけでは走査は止まらず、この差は exclude の下の失敗に限られる。これを見分けずに止めるのは [consistency-3 A3](./2026-10-11-scan-route-parity-consistency-3.md#A3) の決定で、FLAG-config-016 の本文もそれを書いているので、要件の指し先と FLAG の見出しを本文に合わせるだけにする。
  - decided_by: 整合の段（consistency phase）
