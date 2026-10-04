# 対話画面（TUI）の扱いの判断

## Context

remote-merge は引数なしで起動すると対話画面（TUI）を開き、サブコマンドを付けると非対話の CLI として動く。
TUI は WebView 方式への移行を想定して凍結中だが、製品 crate の約半分のコード（app・handler・ui と runtime・telemetry の一部）と全テスト 3111 件のうち約 1190 件を占め、疑似端末で動かす遅く不安定なテストも抱えている。
利用者は 2026-10-04 に、以前 terminal-use で調べた際に挙動が怪しく UX も悪く使う価値がないと判断したこと、存在意義がなく保守コストとテスト時間だけが膨らんでいることを示し、TUI の扱いを決めることにした。

## Agreements

- A1 TUI を完全に取り除く。TUI のコード、テスト、仕様、TUI だけが使う依存（ratatui・crossterm・syntect・arboard）を消す。
  - why: 以前 terminal-use で調べた際に挙動が怪しく UX も悪く使う価値がないと利用者が判断し（調査は別の PC で行い記録は残っていない）、凍結したまま保守コストとテスト時間だけが膨らんでいる。feature で切り離すと保守を先送りするだけで、壊れたまま残る。
  - decided_by: user (took the recommendation)
- A2 引数なしで起動したときは使い方を標準エラーに出し、終了コード 2 で終わる。
  - why: これまで TUI を開いていた操作が何も起きないと不親切なので、サブコマンドを付けるよう案内する。引数の誤りと同じ終了コードにする。
  - decided_by: user (took the recommendation)
- A3 TUI にしかない機能（クリップボードとレポートへの出力、サーバの切り替え、検索、三者のバッジ表示とサマリー）は CLI へ移さずに消す。
  - why: 比較・マージ・hunk マージ・三者比較・rollback は CLI で行え、残りは対話画面のための機能である。
  - decided_by: user (took the recommendation)
- A4 CLI の events と、TUI だけが書き込む操作イベントの記録を消す。診断ログ（logs）は CLI も書き込むので残す。
  - why: TUI を除くと操作イベントを書き込むものがなくなる。
  - decided_by: user (took the recommendation)
- A5 TUI だけが使う設定 badge_scan_max_files は、読み込みを止めずに受け付けて使わないことを一度だけ警告する。テーマを保存する state.toml は読み書きをやめ、既存のファイルは消さない。
  - why: 既存の設定ファイルを壊さず、効かなくなったことに気づけるようにする（機密の設定と同じ扱い）。利用者のファイルを勝手に消さない。
  - decided_by: user (took the recommendation)
- A6 Web（WebView 方式）はこの判断では扱わず、TUI を除いた後に改めて決める。
  - why: 何を作るかは TUI の扱いと独立に決められ、今の TUI の状態管理を引き継ぐ前提も置かない。
  - decided_by: user (took the recommendation)

- A7 TUI だけを対象にした仕様の項目（docs/ir/tui の全て、REQ-scan-001、REQ-backup-020 とそれぞれの例、操作イベントと CLI の events の要件と例）を除き、TUI と CLI の両方に関わる項目からは TUI の部分だけを除く。ホスト鍵の例 EX-ssh-001 は確認手段のない接続一般を扱っているので、名前から TUI を外して残す。
  - why: A1・A4 により TUI と操作イベントがなくなる。両方に関わる項目の CLI の部分は変わらない。
  - decided_by: user (took the recommendation)
- A8 TUI の部分を決めた過去の決定は文を書き換えず、この記録への superseded_by を付ける。対象は、除いた項目と TUI の部分を除いた項目の出典をたどって決める。
  - why: 決定の履歴を残したまま、どこが置き換わったかを辿れるようにする。
  - decided_by: user (took the recommendation)
- A9 TUI だけに関わる未決の FLAG は閉じる。TUI のバッジ走査が sudo の検査を通らない FLAG-config-012 も、バッジ走査ごとなくなるので閉じる。
  - why: A1 により前提がなくなる。
  - decided_by: user (took the recommendation)
- A10 引数なしで起動したときの振る舞いの変更と CLI の events の削除は破壊的な変更として扱い、リリースでは版を上げて変更履歴に明記する。
  - why: これまでの使い方とスクリプトが動かなくなるため、利用者が版で気づけるようにする。
  - decided_by: user (took the recommendation)
- A11 手引き（skills/remote-merge/SKILL.md とその説明文）、README.md、PROJECT.md から TUI の説明と凍結の規約を除き、CLI 専用のツールとして書き直す。
  - why: 古い説明が残ると、利用者やエージェントが存在しない画面を使おうとする。
  - decided_by: user (took the recommendation)
- A12 TUI だけが使うテストの道具（疑似端末で TUI を動かす仕組み）を除き、TUI の終了を確かめる手法を決めた記録には A8 と同じく superseded_by を付ける。
  - why: A1 により確かめる対象がなくなる。
  - decided_by: user (took the recommendation)

- A13 badge_scan_max_files の指定に出す警告は "Warning: badge_scan_max_files is no longer used and is ignored" とし、値は検査せず、一回の実行につき一度だけ標準エラーに出す。
  - why: A5 の警告を、機密の設定の警告と同じ形で利用者が気づけてテストで確かめられるように決める。使わない値の範囲で設定全体を止めない。
  - decided_by: user (took the recommendation)

- A14 CLI のサブコマンドの実行も診断ログを保存し、CLI の logs で閲覧できるようにする。リモートで内部的に動く agent サブコマンドは保存しない。保存する内容の規則（ファイルの中身や認証情報を含めない）は変えない。
  - why: これまで診断ログを保存していたのは TUI だけで、TUI を除くと logs が読む記録がなくなる。障害を調べる手段を残し、エージェントが使った後からも原因を追えるようにする。agent はリモートのサーバ上で動くので、利用者の手元の記録にならず、リモートに余計なファイルを残す。
  - decided_by: user (took the recommendation)
- A15 hunk マージの前に衝突と書き込みを確認する要件 REQ-merge-010 とその例 EX-merge-019・EX-merge-020 を除き、CLI の --hunks は他の CLI の merge と同じく確認を出さない。衝突の確認は --ref があり --force がないときの既存の確認に任せ、この食い違いを記録した FLAG-merge-016 を閉じる。
  - why: これらを確かめていたのは TUI だけで、CLI の merge は確認を出さない非対話の設計で揃っている。
  - decided_by: user (took the recommendation)

## Undecided

- U7 Web（WebView 方式）を作るか、何を作るか。
  - decides: user

## Revisions

- 2026-10-04: 当初の未決 U1〜U6 を、利用者が推奨を採って A1〜A6 で決めた。U6 の Web は U7 として未決のまま残した。境目の細部は A7〜A13 で決めた。
- 2026-10-04: A4 の理由に「診断ログは CLI も書き込む」と書いたが、実際に診断ログを保存していたのは TUI だけだった。A4 で logs を残す判断は変えず、CLI でも保存することを A14 で決めた。
- 2026-10-04: A7 で除くとしたホスト鍵の例 EX-ssh-001 は、確認手段のない接続一般を扱う CLI のテストに付いていたため、除かずに名前から TUI を外した。
