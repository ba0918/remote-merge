# リモートの道具の違い

リモートのファイルの操作はエージェントだけで行う（[土台の作り直し A12](decision/records/2026-10-12-foundation.md#A12)）。SSH でリモートのシェルと道具を使うのは、エージェントを置いて起動するときだけである。この文書は、そのために確かめた事実だけを並べる。どう書くかは決めない（[土台の作り直し A8](decision/records/2026-10-12-foundation.md#A8)）。

## 対象にするリモート

- ログインシェル: Bourne 系（sh・bash・dash・BusyBox ash）と fish。コマンドは `sh -c` に一つの引数で渡す（[REQ-ssh-020](ir/ssh/remote-shell.md#REQ-ssh-020)）。
- エージェントの配布: Linux の x86_64（musl）と macOS。本番の CentOS 5（2.6.18-419.el5、x86_64）で、リリース済みの版のエージェントが配置・起動・走査できることを利用者が確かめた（2026-10-13）。musl のエージェントが 2.6.18-419.el5 のカーネルで手元と同じ答えを返すことを qemu で確かめた（2026-10-13。このときの通信の版は 15 より前）。版 15 で足したリンク先を辿れない symlink と名前を読めない symlink（[REQ-scan-018](ir/scan/directory-links.md#REQ-scan-018)）は、`scripts/kernel-2.6.18/fixtures.sh` の mk2 を一般の利用者として問う同じ確かめで、tmpfs と ext3 の両方で手元と同じ答えを返した（2026-10-23）。root として問う回の答えは、手元を一般の利用者で走らせるので、手元と比べていない。ARM の Linux は配布に入っていない（[REQ-ssh-005](ir/ssh/fallback.md#REQ-ssh-005)）。

## シェル

- bash にしかない文法は使えない。BusyBox ash と dash は `[ -x ]`・`pwd -P`・`${#var}` の振る舞いが bash と違うところがある。
- CentOS 5 の bash 3.2 は、コマンド置換（`$(…)`）の中の `case` のパターンの ")" をコマンド置換の終わりと読み、構文の誤りにする。CentOS 6 の bash 4.1 は読める。

## エージェントの配置と起動

- OS とアーキテクチャは `uname` で尋ねる。macOS は `arm64` と答える。
- 配置先のディレクトリの確認は `ls -ldn` で行う（`stat -c` のないサーバがある）。利用者の uid は `id -u` で尋ねる。
- 置くときは `mkdir`・`chmod`・`mv`・`rm` を使う。実行ファイルは標準入力で base64 にして送り、`openssl base64 -d -A -out` で復号して書く（sudo のサーバでは前に `sudo` を置く）。
- 置いたバイナリの照合（[REQ-ssh-016](ir/ssh/deploy.md#REQ-ssh-016)）の SHA-256 は `sha256sum`、なければ `shasum -a 256` で取る。`sha256sum` がなく `shasum -a 256` だけのサーバがある。
- 終了ステータスを受け取れずにチャンネルが閉じたコマンドは、結果が分からない（接続断。[用語](ir/CONTEXT.md)）。成功にしない。

## エージェント

- 通信は長さ付きのフレームで、通信の版が手元と同じエージェントだけを使う（[REQ-ssh-021](ir/ssh/fallback.md#REQ-ssh-021)）。今の版は protocol の `define_protocol_version!` の値で、`remote-merge --version` が `protocol v<版>` として出す。操作の失敗は種類（ない・ディレクトリでない・権限がない・上限を超える・I/O）で運ぶ。
- 大きい中身は 4MB ずつに分けて送る。一度に送るパスは 2000 件まで、フレームの上限は 16MB。
- CentOS 5 のカーネル（2.6.18）には `utimensat` がない（ENOSYS）。エージェントが更新時刻を変える操作を足すときは `utimes` を使う。今のエージェントは更新時刻を変えない。2.6.18 で動くことは `scripts/kernel-2.6.18/reproduce.sh` で確かめる。
- CentOS 5 の既定の ext3（128 バイトの inode）は更新時刻を秒の精度でしか持たない。走査はナノ秒の部分が 0 の時刻を秒の精度として載せる。
