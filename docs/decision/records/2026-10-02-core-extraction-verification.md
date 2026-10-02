# core抽出の検証手順

## Context

[core抽出の決定](./2026-10-02-core-extraction.md#Agreements)が承認・コミットされた。
型の再公開、テストの所属変更、変異検査の選択が互いに影響する。
製品の内容を変えずに同じ検査を実行できることを、抽出前後の比較で確認する。

Position: 計画の具体化を記録した。計画とこの記録を同時に承認する草稿であり、実装は未着手。

## Delegated

- D1 比較元はcore抽出の決定をコミットしたmainの7d7b13eの完全なコミットIDとする。両役割の変更記録は今回の比較だけで作成し直す。
  - why: 前回の配置変更の記録で今回の抽出をカバーしたことにせず、計画以降の全変更を独立した担当と照合する。
  - decided_by: planner（PROJECT.mdのChange conformanceに従う）
- D2 抽出前後のテスト名と無視状態の集合を比較する際は、移動した170単体テストだけのpackage・library target・test binaryの所属をremote_mergeからremote_merge_coreへ対応付ける。各テスト名と無視状態、移動対象外のpackageとtargetの対応は変えない。全ての製品モジュール、テスト、公開項目の内容を維持し、lib.rsの宣言から再公開への差分だけを許可する。
  - why: 同じ総件数だけではテストの脱落を判断できず、逆に所属crateの変更は今回の目的であるため、許可した差分と検査漏れを区別する必要がある。
  - decided_by: planner（A1・A3・A5の具体化）
- D3 scripts/mutants.shは両crateの単体テストと従来の製品contract・cli_diffを同じ実行で選択する。必要なworkspace・package選択だけを補正し、既存の資源制限・隔離・判定・テスト選択は緩めない。
  - why: coreだけを変異させた際にも、coreの単体テストと型を利用する製品の契約テストが検証に参加する必要がある。
  - decided_by: planner（A3・A5の具体化）
- D4 限定した変異検査はdiff/engine.rsのapply_selected_hunks_single_passにある既存の同等変異2件を対象とし、抽出前後で同じ変異を列挙・実行する。同等変異2件のパスだけを更新し、既存の判定や根拠は再審査しない。
  - why: この関数は移動対象であり、既存の同等変異の記録もあるため、新しいcrateでの対象選択と結果の照合を確認できる。全変異は変更していない製品ロジックの再審査になるため回さない。
  - decided_by: planner（A1・A3の検証範囲の具体化）
