# バックアップ保存処理の内部crateへの抽出

## Context

engineにはバックアップ識別・共有Side・設定参照とserviceの入出力型が既に所属している。
今回は既存backup_store全moduleの所有先だけを移す。
呼び出し側は以下の範囲と、初回停止で確認された二つのfmt変更を確定した。

## Agreements

- A1 利用者は振る舞い・公開API・テスト期待値・依存版とfeature・製品版0.2.13と通信版v4を維持する移行の設計、継続的実装・独立レビュー・ローカル統合と後片付けを確認なしで進めるよう委任した。今回の実装者は確定した範囲を実装し、新しい振る舞い・IR・テストを追加せず、独立検査と統合は呼び出し側に引き渡す。
  - why: 所属変更と保存処理の意味の変更を混ぜず、実装と独立検査の責任を分ける。
  - decided_by: user

## Delegated

- D1 backup_storeの全moduleを既存engineへ移す。保存処理の層は既存全実装、識別と設定・DTOの層は既存のSide・AppConfig・backup/service型、互換性の層はRust標準のcrate-private再公開を採用する。AppConfigはconfig crateから直接参照し、engineのprivate config別名を広げない。
  - why: コードベース内の同じ実装で全層が足り、型やvalidatorのコピー、wrapperや新しい抽象化は不要である。
  - decided_by: caller（A1の設計委任に基づく確定設計）
- D2 下位の三つの型と八つのmethodだけをpubにし、製品はBackupStore・BackupRecord・SymlinkBackupをpub(crate)で再公開する。runtimeのprivate module宣言と全consumerは変えない。保存形式のprivate enum・private fields・helperはそのまま保つ。
  - why: crate間で同じ実体を使うための十一の可視性だけを広げ、製品の公開APIや下位の保存表現を余計に公開しない。
  - decided_by: caller（必要な可視性と製品境界を確定した設計）
- D3 SideとAppConfigの二import、十一pub(crate)のpub化だけを変える。加えてSide importを既存backup importの直後へ移すことと、pub fn new(root: Option<PathBuf>, startup_directory: PathBuf, now: DateTime<Utc>) -> Self {へのsignature折り畳みだけを認める。これらを個別に逆変換して全元blobとmodeを照合する。全3109テストのowner・名前・期待値を維持し、CoreRuntime/SideIOと既存contractsは製品側に残す。
  - why: 予約・遅延初期化エラー・retention・target hash・raw path hash・JSON tag/default/省略・一時書き込みとcleanup・Unix権限・一覧順序と不完全entryの扱いを変えずに所有先だけを移す。追加二点は初回rustfmtの実際の要求への対応であり、他のwhitespace正規化を認めない。
  - decided_by: caller（初回停止後の再開指示で二つのfmt変更を明示承認）
- D4 sha2は元の製品と同じ指定の通常依存を採用し、既存serde_jsonは同じ版のままdevから通常依存へ移す。同等変異一覧はstorage entryのfile値だけを新ownerへ変える。比較元はe042e7b2fea675ff807a3180730138ea507b7c68に固定し、実装者と独立担当が現在の全比較を別々に記録する。既存検査と一時producer比較で照合し、新IR・恒久oracle・変異実行gateを作らない。
  - why: 既存保存形式を同じserde/JSON実装で引き継ぎ、外部版やfeatureを変えず実際のproducerだけに依存を置く。初回成功baselineは同じclean baseで再利用し、初回fmt失敗を成功扱いせず保存する。
  - decided_by: caller（確定した依存・比較・検査手順と再開指示）
- D5 ローカル操作・hunk・handlerの判断処理は今回移さない。他のruntime/CLI/app/service、既存engineと他crate、root/製品manifest、契約テスト・IR・README・scripts/hooks/CIは変更しない。
  - why: 保存処理の所属変更に別のI/Oや安全性の判断を混ぜない。
  - decided_by: caller（閉じた実装範囲）
