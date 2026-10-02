# 接続設定型の内部settings crateへの抽出

## Context

SSHの設定型は製品のconfigモジュールで定義され、複数の利用側が同じ公開パスから使っている。
読み込みや認証の解決はそのままに、標準ライブラリだけを使う接続設定型の所属を分ける。
今回の作業は既存内容を維持する小さい抽出であり、正式な実装計画や新しいIRは作らない。

Position: 利用者が限定した抽出を採用した。実装者の照合後、独立した担当がレビューと記録を行う。

## Agreements

- A1 ServerConfigとそのDebug実装、AuthMethod、SshOptions、SshConfigとそのDefault実装、StrictHostKeyCheckingだけを内容・trait・公開範囲を変えずに移す。既存remote_merge::config公開パスを保ち、設定の読み込み・認証の解決・パースと既存テストは製品に残す。
  - why: 接続設定型の所属変更を、設定の意味や認証動作の変更から切り離す。
  - decided_by: user (took the recommendation)
- A2 専用crate名をremote-merge-settingsとし、内部版0.1.0・edition 2021・publish=falseで作る。通常のworkspace検査に参加するmembersとdefault-membersへ追加する。
  - why: 設定型の独立した所属を明示し、依存としてのビルドだけでなく通常検査の対象にもする。
  - decided_by: user
- A3 新しいIR・要件・テスト・依存ライブラリは追加せず、既存の依存版とfeature、製品・core・protocolの版、通信版を変えない。
  - why: 機械的な抽出を製品仕様や検査方針、依存更新と混ぜない。
  - decided_by: user

## Delegated

- D1 新crateはstdのPathBufと既存の5型・実装だけで構成し、通常依存・dev依存・featureを設けない。製品は既存Cargoのpath依存とRustのpub useで同じ型を再公開する。
  - why: 型の複製やラッパー、feature転送を作らず、既存利用側のimportを維持できる。
  - decided_by: caller（A1〜A3の具体化）
- D2 全3109テストの名前・package・library target・test binary・無視状態を一切対応変更せず比較する。89件のconfigテストは製品側に残し、新settingsライブラリのテスト数は0件とする。抽出ブロックの全内容とmode、残るconfig本文と他のsource/testsも比較する。
  - why: テストを移さないため所在変更の正規化は不要であり、既存のDebug・Default・設定と認証の検査をそのまま根拠にする。
  - decided_by: caller（A1・A3の検証方法）
- D3 ブランチ全体の比較元を0d683e86e13c3f7dec9a458a97ff7181e0691f37に固定し、旧両変更記録を除いて今回の比較を実装者と独立したレビュー担当がそれぞれ記録する。必須の変異検査は設けず、既存テストと内容・依存・配布の前後比較で検証する。
  - why: 前回の抽出記録を今回の根拠として扱わず、挙動を変えない型の所属変更に新しい変異やテストを要求しない。
  - decided_by: caller（PROJECT.mdの変更照合とA3に従う）
- D4 expand_tilde、DEFAULT_MAX_DIR_ENTRIES、その他の設定型・privateなraw型・読み込み処理、およびSSHのprivateなtree_parser・exec_strict境界は移さない。ServerConfigのパスワード伏字、None表示、他のDebug項目、SshConfigの300秒・Ask・false・falseのDefault値を保持する。
  - why: 将来の境界判断や安全性の変更を今回の機械的な5型の抽出へ混ぜず、既存の可観測動作を維持する。
  - decided_by: caller（A1の閉じた範囲の具体化）
