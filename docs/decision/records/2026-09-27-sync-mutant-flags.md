# sync のテスト整理で残った問題を FLAG として残す判断

## Context

sync のテスト整理（[計画](../../plans/sync-test-cleanup.md)）で、変異テストの見逃しは [テスト手法の選び方の A8](./2026-09-27-test-method-selection.md#A8) に従い、テストの追加・同等変異の登録・FLAG の記録のどれかで決着させる。
整理後に残った見逃しのうち、書き込む前の確認の行（[REQ-cli-040](../../ir/cli/sync.md#REQ-cli-040)）で件数が 0 の部分を出すか省くかを変える 2 件は、IR の書き方からはどちらが正しいかを決められず、テストを足すとどちらかの扱いを固定することになる。
これを仕様として決めずに、次にこのトピックを扱うときの論点として残す。経緯は "docs/testing/sync-test-cleanup.md" にある。

## Agreements

- A1 未決の FLAG として残す。sync の書き込む前の確認で、実装は書き込むファイルだけがある書き込み先を "[先] N files to merge"、削除予定だけがある書き込み先を "[先] M files to delete" と件数が 0 の部分を省いて出すが、REQ-cli-040 は行の形を "[先] N files to merge, M files to delete" と書いており、件数が 0 の部分を省くかは定めていない。旧総合仕様のマルチサーバ同期の節の出力例 "[server1] 3 files to merge (2 modified, 1 added)" は削除の部分を出していない。変異テストの見逃し 2 件（src/cli/sync.rs の print_sync_plan の件数の比較を ">" から ">=" にするもの）は、この違いでしか検知できない。
  - why: 利用者の判断。承認済みの IR の書き方を直すか実装の食い違いとするかは仕様の判断で、テスト整理の中では決めない。
  - decided_by: user (took the recommendation)
