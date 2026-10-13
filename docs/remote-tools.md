# リモートの道具の違い

SSH の経路は、リモートのシェルと道具（find・stat・dd など）を使う。このツールは古いサーバ（CentOS 5 など）や BusyBox のサーバでも動かすので、道具の違いに合わせる必要がある。この文書は、その違いのうち確かめた事実だけを並べる。どう書くかは決めない（[土台の作り直し A8](decision/records/2026-10-12-foundation.md#A8)）。

## 対象にするリモート

- ログインシェル: Bourne 系（sh・bash・dash・BusyBox ash）と fish。コマンドは `sh -c` に一つの引数で渡す（[REQ-ssh-020](ir/ssh/remote-shell.md#REQ-ssh-020)）。
- findutils: 4.2.27（CentOS 5）、4.4.2、4.5.11、4.9.0、4.10.0 をコンテナで確かめた。BusyBox の find と bfs も来うる。
- 引数の長さ: CentOS 5 の ARG_MAX は 128KB。一つのコマンド行は、その半分を超えないように分ける。

## シェル

- 出力を読む前提として、スクリプトの先頭で `LC_ALL=C` を代入する。道具のメッセージの言語と、バイトの扱いをそろえるため。
- BusyBox ash と dash は、`[ -x ]`・`pwd -P`・`${#var}` の振る舞いが bash と違うところがある。bash にしかない文法は使えない。
- bash 3.2（CentOS 5 の sh）は、シェルの関数を一回呼ぶ費用が、そのときの位置引数（`"$@"`）の数に比例して増える。何もしない関数を 2,500 回呼ぶと、位置引数が 0 個なら約 18 ミリ秒、2,500 個なら約 620 ミリ秒、10,000 個なら約 1.8 秒かかった。数千の引数を受けて、その一つずつについて関数を呼ぶと、全体で引数の数の二乗に比例して遅くなる。
- コマンド置換（`$(…)`）は、呼ぶたびにプロセスを一つ起こす。数千のパスを一つずつコマンド置換で変数に入れると、秒の単位で遅くなる。
- `{ e=$(cat "$p" 2>&1 >&3 3>&-); c=$?; } 3>&1` で、cat の中身をそのまま標準出力に出し、標準エラーを変数 `e` に、終了コードを `c` に分けて取れる。CentOS 5〜7 の bash、Ubuntu の dash、Alpine 3.20 の BusyBox ash で確かめた。
- 作業ディレクトリが消えているとき、dash（Ubuntu の sh）と BusyBox ash の組み込みの `pwd -P` は空の行を出して終了コード 0 を返す。bash（CentOS 5〜7 の sh）の `pwd -P` と `/bin/pwd -P`（coreutils と BusyBox）は終了コード 1 を返す。終了コードだけでは確かめられないので、出力が "/" で始まるかも見る。
- noclobber（`set -C`）の `>` は、書き込み先がないときに O_CREAT|O_EXCL で開く。すでにある通常ファイル、リンク切れの symlink、通常ファイルへの symlink には開かずに失敗し、文言は bash（CentOS 5〜7）が "cannot overwrite existing file"、dash（手元）と BusyBox 1.37.0 の ash が "File exists" で終わる。すでにある通常ファイルでないもの（/dev/null を指す symlink など）は辿って開く。`{ …; } >"$p"` の開く失敗では、どのシェルも終わらずに次の文へ進み、`{ }` の中は動かない。CentOS 5〜7 の bash、手元の dash、BusyBox 1.37.0 の ash で確かめた。
- CentOS 5 の bash 3.2 は、コマンド置換（`$(…)`）の中の `case` のパターンの ")" をコマンド置換の終わりと読み、構文の誤りにする。CentOS 6 の bash 4.1 は読める。
- `IFS= read -r 変数` で標準入力の一行を読める。終わりの改行がないと、変数には読めた分が入り、終了コードは 1 になる。CentOS 5〜7 の bash、手元の dash、BusyBox 1.37.0 の ash で確かめた。
- `readlink` はオプションなしの形だけを使う（`-f` などは古い coreutils と BusyBox で違う・ない）。`readlink` 自体がないサーバもある（[REQ-paths-009](ir/paths/routes.md#REQ-paths-009)）。

## find

- `-printf` の書式を使える（4.2.27 にもある）。BusyBox の find（1.37.0 で確かめた）は `-printf` を受け付けない。名前とリンク先に TAB・改行が入りうるので、項目を NUL（`\0`）で区切って出す。
- `find / -prune -printf '' -exec sh -c 'exit 0' sh {} +` は、4.2.27（CentOS 5）・4.4.2（CentOS 6）・4.5.11（CentOS 7）で終了コード 0、BusyBox 1.37.0 で 1（-printf を知らない式とする）で終わる。BusyBox 1.37.0 の find は `-exec … {} +` だけなら 0 で終わる。
- `-mindepth 1 -maxdepth 1` を起点の直後に置くと、4.2.27・4.4.2・4.5.11 のどれも知らせを出さずに直下の子だけを出す。
- `%T@` の桁は版で違う。4.2.27 は秒しか出さず、1970 年より前の -1000000.25 秒を "-1000001" と出す。新しい版は小数部を出す。出た桁までを読む（[FLAG-scan-015](ir/scan/FLAGS.md#FLAG-scan-015)）。
- 新しい版の `%T@` は秒の整数部と秒未満のナノ秒をそのまま並べる。1970 年より前の -1000000.25 秒は "-1000001.7500000000" と出る（4.9.0 で確かめた）。
- `%Y`（リンク先を辿った種類）は、途中にディレクトリでないものを含むリンクを版で違って出す。4.9.0 はリンク切れと同じ "N" を出し、4.2.27 は "d" を出したうえで標準エラーに "Not a directory" を出し、終了コードは 0 のままである。リンク切れと辿れないリンクを見分けるのには使えない。シェルの `[ -d ]`・`[ -e ]` と、`ls -dL` の文言の終わり（"No such file or directory" かどうか）なら、4.2.27・4.4.2・4.5.11・4.9.0 と BusyBox で同じに分かれる。
- 読めないディレクトリの標準エラーの行は、4.5.11 以降は引用の中の UTF-8 でないバイトなどを C の書き方（"\346" のような 8 進）で書く。
- 読めるが検索の権限のないディレクトリ（mode 0644 など）は、版で知らせ方が違う。4.2.27 はディレクトリ自身を読めないと知らせて子の名前を出さない。4.4.2 は子があっても何も知らせず、終了コード 0 でディレクトリだけを出す。4.5.11 以降は子ごとに情報を読めないと知らせる。find の知らせからディレクトリを読めるかを推し量ることはできない。
- `-exec コマンド {} +` は 4.2.27 でも使える。まとめる引数の数は find が引数の長さの上限に合わせて分ける。
- 4.2.27 はパスを引用せずに知らせるので、名前に改行を含むと知らせの行が割れる。
- `-L` で走らせると、find 自身がリンク先を辿り、循環や辿れないリンクの診断を標準エラーに出す。その文言は版で違う（GNU findutils 4.5.11 以降の C ロケールはパスを `'` で引用、4.4.2 は "`" で始めて引用、4.2.27 は引用せず循環を別の文言で出す）。exclude の下の診断を見分けるのはこの違いで難しいので、find にはリンク先を辿らせない（[土台の段 2 C5](decision/records/2026-10-12-foundation-2.md#C5)）。
- `-name`・`-path` のパターンは fnmatch の規則で、このツールの exclude の glob と同じ意味にならない形がある（"**" と、"?"・"[" を含むものの一部）。同じ意味に書けるパターンだけを find の刈り込み（`-prune`）に使える。
- 一覧に出た名前が、情報を取りに行く前に消えたときは、どの版もその名前について "No such file or directory" と知らせ、終了コードは 1 になる。パスの引用は読めないディレクトリの知らせと同じく版で違う（4.2.27 は引用しない、4.4.2 は "`" で始めて `'` で閉じる、4.5.11 と 4.9.0 は `'` で囲む。いずれも C ロケール）。4.2.27・4.4.2・4.5.11・4.9.0 で、find の途中で項目を消して確かめた。
- 標準エラーに何か出たのに終了コードが 0 のことがある（4.2.27）。終了コードだけで成功を決められない。

## 失敗の理由の見分け方

root でない利用者で、C ロケール（`LC_ALL=C`）で確かめた。対象は CentOS 5（coreutils 5.97）・CentOS 6（8.4）・CentOS 7（8.22）のコンテナ、手元の Ubuntu（coreutils 9.4、sh は dash）、Alpine の BusyBox 1.37.0。

- シェルのテストが偽で理由が分からないときの `ls -d` の標準エラーは、どれも OS の文言で終わる。ないパスは "No such file or directory"、途中が通常ファイルのパス（通常ファイルへの symlink を途中に含むものも）は "Not a directory"、検索の権限がないディレクトリの下のパス（その下の何段目でも）は "Permission denied" である。前に付く文言（"ls: パス: "・"ls: cannot access パス: "・"ls: cannot access 'パス': "）は版で違うので、終わりだけを読む。終了コードは coreutils が 2、BusyBox が 1 である。CentOS 5〜7 と BusyBox 1.37.0 のコンテナで uid 1000 の利用者として確かめた（CentOS 5 と BusyBox は "ls: パス: "、CentOS 6・7 は "ls: cannot access パス: " を前に付ける）。検索の権限がないディレクトリの下のパスは `[ -d ]` も偽になる。
- 自分を指す symlink の下のパスは、coreutils が "Too many levels of symbolic links"、BusyBox が "Symbolic link loop" で終わる。
- 検索の権限がないディレクトリそのものは、`[ -e ]`・`[ -d ]` が真で、`[ -r ]`・`[ -x ]` が偽になり、`ls -d` は成功する。その下のパスは `[ -e ]`・`[ -L ]` がどれも偽になる。
- リンク切れの symlink は `[ -L ]` が真で `[ -e ]` が偽になる。coreutils の `ls -d` はリンクそのものを示して成功し、BusyBox の `ls -d` はリンク先を辿って "No such file or directory" で失敗する。
- `cat`・`rm`・`chmod`・`stat -L -c %s`・`sha256sum`・`ls -ldn` の失敗の文言も、ない・途中がディレクトリでない・権限がないで、上と同じ三つの文言で終わる（前に付く文言は道具と版で違う）。
- 他人の持ち物のファイルの権限を変えようとすると、`chmod` は "Operation not permitted"（EPERM）で終わる。
- 読む権限のないファイル（mode 000）は `[ -e ]` が真で `[ -r ]` が偽になり、`cat`・`sha256sum` は "Permission denied" で終わる。
- `mkdir -p` で、作ろうとした親がすでにある通常ファイルのとき、CentOS 6・7・手元・BusyBox は "File exists" で、CentOS 5 は "exists but is not a directory" で終わる。その下（`f/a` の `f` が通常ファイル）なら、どれも "Not a directory" で終わる。書き込めないディレクトリの下に作ろうとすると "Permission denied" で終わる。ロケールを C にしないと、引用符などの文言が変わる。

## ハッシュ・大きさ・書き込み

- SHA-256: `sha256sum` がなく `shasum -a 256` だけのサーバがある。
- 大きさ: `stat -L -c %s` で尋ねられる。`stat -c` のないサーバ（エージェントの配置先の確認で避けた例）では `ls -ldn` を使う。
- 書き込み: 中身は標準入力で渡す。バイナリは base64 にして `openssl base64 -d -A` で戻せる。書き込み先が symlink のときに辿らないよう `dd oflag=nofollow` を使えるが、受け付けない dd がある（そのときの扱いは [REQ-merge-038](ir/merge/concurrency.md#REQ-merge-038)）。古い dd は `status=none` を受け付けない。受け付けない dd（CentOS 5 の coreutils 5.97）は、書けずに終わったときも理由の行の後に転送量の行（"1+0 records in"・"0+0 records out"・"0 bytes (0 B) copied, …"）を標準エラーに出す（/dev/full への書き込みで確かめた）。転送量の行はどれも数字で始まる（"N+N records in"・"N+N records out"・"N bytes"）ので、理由の行と見分けるのは行の始まりの形で行い、"bytes" や "copied" を含むかでは見分けない（パスにその語を含む理由の行がある）。coreutils の dd（CentOS 5〜7 と手元）は `conv=excl` を受け付け、BusyBox 1.37.0 の dd は `oflag=nofollow` も `conv=excl` も受け付けない。受け付けるかは `dd if=/dev/null conv=excl count=0 </dev/null` が 0 で終わるか（coreutils）、1 で終わるか（BusyBox）で分かる（`of=/dev/null` に付けると、/dev/null があるので coreutils でも "File exists" で失敗する）。`oflag=nofollow conv=excl` の dd は、すでにある通常ファイル・リンク切れの symlink・/dev/null を指す symlink のどれにも開かずに "File exists" で終わる（前に付く文言は版で違い、CentOS 5・6 は "dd: opening `パス': "、CentOS 7 と手元は "dd: failed to open 'パス': "）。coreutils の dd（CentOS 5〜7）は `conv=nocreat` も受け付け、`dd if=/dev/null of=パス oflag=nofollow conv=nocreat,notrunc count=0` はすでにあるファイルの中身を変えず、ないパスには作らずに "No such file or directory" で終わる。BusyBox 1.37.0 の dd は `conv=nocreat` を受け付けない（"invalid argument 'nocreat' to 'conv'"）。
- rmdir: ないパスは "No such file or directory"、通常ファイルとディレクトリを指す symlink は "Not a directory" で終わり、どれも消さずに 1 で終わる（CentOS 5〜7・手元・BusyBox 1.37.0）。
- symlink の作成: `ln -s -T` が使えないサーバがある。削除は `rm`（`-f` を付けると、ないことを成功にしてしまう）。

## SSH の実行

- 終了ステータスを受け取れずにチャンネルが閉じたコマンドは、結果が分からない（接続断。[用語](ir/CONTEXT.md)）。成功にしない。
- sudo のサーバでは、エージェントが使えないときに SSH の経路へ切り替えない（[REQ-ssh-005](ir/ssh/fallback.md#REQ-ssh-005)）。

## エージェント

- 通信は長さ付きのフレームで、通信の版が手元と同じエージェントだけを使う（[REQ-ssh-021](ir/ssh/fallback.md#REQ-ssh-021)）。今の版は 15。操作の失敗は種類（ない・ディレクトリでない・権限がない・上限を超える・I/O）で運ぶ。
- 大きい中身は 4MB ずつに分けて送る。一度に送るパスは 2000 件まで、フレームの上限は 16MB。
- 配置先のディレクトリの確認は `ls -ldn` で行う（`stat -c` を避ける）。
