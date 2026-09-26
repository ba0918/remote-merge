# 要件の性質に応じたテスト手法の選び方

## Context

既存テストは以前の「テスト必須」方針で増え、要件を確かめているかが分からないものが多い。バックアップと rollback の取り込みで、根拠テストが構造体の等価だけを見て保存先のキーを確かめていないこと（変異 42 件中 6 件の見逃し）が変異テストで見つかった。
一方で、シェル引数のエスケープや一括読み込みの往復のように、入力の全体で成り立つべき性質を具体例だけで確かめている箇所がある。
要件ごとに検証手法（単体・結合テスト、性質ベーステスト、形式検証、人による確認）を選ぶ基準と、変異テストの使い方を、トピックごとのテスト整理を始める前に決める。

Position: 全ラウンド終了（A1〜A15、U1、D1）。IR を docs/ir/testing/methods.md に作成し承認へ。

## Agreements

- A1 検証手法の選び方は `docs/ir/testing/` の新しい IR 文書に、verification を review とする要件として置く。
  - why: 既存のテスト環境の方針（REQ-testing-001〜008）と同じ形にし、kotowari check と後続の工程が IR として読めるようにする。
  - decided_by: user (took the recommendation)
- A2 要件の verification はテストの有無ではなく要件の性質で選ぶ。具体的な場面で結果が決まる挙動は unit、入力の全体で成り立つべき性質（往復で元に戻る、並び順、重複しない、root の外に出ない）は property、網羅的な証明が必要なものは proof、文書や運用手順は review とする。
  - why: テストの無い要件を review にしたり、全体で成り立つべき性質を少数の具体例だけで確かめたりすることを防ぐ。
  - decided_by: user (took the recommendation)
- A3 property の要件は proptest で検証し、proptest は最初の property 要件のテストを書くときに dev-dependency に加える。
  - why: Rust で広く使われ、失敗した入力を最小まで絞り込め、パスや文字列の組み合わせの生成器を細かく書ける。
  - rejected: quickcheck は生成器を細かく書きにくい。
  - decided_by: user (took the recommendation)
- A4 形式検証（Kani）は導入せず、proof は利用者がその要件について明示的に求めたときだけ使う。
  - why: このプロジェクトはファイル操作・SSH・文字列とパスの処理が中心で、範囲を区切った整数や配列のロジックが少なく、費用に見合う対象が乏しい。
  - decided_by: user (took the recommendation)
- A5 変異テストは要件の検証手法にせず、根拠テストが十分かの判定とテスト削除の安全確認に使う。トピックごとのテスト整理の最初と最後に、そのトピックのファイルに絞って実行し、フックと CI には入れない。テストを削除した後の実行で見逃しが増えたら、その削除は安全と確認されない。
  - why: 全テストが一回 30〜45 秒かかり一トピックで 30 分前後になるため常時実行には重く、審査の場面で使えば弱い根拠テストと不要なテストを見分けられる。
  - decided_by: user (took the recommendation)
- A6 既存要件の verification は、トピックごとのテスト整理のたびにそのトピックの要件だけ A2 の基準で見直し、IR 全体を一度に書き換えない。
  - why: テストの審査と同じ単位で見直せば、見直しと根拠テストの整備を一度に済ませられる。
  - decided_by: user (took the recommendation)
- A7 proptest が書き出す失敗入力のファイル（proptest-regressions/ 配下）はコミットし、一つの性質で試す入力の数は proptest の既定の 256 件とする。
  - why: 一度見つかった失敗を誰の環境でも毎回再現させる。
  - decided_by: user (took the recommendation)
- A8 変異テストの見逃しは一件ごとに、要件を確かめるテストを足すか、観測できる違いを生まない同等変異として理由付きで同等変異の一覧に登録するか、不具合の疑いを FLAG に記録するかのどれかに決める。同等変異の一覧は ".kotowari/mutants-equivalents.yaml" に置いて設定の mutants.equivalents から参照し、同等と判断するのは別の文脈でその変異を落とすテストを書こうとして書けなかったときだけとする。
  - why: 見逃しを放置も黙認もせず、同等と判断した理由を後から検証できるように残す。
  - decided_by: user (took the recommendation)
- A9 変異テストは "scripts/mutants.sh" に対象ファイルを渡して実行し、スクリプトは毎回 cargo-mutants の結果を新しく作ってそのまま kotowari mutants で読む。
  - why: 古い結果を読む取り違えをなくし、手順を一つのコマンドにまとめる。
  - decided_by: user (took the recommendation)
- A10 property の要件のテストは tests.files の範囲（tests/contract/ の下）に置き、公開された関数を通して確かめる。対象が公開されていなければ、公開するか公開された入口から確かめるかをその都度決める。
  - why: src/ 内の単体テストは kotowari の検査範囲外で、印を付けても根拠として数えられない。
  - decided_by: user (took the recommendation)
- A11 "scripts/mutants.sh" と同等変異の一覧は、この方針の承認とは別の作業として、最初に変異テストを使うトピックのテスト整理の開始時に作る。
  - why: この方針は選び方の基準を定めるもので、道具は最初に使う作業で作れば足りる。
  - decided_by: user (took the recommendation)
- A12 セッション ID の並び順のように全ての入力の組で成り立つ順序は property、rollback の終了コードのように場面ごとに結果が決まる挙動は unit、集約先のディレクトリ権限のように有限個の経路の結果で確かめられるものは unit とする。
  - why: 選び方の境界を具体例で示し、一件の具体例だけで全体の性質を確かめたことにしない。
  - decided_by: user (took the recommendation)
- A13 選び方の要件は review とし、トピックごとのテスト整理の完了時に、各要件の verification が選び方に合うこと、変異テストの見逃しが全てテストの追加・同等変異の登録・FLAG のどれかで決着していること、property の要件のテストが proptest で tests/contract/ の下にあることを人が確かめる。
  - why: 方針の遵守はコードの挙動ではなく、テスト整理の成果物を読んで確かめるものである。
  - decided_by: user (took the recommendation)
- A14 "scripts/mutants.sh" は cargo-mutants を systemd-run --user のサービスとして起動し、既定で MemoryHigh を WSL のメモリの 35%、MemoryMax を 40%、MemorySwapMax を 0、並列数を 2 とし、それぞれ環境変数で上書きできるようにする。
  - why: 変異テストの実行中にメモリを使い切って WSL ごと落ちることが何度か起きており、柔らかい上限で抑えつつ硬い上限で環境全体の停止を防ぐ。割合で指定して WSL のメモリ割り当ての変更に追従させ、並列数はメモリを測っていないため控えめにする。
  - decided_by: user (took the recommendation)
- A15 メモリ上限でプロセスが強制終了された変異テストの実行は失敗として 0 以外で終了し、kotowari mutants で読まず、途中までの結果も使わない。
  - why: 強制終了されたテストを cargo-mutants は変異の検知と数えるため、その結果で根拠テストを十分と誤判定しないようにする。
  - decided_by: user (took the recommendation)

## Delegated

- D1 変異テストのテストのタイムアウトは cargo-mutants の自動設定に任せる。
  - why: タイムアウトした変異は kotowari mutants が notice として示し、見逃しなしとは扱われないため、値によって審査の結論は変わらない。

## Undecided

- U1 通常テストやフックなど、変異テスト以外の重いコマンドをメモリ上限で囲むか。
  - decides: 利用者（WSL が落ちたときの状況を調べてから）
