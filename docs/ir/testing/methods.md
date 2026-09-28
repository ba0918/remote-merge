# 要件の性質に応じた検証手法

要件の verification の選び方、性質ベーステストと形式検証の使いどころ、変異テストによる根拠テストの審査と削除の安全確認。

## Requirements

### REQ-testing-009: verification を要件の性質で選ぶ
- kind: invariant
- source: docs/decision/records/2026-09-27-test-method-selection.md#A2, docs/decision/records/2026-09-27-test-method-selection.md#A6, docs/decision/records/2026-09-27-test-method-selection.md#A12, docs/decision/records/2026-09-27-test-method-selection.md#A13
- verification: review
- how_to_verify: トピックごとのテスト整理の完了時に、そのトピックの各要件について、テストの有無ではなく要件の性質から見て verification が選び方に合うことを人が読んで確かめる。

要件の verification はテストの有無ではなく要件の性質で選び、具体的な場面で結果が決まる挙動は unit、入力の全体で成り立つべき性質は property、網羅的な証明が必要なものは proof、文書や運用手順は review とする。既存要件はトピックごとのテスト整理のたびにそのトピックの分だけ見直す。

### REQ-testing-010: 性質は proptest で検査範囲に置く
- kind: invariant
- source: docs/decision/records/2026-09-27-test-method-selection.md#A3, docs/decision/records/2026-09-27-test-method-selection.md#A7, docs/decision/records/2026-09-27-test-method-selection.md#A10, docs/decision/records/2026-09-27-test-method-selection.md#A13
- verification: review
- how_to_verify: トピックごとのテスト整理の完了時に、property の各要件のテストが proptest を使って "tests/contract/" の下にあり公開された関数を通して確かめていること、proptest-regressions/ 配下の失敗入力のファイルがコミットされていることを人が確かめる。

property の要件は proptest を使い、公開された関数を通して "tests/contract/" の下で検証し、一つの性質で試す入力の数は proptest の既定の 256 件とし、proptest が書き出す失敗入力のファイルをコミットする。

### REQ-testing-011: 形式検証は求められたときだけ使う
- kind: prohibition
- source: docs/decision/records/2026-09-27-test-method-selection.md#A4
- verification: review
- how_to_verify: verification が proof の要件ごとに、利用者がその要件について形式検証を明示的に求めた決定が決定記録にあることを人が確かめる。

利用者がその要件について明示的に求めた場合を除き、要件の verification に proof を使わない。

### REQ-testing-012: 変異テストで根拠と削除を審査する
- kind: invariant
- source: docs/decision/records/2026-09-27-test-method-selection.md#A5, docs/decision/records/2026-09-27-test-method-selection.md#A8, docs/decision/records/2026-09-27-test-method-selection.md#A13, docs/decision/records/2026-09-28-mutation-scope.md#A1, docs/decision/records/2026-09-28-mutation-scope.md#A2
- verification: review
- how_to_verify: トピックごとのテスト整理の最後に（テストを削除する整理では最初にも）そのトピックのファイルか、その中の決着の対象の関数で変異テストを実行した結果があり、全ての見逃しがテストの追加、理由付きの同等変異の登録、FLAG のどれかで決着していることを人が確かめる。

トピックごとのテスト整理の最後に、そのトピックのファイルに絞って変異テストを実行し、テストを削除する整理では最初にも実行する。実行は見逃しを決着させる対象の関数に絞ってよく、対象外の関数の見逃しと、最初の実行を省いたときの比べる相手には、そのファイルを前に回したときの記録を使う。見逃しは一件ごとにテストを足すか、別の文脈で落とすテストを書けなかった同等変異として ".kotowari/mutants-equivalents.yaml" に理由付きで登録するか、不具合の疑いを FLAG に記録する。変異テストはフックと CI では実行しない。

