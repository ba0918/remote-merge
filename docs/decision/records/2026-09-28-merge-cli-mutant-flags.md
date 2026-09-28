# merge の指定・確認・出力のテスト整理で残った問題を FLAG として残す判断

## Context

merge の指定・確認・出力のテスト整理（[計画](../../plans/merge-cli-test-cleanup.md)）で、変異テストの見逃しは [テスト手法の選び方の A8](./2026-09-27-test-method-selection.md#A8) に従い、テストの追加・同等変異の登録・FLAG の記録のどれかで決着させる。
整理後に残った見逃しのうち 2 件は、IR がその場合の振る舞いを決めておらず、テストを足すとどちらかの扱いを固定することになる。
これを仕様として決めずに、次にこのトピックを扱うときの論点として残す。経緯は "docs/testing/merge-cli-test-cleanup.md" にある。

## Agreements

- A1 未決の FLAG として残す。--ref があり --force のない merge で、参照先に対して左右がテキストのファイルの別々の箇所を変えたとき（参照先 "a b c d e"、左 "A b c d e"、右 "a b c d E" の各行）、実装は重なる変更がないため競合とせず、書き込み先を左の中身で上書きし、右の変更 "E" は失われ、failed は空で終了コード 0 になる。REQ-cli-016 と REQ-cli-051 の「参照先に対して左右が異なる変更」に別々の箇所の変更が含まれるかを IR は決めておらず、含むと読むなら実装は REQ-cli-017 と食い違い、書き込み先の変更を黙って失う不具合の疑いがある。変異テストの見逃し（src/service/merge.rs の has_three_way_conflict でテキストの比較の分岐を消すもの）はこの違いでしか検知できない。
  - why: 利用者の判断。競合の範囲を決めるのは仕様の判断で、テスト整理の中では決めない。
  - decided_by: user (took the recommendation)
- A2 未決の FLAG として残す。バックアップが有効で集約先の場所が決まらない構成で、書き込むファイルが一つもない merge のとき、実装は全てのファイルが参照先に対する競合で外れた場合はエラーで止まらず failed に "three-way conflict" を出し、全てのファイルが機密ファイルなどのスキップで外れ競合もない場合は "backup store location could not be determined" のエラーで止まる。REQ-backup-018 の「書き込む前にエラーで止まる」が書き込むファイルのない merge に及ぶかを IR は決めていない。変異テストの見逃し（src/cli/merge.rs の execute_merge で、全てのファイルが競合で外れたときの早期の戻りの条件の否定を消すもの）はこの違いでしか検知できない。
  - why: 利用者の判断。書き込むファイルがないときに止まるかは仕様の判断で、テスト整理の中では決めない。
  - decided_by: user (took the recommendation)
