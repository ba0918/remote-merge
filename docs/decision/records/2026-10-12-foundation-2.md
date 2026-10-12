# 土台の段 2: 走査を一つの規則にする

## Context

[土台の作り直し](./2026-10-12-foundation.md) の段 2 である。走査は三つの経路に別々の実装があり、exclude・include・件数・UTF-8 でない名前・循環・root_dir や部分パスがないときの判定を、それぞれが自分で決めていた。IR も、同じ規則を要件ごとに「ローカル・SSH・エージェントのどの経路でも」と繰り返し、経路の差を FLAG（FLAG-config-016・FLAG-scan-002・FLAG-scan-007・FLAG-scan-016）に残していた。[foundation A10](./2026-10-12-foundation.md#A10) のとおり、実装の前に IR を経路によらない文に書き直す。

Position: C1〜C7 は決まった。IR の書き直しから始める。

## Agreements

- C1 走査の結果（一覧・不完全の報告・エラー）が経路によらないことを、一つの要件にまとめる。個々の要件からは「ローカル・SSH・エージェントのどの経路でも」を外す。経路で変わりうることは、道具の限界で揃えられないと確かめたものだけを FLAG に残す（今は FLAG-scan-015 の秒しか出さない find の更新時刻だけ）。
  - why: 同じ規則を要件ごとに繰り返す書き方は、経路ごとに実装を書く構造を写していた。
  - decided_by: 利用者（[foundation A10](./2026-10-12-foundation.md#A10) から導いた。自走の指示により書き下ろした）
- C2 走査は、起点（root_dir 全体、または指定したパス）が何であっても、exclude と include を root_dir からの相対パスに当てる。merge・sync のパスの指定で親ディレクトリの下だけを走査するときも同じにする。[FLAG-scan-007](../../ir/scan/FLAGS.md#FLAG-scan-007) はこれで閉じる。
  - why: [REQ-config-021](../../ir/config/filters.md#REQ-config-021)・[REQ-config-022](../../ir/config/filters.md#REQ-config-022) は root_dir からのパスに当てると決めており、走査の範囲で当て方が変わるのは誤りだった（include の外や除外したディレクトリの下に書き込む）。
  - decided_by: 利用者（[foundation A3](./2026-10-12-foundation.md#A3) と 2026-10-05 の品質の基準から導いた。自走の指示により書き下ろした）
- C3 diff が名前で指定したディレクトリの配下を展開するときも、同じ走査（起点をそのディレクトリにしたもの）を使う。UTF-8 でない名前は [REQ-scan-012](../../ir/scan/listing.md#REQ-scan-012) のとおり読めない名前として出す。[FLAG-scan-016](../../ir/scan/FLAGS.md#FLAG-scan-016) はこれで閉じる。
  - why: 展開だけ別の一覧の仕方を使うと、同じファイルをパスの指定の仕方で別に扱う。
  - decided_by: 利用者（[foundation A3](./2026-10-12-foundation.md#A3) から導いた。自走の指示により書き下ろした）
- C4 status・diff・merge・sync のどのコマンドの走査も、途中のディレクトリへの symlink の配下を辿って一覧に載せる（[REQ-scan-002](../../ir/scan/directory-links.md#REQ-scan-002) を status だけでなく四つのコマンドの要件にする）。[FLAG-scan-002](../../ir/scan/FLAGS.md#FLAG-scan-002) はこれで閉じる。
  - why: 今の動き（四つとも辿る）と、共有ディレクトリへの symlink の中を比べる [skip-subtree A4](./2026-10-12-skip-subtree.md#A4) が同じ向きで、旧仕様の「辿らない」はそれと食い違う。
  - decided_by: 利用者（skip-subtree A4 から導いた。自走の指示により書き下ろした）
- C5 exclude に当たる項目と、exclude に当たるディレクトリの下は、どの経路でも読みに行かず、そこにある循環・読めないディレクトリ・辿れないリンクで走査を止めない。[FLAG-config-016](../../ir/config/FLAGS.md#FLAG-config-016) は、SSH の経路の走査を、find にリンク先を辿らせない形に替えて閉じる。辿るかどうかと循環の検出は、走査の規則の側で決める。
  - why: find にリンク先を辿らせると、find の版ごとに違う診断の文言を読み分ける必要があり、古い find のサーバでだけ結果が変わっていた。
  - decided_by: 利用者（[foundation A3](./2026-10-12-foundation.md#A3)・[foundation A5](./2026-10-12-foundation.md#A5) から導いた。自走の指示により書き下ろした）
- C6 走査の規則（exclude・include・件数・UTF-8 でない名前・循環・辿るか・読めないときに止めるか）は一つの純粋な関数群に置き、経路は生の項目（名前のバイト列・種類・大きさ・更新時刻・権限・リンク文字列・デバイス番号と inode 番号）を集めるだけにする。ローカルとエージェントは同じ集め方を使う（エージェントはリモートのマシンの上のローカルである）。SSH は find の出力を同じ生の項目に訳す。
  - why: 規則を一か所に置けば、経路の差は生の項目の集め方にしか生まれず、そこは生の項目の比較で確かめられる。
  - decided_by: 利用者（[foundation A3](./2026-10-12-foundation.md#A3) から導いた。自走の指示により書き下ろした）
- C7 走査の規則は、まとめて集める形で動く（[foundation A5](./2026-10-12-foundation.md#A5)）。SSH は一度の find で部分木の生の項目を集め、ディレクトリへの symlink を辿るときだけ、そのリンク先をもう一度まとめて集める。
  - why: 項目ごと・ディレクトリごとに尋ねると SSH の往復で使えない速さになる。
  - decided_by: 利用者（[foundation A5](./2026-10-12-foundation.md#A5) から導いた。自走の指示により書き下ろした）
