# マージ準備の所属変更

## Context

製品のmerge_flowにはI/Oを伴わない検査とテキスト準備が残っている。
handlerの実行前判定とmtime判定も副作用を持たない。
確定設計に従い、既存本文をengineへ移し、製品の公開パスとI/O順序を保つ。

## Agreements

- A1 利用者は振る舞いを維持する移行の設計・実装・独立レビュー・ローカル統合・後片付けを委任した。今回の実装担当は固定baseから閉じた抽出を行い、独立レビューと統合を親担当へ引き渡す。
  - why: 機能・bug修正・GUI・テスト方法・IR変更を所属変更に混ぜない。push・release・global設定変更は許可されていない。
  - decided_by: user

## Delegated

- D1 check_source_existsとvalidate_hunk_merge_targetの既存本文をengineのservice/merge_flowへ移し、旧製品パスからpublicに再公開する。execute_hunk_mergeの純粋部分をprepare_hunk_mergeへ移し、Equalまたはmerged_textと既存HunkMergeInfoを持つModifiedで返す。
  - why: 検査・diff helper・結果型は既存実装を採用できる。新しいtrait・registry・DTO・依存を追加する必要がない。
  - decided_by: caller（承認済みの確定設計）
- D2 製品のvalidator先行、両側read、prepare、dry-run、backup、writeの順を保つ。binary先行、lossy変換、Equal先行、diff operand順、表示hunk番号、元index順の検証、適用だけの重複排除、報告の重複保持、全選択のsource本文、部分選択のtarget末尾改行と既存の>=比較を変えない。Equal/Binary/SymlinkDiff/Modifiedを網羅する。
  - why: 境界を結果wrapperへ変えるだけで、アルゴリズムとエラー文字列を置き換えない。
  - decided_by: caller（承認済みの本文境界と意味の維持）
- D3 merge_exec_logic全体をengineのmerge/executionへ、merge_mtime_logic全体をmerge/mtimeへ移す。十八と十二の既存単体テストは本文・期待値・名前のまま移し、八つの公開項目を製品facadeから明示的に再公開する。未使用の公開関数も残し、新しい呼び出しは追加しない。
  - why: 優先順、型identity、左側先行の衝突収集と右側存在gateを保つ。製品merge_flowの十八テストと全contract/TUIテストは変更しない。
  - decided_by: caller（承認済みの三十テスト所有先移動）
- D4 比較元を7bfb6d286f70fe897175d41bf60b3668c6012a51に固定する。八sourceとPROJECTの所属説明、本決定、現行変更記録だけを変更対象とする。既存3111 identityは三十のowner/module移動以外保つ。既存manifest・feature・製品0.2.13・通信v4・install方法を維持する。
  - why: 文字列とcommentを識別するbrace parserで本文を照合し、実際のshell baselineと候補の全テストを比較できる範囲に閉じる。新しい振る舞いを導入しないため、架空のREDや新しいpolicyテストを作らない。
  - decided_by: caller（確定したsource capと検証方法）
- D5 古いimplementation/review記録を退役し、実装担当は固定baseに対するimplementation.yamlだけを作る。独立レビューとreview.yaml、統合前のreview-phase照合は親担当へ引き渡す。予期しないテスト失敗時は再試行や環境変更をせず停止する。
  - why: 実装者の検査を独立承認と混同しない。通常hook、限定runner、数値exitと失敗履歴を維持し、未検証platformを成功扱いしない。
  - decided_by: caller（実装と独立レビューの分離）
