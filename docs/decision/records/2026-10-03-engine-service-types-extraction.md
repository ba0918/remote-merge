# 共有Sideとservice基礎処理の抽出

## Context

engineにはローカル走査・バックアップ識別とmergeの基本操作が既に所属している。
今回は共有Sideと閉じた五つのserviceモジュールを既存152テストと共に移す。
呼び出し側が以下の機械的抽出の範囲を確定し、実装後は独立担当がレビューと記録を行う。

## Agreements

- A1 利用者は振る舞い・公開API・依存版とfeature・製品版0.2.13と通信版v4を維持する移行の設計、継続的実装・独立レビュー・ローカル統合と後片付けを確認なしで進めるよう委任した。今回の実装者は確定した範囲を実装し、新しい振る舞い・IR・テストを追加せず、独立レビューと統合は呼び出し側に引き渡す。
  - why: 所有先の変更を製品の意味の変更と混ぜず、実装と独立した検査の責任を分ける。
  - decided_by: user

## Delegated

- D1 共有Sideとservice/types・max_files・path_resolver・fast_path・diffを既存engineへ全module単位で移す。再利用の各層はSideのstdだけの既存実装、既存DTOとserde契約、四つの既存アルゴリズムとcore型、Rust標準の再公開を採用する。serdeは製品と同じderive付き通常依存、serde_jsonは製品と同じ版指定のdev依存として既存JSONテストだけに使う。
  - why: コードベース内の実装で全層が足り、新しい型・互換shim・helper橋渡しやserialization実装を作る必要がない。serdeの既存tag・flatten・rename・skip・defaultが固定するJSON表現をそのまま維持する。
  - decided_by: caller（A1の設計委任に基づく確定設計）
- D2 Sideはengineのside moduleに同じ実体とinherent methodsを置き、製品app::Sideとapp::sideの旧パスを再公開で維持する。serviceの五つの旧パスも同じ実体を再公開し、製品app/mod.rs・service/mod.rsと消費側は変更しない。engineにapp namespace・UI・runtimeやI/Oの依存を追加しない。
  - why: nominal型と既存公開APIを保ち、共有の比較対象を表示層の名前空間へ結び付けない。DTO内のConflictRegionも既存coreの実体を参照する。
  - decided_by: caller（公開範囲と責務を確定した設計）
- D3 六つの全source blobとmodeを一切変更せず、private helpersと152既存テストを一緒に移す。他2957テストは同一で、Sideの15件だけapp::side::testsからside::testsへの接頭辞変更を認める。残る137件はownerだけを移し、名前・本文・cfg・属性は維持する。
  - why: 型の実体、serde表現とアルゴリズムを変えずに所属だけを変えたことを全byte比較と全3109件の収集結果で確かめる。既存の同等変異一覧には今回の対象がなく、変更しない。
  - decided_by: caller（A1の具体化と検証方法）
- D4 比較元は32db611ad1485f3e6b5bacdd7305daed2e8b9ce4に固定する。旧両記録を除いて現在の全比較を実装者と独立レビュー担当が別々に記録する。既存の順序付き検査、API/本文比較、配布前後比較を行い、変異実行は必須gateとせず新しいoracleやlayout testを作らない。
  - why: 直前の基礎処理の記録を流用せず、今回の移動を既存の製品契約で検査する。実装側の成功だけで独立レビューと統合のgateを通したことにしない。
  - decided_by: caller（確定した比較・記録・検査手順）
- D5 残るclassification・merge/sync/rollback・storage・hunkの移行は今回行わない。他のservice、app・CLI・runtime・handler、既存engine基礎実装、他crate・rootと製品manifest、契約テスト・IR・README・scripts/hooks/CIは変更しない。
  - why: 閉じた共有型と五つのserviceモジュールの所属変更に、実行処理やI/Oの判断を混ぜない。
  - decided_by: caller（閉じた実装範囲）
