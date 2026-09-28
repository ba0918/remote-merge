# merge で書き込み先の変更を黙って失う二つの問題を直す判断

## Context

merge の取り込みで、書き込み先の変更を黙って失う疑いが二つ FLAG に残った。
一つは [FLAG-cli-025](./2026-09-28-merge-cli-mutant-flags.md#A1) で、--ref があっても左右が参照先の別々の箇所を変えたファイルを競合とせず、左の中身で上書きして右の変更を失う。
もう一つは [FLAG-merge-015](./2026-09-28-adopt-merge-hunks.md#A5) で、diff --format json が示す hunk の番号と --hunks が数える番号の区切りが違い、選んだものと違う変更を書き込みうる。
どちらもデータを失うため、取り込みの続きより先に直すと利用者が決めた。二つの FLAG をここで回収し、合わせて --hunks の指定のエラーの表の一行を実装に合わせる。

## Agreements

- A1 --ref があり、参照先を実際に使い（左か右と同じ参照先は使わない）、--force も --dry-run もないファイル全体の merge は、左右と参照先の三つとも中身を読めた通常ファイル（バイナリを含む）のうち読み込み元と書き込み先の中身が違うものについて、中身をバイト列で比べて書き込み先が参照先から変わっていれば書き込まずに failed に出す。左右の両方が参照先から変わっていれば error を "three-way conflict"、書き込み先だけが変わっていれば "destination changed since reference" とする。読み込み元だけが参照先から変わったファイルは書き込み、--force があればこの確認をせずに書き込む。三つのどれかで中身を読めないファイルの扱いは FLAG-cli-023 のまま未決とする。
  - why: ファイル全体の merge は書き込み先を読み込み元の中身でまるごと置き換えるため、書き込み先の変更は変わった箇所にかかわらず失われる。参照先を指定した利用者は書き込み先の変更を守りたいので、書き込み先が参照先から変わっていれば止めるのがいちばん単純で安全である。右だけが変わったときに "three-way conflict" と言うのは事実と違うため、別の文言にし、既存の "three-way conflict" を読む利用者は壊さない。
  - rejected: 重ならない変更を行単位で混ぜ合わせる三者マージ（新しい機能になり、今回の修正の範囲を超える）。スキップとして扱う（黙って飛ばすと利用者が気づきにくい）。
  - decided_by: user (took the recommendation)
- A2 --hunks の merge の競合の判定は変えず、参照先に対して左右が同じ箇所を異なる内容に変えたときだけを競合とする。
  - why: --hunks は書き込み先の中身に選んだ変更だけを当てるため、書き込み先が別の箇所で変えた内容は残る。A1 の規則を当てると正当な部分マージまで止まる。
  - decided_by: user (took the recommendation)
- A3 --hunks の番号は、diff --format json の hunks と同じ区切り（変更の前後 3 行の文脈でまとめたもの）を 0 から数えたものとし、hunk の数と番号の範囲外の判定もこの区切りで数える。一つの区切りに複数の変更のかたまりが入るとき、その番号を選べばその全てを適用する。番号は、同じ --left・--right・パスで実行した diff --format json の番号と一致し、--hunks のテキストの総数と JSON の hunks_total もこの区切りで数える。
  - why: 利用者は diff の出力で番号を調べて --hunks に渡すため、見る番号と書き込む番号を一致させる。diff の出力の形は変えずに済む。
  - rejected: diff --format json の hunks を変更ごとの区切りに変える（diff の出力を読む利用者に影響する）。
  - decided_by: user (took the recommendation)
- A4 「競合」は、参照先に対して左右が同じ箇所を異なる内容に変えたことだけを指す。同じ箇所とは、参照先からの変更の行の範囲が重なることで、バイナリや UTF-8 として読めないファイルはファイル全体を一つの箇所とする。左右の両方が参照先から変わっていても、変わった箇所が重ならなければ競合と呼ばない。
  - why: 差分表示の REQ-cli-016 はこの意味で競合を示しており、A1 の規則を「競合」と呼ぶと同じ言葉が二つの意味になる。
  - decided_by: user (took the recommendation)
- A5 今回は FLAG-cli-025 と FLAG-merge-015 だけを直し、--dry-run で競合を確かめないこと（FLAG-cli-024）、--hunks が確認を出さないこと（FLAG-merge-016）、--hunks に書き込み直前の確認がないこと（FLAG-merge-017）は FLAG のまま残す。
  - why: 残す三つはデータを黙って失う問題とは別で、--dry-run の予告を変えると --dry-run の終了コード（FLAG-cli-017）にも触れるため、別の回でまとめて決める。
  - decided_by: user (took the recommendation)
- A6 --hunks の指定のエラーの表の「パスが一つでない」の行を「パスが二つ以上」に絞る。パスがない merge は引数の解析で止まり "--hunks requires exactly one path (got 0)" は出ないため、その場合は merge の指定のエラーの表の「パスがない」の行（引数の解析エラー）に従う。
  - why: 表の行が実装の出さない文言を約束していた（取り込みのレビューで実行して確かめた）。実装は変えず、表を実装に合わせる。
  - decided_by: user (took the recommendation)
- A7 三者比較の参照先を書き換えない例（EX-cli-021）は、書き込み先が参照先と同じで読み込み元だけが変わったファイルを --ref を指定して左から右へ merge し、右だけが更新され参照先が変わらないことを示す形にする。
  - why: 三つとも中身が違うファイルは A1 で書き込まれなくなり、元の例は A1 と食い違う。例の目的は参照先を書き換えないことなので、書き込みが起きる構成に直す。
  - decided_by: user (took the recommendation)
- A8 通常ファイルの merge が指定した書き込み先だけを読み込み元の内容に更新する前提に、参照先を使うときは書き込み先が参照先から変わっていないことを加える。
  - why: 前提が競合と外部更新のないことだけでは、A1 で書き込まないファイルまで更新すると読める。
  - decided_by: user (took the recommendation)
- A9 未決の FLAG として残す。--ref を使う merge で読み込み元か書き込み先が symlink のとき、A1 の確認が symlink を何で比べるか（リンク先の文字列か、辿った先の中身か）を実装から確かめておらず、A1 の対象から外す。
  - why: 今回の修正はデータを黙って失う通常ファイルの問題に絞り、symlink の比べ方は実装を確かめてから決める。
  - decided_by: user (took the recommendation)
- A10 残っている FLAG のうち、ファイル全体の merge で参照先に対する確認に落ちるファイルを「三者の競合」と呼んでいた本文（--force の働き、--dry-run で確かめないこと、書き込むファイルのない merge）は、A1 で書き込まないファイル（"three-way conflict" と "destination changed since reference" の両方）を指すものと読み、本文の言い方をそれに合わせる。
  - why: A4 で「競合」を同じ箇所の変更に限ったため、古い言い方のままでは A1 の書き込み先だけが変わったファイルが含まれるかが読み分けられない。
  - decided_by: user (took the recommendation)
- A11 三者比較の比較結果に競合を示す規則と、--hunks の merge を三者の競合で止める規則は、A4 の「競合」の意味で言い直す。--hunks の merge は、選んだ hunk の中だけでなくファイルのどこかに競合があれば止める。
  - why: 二つの規則は FLAG-cli-025 の曖昧さの元になった「左右が異なる変更」という言い方のままで、--hunks の実装はファイル全体で競合を判定している。
  - decided_by: user (took the recommendation)

## Revisions

- A1 は [merge の指定・確認・出力の取り込みの A6](./2026-09-28-adopt-merge-cli.md#A6)（競合のあるファイルだけを "three-way conflict" で失敗とする）を置き換える。
- A6 は [merge の変更のまとまりを選ぶマージの取り込みの A1](./2026-09-28-adopt-merge-hunks.md#A1) の「パスが一つでない」を「パスが二つ以上」に絞る。
- A7 は既存の例 EX-cli-021 の構成を、A8 は既存の要件 REQ-merge-018 の前提を、A10 は FLAG-cli-021・024・026 の本文の言い方を、A11 は REQ-cli-016 と REQ-merge-031 の言い方を改める。
