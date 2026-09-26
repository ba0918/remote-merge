# セッション ID の要件を性質として検証する

## Context

バックアップと rollback の取り込みで、セッション ID の形式と順序（REQ-backup-022）と重複しないこと（REQ-backup-023）は、現状追認のため verification を unit として IR に入れた。
その後に定めたテスト手法の選び方（[A2](./2026-09-27-test-method-selection.md#A2)、[A12](./2026-09-27-test-method-selection.md#A12)）では、全ての組で成り立つ順序と重複しない性質は property とする。
backup トピックのテスト整理を始める前に、この二つの要件の verification を選び方に合わせる。

## Agreements

- A1 REQ-backup-022 の verification を property にし、セッション ID の形式と、日時の順・同じ日時なら N を数値として比べた順の並びを、全ての ID の組について proptest で確かめる。
  - why: 並び順は全ての ID の組で成り立つべき性質で、選び方の A12 がこの並び順を property の例として挙げている。
  - decided_by: user (took the recommendation)
- A2 REQ-backup-023 の verification を property にし、同じ秒や並行に作られたセッション ID が集約先全体で重複しないことを proptest で確かめる。
  - why: 重複しないことは全ての作成の組で成り立つべき性質で、選び方の A2 が property の例として挙げている。
  - decided_by: user (took the recommendation)
- A3 既存の具体例のテスト（"-10" が "-9" の後ろに並ぶ、同じ秒の二回の作成で ID が異なる）は、proptest の性質テストと併せて残す。
  - why: 過去に確認した具体的な境界を、性質テストの入力生成に依存せず毎回確かめ続ける。
  - decided_by: user (took the recommendation)
