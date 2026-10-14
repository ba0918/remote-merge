# Engine基礎処理の内部crateへの抽出

## Context

SSHとAgentの所有先は既に分離されている。
今回はローカル走査・バックアップ識別・merge実行の基本操作と楽観的ロックを既存81テストと共に移す。
振る舞いを維持する機械的抽出であり、新しいIR・要件・テストや正式な計画は作らない。

Position: 利用者の自律的な移行の委任を受け、呼び出し側が以下の設計と閉じた範囲を確定した。実装者の照合後、独立した担当がレビューと記録を行う。

## Agreements

- A1 振る舞い・公開API・既存依存版とfeature・製品版0.2.13と通信版v4を維持する移行の設計、継続的レビューとローカル統合・後片付けは確認なしに進める委任を受けた。今回の実装者は確定した基礎処理だけを移し、bug/feature変更や新IR・テストを作らない。独立検査と統合は呼び出し側が行う。
  - why: 所属変更を入出力・エラー・パス検証・ロックや走査の意味の変更と混ぜず、実装と独立レビューの責任を分ける。
  - decided_by: user

## Delegated

- D1 remote-merge-engineを内部版0.1.0・edition 2021・publish=falseの第八member/default-memberとし、local・backup・merge/executor・merge/optimistic_lockを元のmodule構造で移す。既存実装をコードベース探索の段階で採用し、core/configの同じ項目をprivate別名で使う。backupはconfigの定数を元の行のまま再公開する。依存は元の製品指定を採用し、SSH/Agent/TUIなど不要な辺を追加しない。
  - why: 閉じた基礎処理を既存の型と標準のRust再公開・Cargo path依存で再利用でき、新しい抽象化やvalidatorコピー、configからの上向き依存は不要である。
  - decided_by: caller（A1の設計委任に基づく確定設計と再利用判断）
- D2 製品のlocal/backup/optimistic_lockは同じ下位公開項目を再公開し、executorは既存八公開項目だけを明示的に再公開する。validate_path_within_rootは下位でpubにして製品からpub(crate)で再公開するが本文を変えない。normalize_pathはpub(crate)のままにし、製品lib/merge modと利用側は変更しない。
  - why: 同じnominal型と旧公開パスを保ち、必要な製品内validator利用だけを維持する。globで新しいvalidatorを製品から公開しない。
  - decided_by: caller（公開範囲と利用者を確定した設計）
- D3 81テストのowner/binaryだけを移し、元の名前・接頭辞・cfg・無視状態と他3028件をそのまま保つ。localの五warnとexecutorの二infoに旧targetを明記し、その七追加と一つの可視性だけを逆変換して全本文とmodeを比較する。同等変異一覧はbackupの二つのfile値だけを新ownerへ変更し、rationaleと他entryを保つ。
  - why: テスト本文と既存ログフィルターを維持し、現在の変異対応先だけを移す。file/line/module_pathはowner変更で変わり得るがtarget変更として扱わない。既存storage entryや過去の検査資料は今回の対応先ではない。
  - decided_by: caller（A1の具体化と検証方法）
- D4 比較元を3c0347b628cf4a62f02bd754e024f821a4855681に固定し、旧両記録を除いて現在の比較を実装者と独立担当が記録する。既存順序付き検査・API/本文比較と配布前後比較を行う。必須の変異検査は設けず、新しいpolicy oracleやlayout testは作らない。
  - why: 前回の移動を今回の根拠とせず、既存のruntime境界と契約を変えない抽出を確かめる。未取得の動的ログや未実行の環境を検証済みと扱わない。
  - decided_by: caller（PROJECT.mdの照合と検証方法）
- D5 残るservice・storage・hunkなどのengine移行は今回行わず、製品app/service/handler/runtime/main、他の下位crate、契約とfixtures、IR/README/scripts/hooks/CIは変更しない。
  - why: 四つの基礎モジュールを移す範囲を超える責務や安全性の判断を混ぜない。
  - decided_by: caller（閉じた実装範囲）
