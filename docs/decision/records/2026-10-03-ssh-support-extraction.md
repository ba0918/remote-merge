# SSHのヒント・ホストキー確認・パスフレーズ取得の所属変更

## Context

SSHのパースとバッチ処理は既存の内部crateに所属している。
次はhint・host_key_verifier・passphrase_providerと既存29単体テストを同じcrateへ移す。
内容を維持する小さい抽出であり、正式な実装計画や新しいIRは作らない。

Position: 利用者が限定した抽出を採用した。実装者の照合後、独立した担当がレビューと記録を行う。

## Agreements

- A1 三つのモジュールと既存29単体テストを移し、既存remote_merge::ssh公開パスを同じtrait・型の再公開で保つ。ホストキーポリシーとCLI/TUIの判定、プロンプト・エラー文言、パスフレーズのゼロ化・空値・環境変数名と優先度・リトライ上限3、ヒント文字列を変えない。
  - why: サポート処理の所属変更を接続や認証の振る舞いの変更から切り離す。
  - decided_by: user (took the recommendation)
- A2 新しいcrate・IR・要件・テストは作らず、既存内部版0.1.0・publish=false、製品版0.2.13・通信版v4を保つ。既存の依存版・featureと他のソースは変更しない。
  - why: 機械的な移動を製品仕様や依存更新、検査方針の変更と混ぜない。
  - decided_by: user
- A3 ホストキー確認の三つのinfo/warnはremote_merge::ssh::host_key_verifier、端末エラーのdebugはremote_merge::ssh::passphrase_providerの既存targetを明記して保つ。メッセージ・レベル・フィールドは変えない。
  - why: 定義元crateが変わっても既存のログフィルターを維持する。ファイル名・行番号・module_pathは配置変更に伴い変わり得るが、targetの変更として扱わない。
  - decided_by: user

## Delegated

- D1 既存の三つのモジュールをコードベース探索の段階で採用する。既存settings型はcrate内のconfig別名で再利用し、Rustのpub use・Cargoのpath依存・既存tracingのtarget指定を使う。rpassword・zeroize・serial_testは製品の既存指定をコピーし、新しいロジックやラッパーを作らない。
  - why: 型・trait・入力処理を複製せず、五つの既存config参照も書き換えずに閉じた移動を実現できる。
  - decided_by: caller（A1〜A3の具体化と再利用判断）
- D2 移した29件だけのpackage・library target・test binaryとssh接頭辞を新crateルートへ対応付ける。他の3080件は所在・名前・無視状態を一切変えず比較し、以前移した51件もそのまま保つ。四箇所の正確なtarget追加だけを除いて全本文とGit modeを比較する。
  - why: 総3109件だけでは検査漏れを判断できず、承認した所在変更とログtarget保持の差分を限定して確かめる必要がある。元の単一行マクロはtargetを行内に追加して再整形を避ける。
  - decided_by: caller（A1・A3の検証方法とローカルformat確認に基づく実装詳細）
- D3 ブランチ全体の比較元を67f7d5cd16e5b204bf8f4be044ba1bbdb85014e4に固定し、旧両変更記録を除いて今回の比較を実装者と独立したレビュー担当がそれぞれ記録する。既存検査・本文比較・安全なローカルログ確認と配布の前後比較を行い、必須の変異検査は設けない。端末エラーのログを確認するためだけの入力や失敗の捏造は行わない。
  - why: 前回の抽出記録を今回の根拠とせず、挙動を維持する移動を既存の検査と限定した比較で確かめる。実行していない端末エラー経路を動的検証済みと扱わない。
  - decided_by: caller（PROJECT.mdの変更照合とA2に従う）
- D4 client・known_hosts・preferred、残る認証helper・制限・exec_strictとtest-utilsは今回移動も変更も行わない。settings・coreと既存パース二モジュールの内容も維持する。
  - why: 三つの閉じたサポートモジュールの移動を超える境界や安全性の判断を混ぜない。
  - decided_by: caller（A1の閉じた範囲の具体化）
