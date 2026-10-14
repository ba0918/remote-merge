# ローカルhelperとパス検査の抽出

## Context

engineにはローカル走査・基本操作・保存処理と共有型が所属している。
今回は製品runtimeの自由関数とローカルパス検査だけを移し、既存adapterと混在するテストは移さない。
呼び出し側が以下の範囲を確定し、実装後は独立担当がレビューと記録を行う。

## Agreements

- A1 利用者は振る舞い・公開API・テスト期待値・依存版とfeature・製品版0.2.13と通信版v4を維持する移行の設計、継続的実装・独立レビュー・ローカル統合と後片付けを確認なしで進めるよう委任した。今回の実装者は確定した範囲を実装し、新しい振る舞い・IR・テストを追加せず、独立検査と統合は呼び出し側に引き渡す。
  - why: 所属変更とローカル操作の意味の変更を混ぜず、実装と独立検査の責任を分ける。
  - decided_by: user

## Delegated

- D1 local_ioを既存engineに置き、side_ioの十名・十一自由関数定義とTargetPath・ローカル検査・二つのprivate helperを採用する。型の層は既存core FileNodeとprotocol FileHashResult、I/Oと検査の層は既存全実装、互換性の層はRust標準のprivate再公開と既存adapterへの委譲を使う。
  - why: 既存の実装と型で全層が足り、新しい算法やtrait・registry・wrapperは不要である。protocolだけを直接path依存へ追加し、SSH/Agent/TUIへの依存を作らない。
  - decided_by: caller（A1の設計委任に基づく確定設計）
- D2 十一自由関数のpub(crate)をpubにし、製品は同じ十名をpub(crate)で再公開する。TargetPathも下位だけpubにし、製品内の再公開に保つ。二つの検査helperは下位でもprivateにし、全116混在テスト・51CoreRuntimeテストとadapter構成は製品側に置く。
  - why: crateをまたいで同じ実体を使うための可視性だけを広げ、製品公開APIや新しいTargetIo抽象化を作らない。
  - decided_by: caller（型の境界とテスト所有先を確定した設計）
- D3 自由関数の本文・docs・cfgは十一可視性とtruncationの旧warn target明記だけを変える。検査本文はself.rootをrootへ、二つのSelf helper参照をprivate自由関数へ変えるだけとし、旧lexical validationを強化しない。二helperは元の四space indentだけを除き、Self参照だけを変える。fmtが必要な変更は新しいimport/delegateと移したsignatureに限り、個別hunkを保存して逆変換する。移動後に製品のPathとFileHashResultの外側importはlib/test双方で未使用になるため除き、テストだけが使うcompute_local_file_hashとextract_hash_stringの再公開はcfg(test)にする。
  - why: statの秒精度・欠損None、hashのError skip、symlink処理順序、non-Unixの挙動、エラーと旧ログfilterを維持する。全3109テストのowner・名前・cfg・期待値を変えず、RemoteTargetIoやdelegate macroを移動に巻き込まない。
  - decided_by: caller（承認された機械変換と検証方法）
- D4 比較元はe734aefa09f36e4a660b5137f9720b3ee659d6d2に固定し、現在の全比較を実装者と独立担当が別々に記録する。既存順序付き検査・配布比較と一時的な旧producer比較を行い、新IR・恒久oracle・変異gateを作らない。同等変異一覧は今回の対象を持たず変更しない。
  - why: 既存の製品境界を使って所属変更を照合し、旧producer再現を全旧CLI実行や独立レビューの代わりに扱わない。
  - decided_by: caller（確定した比較・記録・検査手順）
- D5 残るローカルadapter・hunk・handler・rollback policyは今回変更しない。他runtime/CLI/app/service、既存engine・他crate、root/製品manifest、契約テスト・IR・README・scripts/hooks/CIは変更しない。
  - why: 自由関数と検査の所属変更に別の構成や安全性の判断を混ぜない。
  - decided_by: caller（閉じた実装範囲）