### REQ-testing-013: 変異テストをメモリ上限の中で実行する
- kind: state_driven
- source: docs/decision/records/2026-09-27-test-method-selection.md#A9, docs/decision/records/2026-09-27-test-method-selection.md#A14, docs/decision/records/2026-09-27-test-method-selection.md#A15, docs/decision/records/2026-09-28-mutation-scope.md#A3
- verification: review
- how_to_verify: MemoryMax を小さく上書きして "scripts/mutants.sh" を実行し、0 以外で終了して kotowari mutants の結果を出さないことを確かめ、既定の設定では cargo-mutants が systemd-run --user のサービスとして MemoryHigh 35%、MemoryMax 40%、MemorySwapMax 0、並列数 2 で起動されることを人がスクリプトと実行ログで確かめる。

"scripts/mutants.sh" は渡されたファイルの変異テストを、ファイルの前に "--re" と正規表現を渡したときは変異の名前がその正規表現に一致するものに絞って、毎回新しく実行して kotowari mutants で読み、cargo-mutants を systemd-run --user のサービスとして既定で MemoryHigh 35%、MemoryMax 40%、MemorySwapMax 0、並列数 2 で起動し、各値は環境変数で上書きできる。メモリ上限でプロセスが強制終了された実行は 0 以外で終了し、その結果を読まない。

## Examples

```gherkin
@id=EX-testing-012 @about=REQ-testing-009 @source=docs/decision/records/2026-09-27-test-method-selection.md#A12
Scenario: 全ての組で成り立つ順序
Given セッション ID の並び順が全ての ID の組で成り立つべき順序である
When その要件の verification を選ぶ
Then property が選ばれ具体例一件のテストだけでは根拠にならない

@id=EX-testing-013 @about=REQ-testing-009 @source=docs/decision/records/2026-09-27-test-method-selection.md#A2,docs/decision/records/2026-09-27-test-method-selection.md#A12
Scenario: 場面ごとに決まる終了コード
Given rollback の終了コードが場面ごとに結果が決まる挙動である
When その要件の verification を選ぶ
Then unit が選ばれテストがないことを理由に review にはならない

@id=EX-testing-014 @about=REQ-testing-010 @source=docs/decision/records/2026-09-27-test-method-selection.md#A10,docs/decision/records/2026-09-27-test-method-selection.md#A13
Scenario: src の中だけにある性質のテスト
Given property の要件のテストが "src/" の単体テストにだけある
When テスト整理の完了を確かめる
Then 検査範囲外のため根拠として数えられず完了とされない

@id=EX-testing-015 @about=REQ-testing-011 @source=docs/decision/records/2026-09-27-test-method-selection.md#A4
Scenario: 求められていない証明
Given 利用者が形式検証を求めていない要件がある
When その要件の verification を選ぶ
Then proof は選ばれない

@id=EX-testing-016 @about=REQ-testing-012 @source=docs/decision/records/2026-09-27-test-method-selection.md#A8,docs/decision/records/2026-09-27-test-method-selection.md#A13
Scenario: 見逃しを理由なく残す
Given 変異テストで見逃しが一件残っている
When テスト整理の完了を確かめる
Then テストの追加か理由付きの同等変異の登録か FLAG の記録がない限り完了とされない

@id=EX-testing-017 @about=REQ-testing-012 @source=docs/decision/records/2026-09-27-test-method-selection.md#A5
Scenario: 削除後に見逃しが増える
Given テスト整理の最初の変異テストの結果がある
When テストを削除した後の変異テストで見逃しが増える
Then その削除は安全と確認されない

@id=EX-testing-018 @about=REQ-testing-013 @source=docs/decision/records/2026-09-27-test-method-selection.md#A15
Scenario: メモリ上限でテストが強制終了される
Given 変異テストの実行中にメモリ上限でプロセスが強制終了される
When "scripts/mutants.sh" が終わる
Then 0 以外で終了し kotowari mutants の結果を出さない

@id=EX-testing-019 @about=REQ-testing-013 @source=docs/decision/records/2026-09-27-test-method-selection.md#A9,docs/decision/records/2026-09-27-test-method-selection.md#A14
Scenario: 上限の中で最後まで実行する
Given メモリ上限に達しない
When "scripts/mutants.sh" に対象ファイルを渡す
Then その実行で新しく作った cargo-mutants の結果が kotowari mutants で読まれる
```
