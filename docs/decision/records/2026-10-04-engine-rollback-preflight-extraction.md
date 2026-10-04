# rollback事前検査の所属変更

## Context

製品SideIOの復元処理には、要求全体を拒否する純粋な判定と結果の展開が残っている。
保存レコードから復元判定用の型への変換も製品が持っている。
既存の判定と変換だけをengineへ移し、製品のI/O順序と既存テストを維持する。
この変更は既存不具合の修正や仕様の全面的な適合宣言ではない。

## Agreements

- A1 振る舞いを維持する所属変更だけを行う。実装担当は独立レビューと統合を親担当へ引き渡す。
  - why: 利用者が承認した移行範囲を機能追加・不具合修正・push・release・設定変更へ広げない。
  - decided_by: user

## Delegated

- D1 engineのservice/rollbackにrestore_request_refusalを追加する。既存のBackupPathRecordとCurrentRestorePath、要求のパス列、検査結果または借用したエラー文字列を受け、既存の三結果ベクトルをOptionで返す。検査失敗の全件failure展開と二つの理由だけの全件skip展開を移す。
  - why: 既存本文と型を採用でき、新しいDTO・trait・依存は不要である。順序・重複・文字列とdecide_restore_pathの判定を変えない。
  - decided_by: caller（承認済みの関数境界）
- D2 engineのbackup_storeにFrom<&BackupRecord>を追加し、製品の既存変換matchを移す。Fileのreal_pathをcloneし、二つのフィールドが両方SomeのsymlinkだけをSymlinkUpdateにする。他のsymlinkはSymlinkのまま扱う。製品のprivate wrapperは一行の委譲に保つ。
  - why: 保存形式からドメインへの一方向の変換として配置し、純粋rollback moduleがbackup_storeを参照する逆向きの依存を作らない。保存形式・serde表現・公開フィールドを変えない。
  - decided_by: caller（承認済みの変換境界）
- D3 製品は各パスのrecord readを即時に?で処理し、変換後のplain Symlinkでは検査を省く。それ以外は同じ位置で検査し、成功はCurrentRestorePathの借用、失敗は既存Displayの文字列を下位関数へ渡す。Someなら即時に返す。第二pass、session予約、dry-run、pre-backup、write、finishを変更しない。
  - why: 先行エラー、producer順序と二passの重複I/Oを保つ。検査の一括収集や既存不具合修正を抽出に混ぜない。
  - decided_by: caller（承認済みの順序維持）
- D4 比較元を07c08bec370b9cd9b3b45bb6dece2610650639fdに固定する。engineのservice/rollbackとbackup_store、製品runtime/side_ioの三source、PROJECTの所属説明、本決定と現行変更記録だけを変更する。3111のtest identity・本文・期待値・markを移動せず維持し、依存・feature・製品0.2.13・通信v4・install方法・IRを変えない。
  - why: 全sourceの再構成比較と範囲外のblob/mode比較、baselineと候補の全テスト・metadata比較で検査できる範囲に閉じる。greenからのrefactorであり、架空のREDや新しいテストを作らない。
  - decided_by: caller（確定した変更範囲と検証方法）
- D5 古いimplementation/review記録を退役し、固定baseに対するimplementation.yamlだけを再作成する。独立レビューとreview.yamlは親担当へ引き渡す。通常hookを有効に保ち、予期しないテスト失敗時は再試行や環境変更をせず停止する。
  - why: 実装者の検査を独立承認と混同しない。既存SkipDifferentKindのcaller不具合は範囲外のまま残し、この抽出の検証と製品全体の適合を区別する。
  - decided_by: caller（検証と承認の分離）
