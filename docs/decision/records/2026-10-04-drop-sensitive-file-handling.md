# 機密ファイルの特別扱いをやめる判断

## Context

設定の sensitive パターンに一致するファイル（機密ファイル）を、diff では既定で中身を隠し、merge・sync・rollback では --force がなければ対象から外し、TUI では持ち出す前に確認するなど、ツールの各所で特別に扱ってきた（根拠は [移行時の A17・A19・A50・A52](./2026-09-25-spec-migration.md#A19)）。
この扱いは権限の境界ではなく（ツールは両側を読める権限で動き、--force で外れる）、仕様と実装の複雑さと、未決の FLAG を多く生んでいる。
利用者は 2026-10-04 に、機密を守ることはこのツールの責務を超えており外側のレイヤーで守るべきだと判断した。
この記録は、その方針の下で機密ファイルの扱いをどこまで、どう取り除くかを決める。

## Agreements

- A1 機密を守ることはこのツールの責務としない。ファイルの中身を誰に見せてよいか、どのファイルを書き換えてよいかは、ツールの外側（実行する人やエージェントの権限、実行環境、除外の設定）で守る。
  - why: ツールは両側を読み書きできる権限で動き、--force で特別扱いを外せるので、ツールの中の機密の扱いは守りになっていない。git diff と同じく、渡されたものを渡されたとおりに扱うのがこのツールの役割である。
  - decided_by: user

- A2 機密ファイルの特別扱いを全てやめ、機密ファイルを他のファイルと同じに扱う。対象は diff の中身とハッシュの非表示と symlink の連鎖の判定、status の --ref での比較除外と印、merge・sync の書き込みと削除からの除外と警告、--hunks の停止、rollback の除外、TUI のクリップボードとレポートへの持ち出し確認である。
  - why: A1 の方針により、ツールの中で中身の見せ方や書き換えの可否を機密かどうかで変えない。
  - decided_by: user (took the recommendation)
- A3 ツール自身の診断の記録（logs・events）にファイルの中身や認証情報を含めない規則は残す。
  - why: 利用者のファイルを機密かどうかで分ける規則ではなく、ツールが自分の記録に余計なデータを書かないという出力の作法であり、A1 の対象ではない。
  - decided_by: user (took the recommendation)
- A4 既存の設定に残る sensitive の指定は、読み込みを止めずに受け付け、使われないことを警告する。
  - why: 既存の設定ファイルを壊さず、効かなくなったことに利用者が気づけるようにする。エラーで止めると既存の設定が使えなくなり、黙って無視すると効いていると誤解させる。
  - decided_by: user (took the recommendation)
- A5 JSON の "sensitive" 欄と、テキストの " [SENSITIVE]" の印を出力から除く。JSON の形が変わる破壊的な変更として扱い、リリースでは版を上げて変更履歴に明記する。
  - why: 常に false の欄を残すと、まだ判定しているように誤解させる。
  - decided_by: user (took the recommendation)
- A6 --force の機密以外の役目（リモート間の merge を止めない、--ref の参照先に対する確認をしない、読み取りの大きさの上限を外す、rollback の確認のプロンプトを省くことと期限切れのセッションを戻せること）は残し、説明とヘルプから機密の記述を除く。
  - why: これらは書き込み先の取り違えや大きな転送を防ぐ確認で、機密を守る規則ではない。
  - decided_by: user (took the recommendation)
- A7 機密ファイルに関わる未決の FLAG（FLAG-cli-022・034・037・046・070、FLAG-cli-063 の機密の部分）は、この変更で前提がなくなるので閉じる。機密に関わらない部分が残るものは、その部分だけを FLAG に残す。
  - why: 機密ファイルを特別に扱わなければ、これらの問いは生じない。
  - decided_by: user (took the recommendation)

- A8 用語集から「機密ファイル」を除き、仕様ではこの言葉を使わない。
  - why: A2 により、機密ファイルを他のファイルと区別する規則がなくなる。
  - decided_by: user (took the recommendation)
- A9 設定に [filter] の sensitive の指定があるときは、標準エラーに "Warning: [filter] sensitive is no longer used and is ignored" を一回の実行につき一度だけ出し、指定を使わずに続ける。グローバル設定とプロジェクト設定のどちらに書いてあっても同じとする。
  - why: A4 の警告を、利用者が気づけてテストで確かめられる形に決める。
  - decided_by: user (took the recommendation)
- A10 凍結中の TUI からも、機密の持ち出し確認のダイアログと、一括マージの確認画面の機密の表示を除く。凍結の例外は除くことに限り、新しい機能は足さない。
  - why: A2 の方針を TUI にも揃える。除くだけなら凍結の目的（WebView 方式への移行前に手を広げない）に反しない。
  - decided_by: user (took the recommendation)
- A11 移行時の [A17](./2026-09-25-spec-migration.md#A17)・[A19](./2026-09-25-spec-migration.md#A19)・[A50](./2026-09-25-spec-migration.md#A50)・[A52](./2026-09-25-spec-migration.md#A52) は文を書き換えず、この記録への superseded_by を付ける。A17 はリモート間の部分が残り、機密の部分だけをこの記録が置き換える。
  - why: 決定の履歴を残したまま、どこが置き換わったかを辿れるようにする。
  - decided_by: user (took the recommendation)
- A12 手引き（skills/remote-merge/SKILL.md、skills/remote-merge/references/json-schemas.md）と README.md から機密の扱いの説明を除き、読ませたくないファイルはツールの外側か exclude の設定で外すと案内する。
  - why: 手引きが古いままだと、利用者やエージェントがツールの中で守られていると誤解する。
  - decided_by: user (took the recommendation)
- A13 診断の記録の規則は中身を変えず、その例の「機密の内容」を「ファイルの中身」に言い換える。
  - why: A3 で規則を残し、A8 で用語を除くため、言葉だけを合わせる。
  - decided_by: user (took the recommendation)

## Revisions

- 2026-10-04: 当初の未決 U1〜U5 を、利用者が推奨を採って A2〜A7 で決めた。境目の細部は A8〜A13 で決めた。
- 2026-10-04: 照合で、A11 が挙げた移行時の四つの決定のほかにも、機密の扱いを決めた決定（diff の symlink の機密の判定、status・rollback・merge の削除の機密の扱い、既定の sensitive のパターン、機密に関わる FLAG を残す決定など）があると分かったため、それらにも同じ superseded_by を付けた。
