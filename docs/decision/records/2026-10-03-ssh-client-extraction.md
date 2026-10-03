# SSH接続実装と内部サポートの所属変更

## Context

SSHのパースとサポート処理は既に内部crateに所属している。
残るclient・known_hosts・known_hosts_io・preferredの閉じた依存を同じcrateへ移し、製品の公開パスと振る舞いを保つ。
機械的移動として扱い、正式な計画や新しいIRは作らない。

Position: 利用者の自律的な移行の委任を受け、呼び出し側が以下の設計と閉じた範囲を確定した。実装者の照合後、独立した担当がレビューと記録を行う。

## Agreements

- A1 振る舞い・公開パス・既存依存の版とfeature・製品版0.2.13・通信版v4を保つ移行の設計、継続的レビューとローカル統合・後片付けは確認なしに進める委任を受けた。今回の実装者は確定した四ファイルの移動だけを行い、新IR・要件・テストを作らない。独立検査と統合は呼び出し側が行う。
  - why: 所属変更を入力・失敗・安全性・認証・ファイル操作の意味の変更と混ぜず、実装と独立レビューの責任を分ける。
  - decided_by: user

## Delegated

- D1 既存clientと相互参照するknown_hosts・known_hosts_io・preferredを一緒に移す。clientの既存公開型とmethodは製品の同じ公開パスから再公開し、他三モジュールは新ownerのcrate内だけに保つ。既存の実装・テストをコードベース探索の段階で採用し、新型やtransportラッパーを作らない。
  - why: 閉じた既存依存をそのまま再利用でき、ホストキー・ホーム展開・認証とファイル操作の挙動を変更する必要がない。
  - decided_by: caller（A1の設計委任に基づく確定設計）
- D2 SshClient::exec_strictはpub(crate)と本文を維持し、新SSH rootの公開関数は同methodにawaitで委譲するだけとする。製品はこの関数をpub(crate)で再公開し、runtimeの三つの呼び出しだけを同じ入力・エラー処理のまま変える。利用者探索で外の参照がない旧privateモジュール宣言と製品configのprivate helper別名は削除し、未使用抑制やdead wrapperを作らない。
  - why: 既存のstrict実行を新たな公開inherent APIにせず、必要な製品内アクセスだけを維持する。呼び出し側の意味を変えない最小の接続方法である。
  - decided_by: caller（境界と三つの利用者を確定した設計）
- D3 test-utilsは製品からSSHの同名featureへ転送し、既存cfgと二つのrequired-features宣言を変えない。private configモジュールはsettingsの五型とconfigのexpand_tilde・DEFAULT_MAX_DIR_ENTRIESをcrate内で再公開し、core別名にerrorを追加する。依存は製品の既存指定を採用し、既存tracingのtarget指定で四モジュールの計30サイトの旧targetを保つ。
  - why: 同じ型・helper・feature条件とログフィルターを標準のRust再公開・Cargo feature/path接続で維持できる。ファイル名・行番号・module_pathは配置変更に伴い変わり得るがtarget変更として扱わない。
  - decided_by: caller（A1の具体化と再利用判断）
- D4 比較元を91f80c9f027770b4d6f474f294b07d7c0831af49に固定し、旧両記録を除いて現在の比較を実装者と独立担当が記録する。移す52テストだけのowner/binaryとssh接頭辞を対応付け、他3057件をそのまま保つ。正確なtarget追加だけを除いて本文とmodeを比較し、三runtime呼び出しと必要な整形だけを逆変換する。既存検査・API/feature確認と配布前後比較を行い、必須の変異検査は設けない。
  - why: 前回の移動を今回の根拠とせず、公開範囲・テストの所在・意味の維持を限定した比較で確かめる。未実行の動的ログや環境を検証済みと扱わない。
  - decided_by: caller（PROJECT.mdの照合とA1に従う）
- D5 Agent・engineなど次の移動は今回行わず、設定crate・settings/core・既存SSHパース/サポート・他の利用側とテスト・IR・スクリプト・フックとCIは変更しない。
  - why: 四ファイルの依存を移す範囲を超える責務や安全性の判断を混ぜない。
  - decided_by: caller（閉じた実装範囲）
