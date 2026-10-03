# Serviceの判断処理と三者比較の抽出

## Context

engineには共有Side・入出力型とserviceの五つの基礎モジュールが所属している。
今回はstatus・merge・sync・rollbackの純粋な判断処理と三者比較を移す。
呼び出し側が以下の範囲を確定し、実装後は独立担当がレビューと記録を行う。

## Agreements

- A1 利用者は振る舞い・公開API・テスト期待値・依存版とfeature・製品版0.2.13と通信版v4を維持する移行の設計、継続的実装・独立レビュー・ローカル統合と後片付けを確認なしで進めるよう委任した。今回の実装者は確定した範囲を実装し、新しい振る舞い・IR・テストを追加せず、独立検査と統合は呼び出し側に引き渡す。
  - why: 所属変更を製品の意味の変更と混ぜず、実装と独立検査の責任を分ける。
  - decided_by: user

## Delegated

- D1 service/status・merge・sync・rollbackを既存125テストとprivate helpersごとengineへ移す。判断処理の層は既存四モジュール、比較の層は旧二関数の本文、表示の層は旧badgeとpalette実装、パターン照合の層は既存glob-matchを採用する。既存Side・core・backup・DTOの同じ型を使う。
  - why: コードベース内の実装で全層が足り、新しいtrait・wrapper・コピーされたhelperや実行依存は不要である。
  - decided_by: caller（A1の設計委任に基づく確定設計）
- D2 下位three_wayはFileComparisonの四variantとLineComparisonの二variantだけを持ち、旧classifier本文はenum名だけを変えて使う。製品の二関数は同じsignatureで下位を呼び、全variantを旧badgeへ明示的に写す。製品badge型・label/style・docsとConflict variantはそのまま保つ。
  - why: 比較結果とpaletteを分離し、下位にapp namespaceやUI依存を置かない。旧行算法はConflictを返さないため下位にも追加せず、Conflict検出という新しい意味を持ち込まない。
  - decided_by: caller（型の所有先と変換を確定した設計）
- D3 製品の四service公開パスは同じ項目を再公開し、125件のownerだけを変更する。他2984件と製品badgeの20テストはそのまま保つ。sync/rollback全blobとmodeは同一、mergeは二import、statusはclassifier参照・識別子と対応doc参照だけを変える。glob-matchは元の製品と同じ指定でengine通常依存へ追加し、他の依存を変えない。
  - why: 敏感なファイルの扱い・文字列・順序・エラーを変えずに所属だけを移したことを全文比較と既存テストで確かめる。consumer側やserialization契約を変更しない。
  - decided_by: caller（A1の具体化と検証方法）
- D4 比較元は8863e28545e5d21f028431cd7b4a581c604639c3に固定する。旧両記録を除き、現在の全比較を実装者と独立担当が別々に記録する。既存検査と配布比較に加え、旧関数本文の明示的再現と全32bool組合せ・行内容組合せを一時probeで照合する。新しい恒久oracleや変異gateは作らない。
  - why: 直前の記録を流用せず、型を分離した変換の同一性を確認する。旧関数再現は全旧CLIの実行ではなく、実装側の成功も独立レビューの代わりにはならない。
  - decided_by: caller（確定した比較・記録・検査手順）
- D5 storage・ローカル操作・hunk・handlerの判断処理の移行は今回行わない。他のapp/CLI/runtime/handler、既存engine基礎と共有型、他crate・root/製品manifest、同等変異一覧、契約テスト・IR・README・scripts/hooks/CIは変更しない。
  - why: 四つのservice判断と二つのclassifierの所属変更に、I/Oや別の責務の判断を混ぜない。
  - decided_by: caller（閉じた実装範囲）
