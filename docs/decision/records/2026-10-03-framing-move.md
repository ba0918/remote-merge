# フレーム処理の既存protocol crateへの移動

## Context

通信プロトコルは既存の内部crateに所属している。
フレーム処理も同じcrateへ移し、Agentの利用側は既存公開パスのまま使う。
今回の作業は内容と振る舞いを保つ配置変更であり、正式な実装計画や新しいIRは作らない。

Position: 利用者が限定した移動を採用した。実装者の照合後、独立した担当がレビューと記録を行う。

## Agreements

- A1 framingと既存7単体テストを内容・Git modeを変えずにremote-merge-protocolへ移し、既存のremote_merge::agent::framing公開パスを再公開で保つ。16 MBの制限、通信形式、エラーの種類と伝播を維持する。clientとserverの既存importは変更しない。
  - why: 既存の閉じたフレーム処理を通信プロトコルと同じ所属へ移し、利用側や通信動作の変更を混ぜない。
  - decided_by: user (took the recommendation)
- A2 新しいcrate・依存・IR・要件・テストは追加せず、既存テストの本文・期待値・印・無視状態、依存・版・featureは変えない。
  - why: 機械的な配置変更を、製品仕様や検査方針の変更から切り離す。
  - decided_by: user

## Delegated

- D1 既存framingモジュール、既存Cargo依存とRustのpub useを再利用する。protocol crateに公開モジュール宣言を追加し、製品のagentモジュール宣言だけを同名再公開に置き換える。
  - why: 新しい依存や型ラッパーを作らず、利用側の既存公開パスを維持できる。
  - decided_by: caller（A1・A2の具体化）
- D2 テスト比較では移した7件だけのpackage・library target・test binaryとagent::framingからframingへの接頭辞変更を対応付ける。他の3102件の所在・名前・無視状態はそのまま比較し、移動ファイル全体のblobとGit modeの一致も確認する。
  - why: 総件数だけでは検査漏れを判定できず、内容と所在を分けて比較する必要がある。
  - decided_by: caller（A1・A2の検証方法）
- D3 ブランチ全体の比較元をcf24f532c627015d53da5664f881859fa655368aに固定し、旧両変更記録を除いて今回の比較を実装者と独立したレビュー担当がそれぞれ記録する。
  - why: 前回の抽出記録を今回の照合として扱わず、全変更の根拠と担当の独立性を確かめる。
  - decided_by: caller（PROJECT.mdの変更照合に従う）
- D4 write_frameの本体をOk(())へ置き換える任意の変異は抽出前にタイムアウトし、検知成功とは数えない。呼出元が追加したcaught必須条件を除き、再実行や別変異・テスト・実行方針の変更は行わない。内容とmodeの完全一致、7件の所在対応、既存全テストと通常検査を配置変更の根拠とする。
  - why: 変異時はフレームが送られず、既存client/serverの読み取り待ちで三つのテスト枠が埋まり、フレームのassertionへ到達しなかった。この追加条件は製品要件ではなく呼出元の検証方針であり、内容を変えない移動の検証に不要と判断した。観測したタイムアウトを成功へ読み替えない。
  - decided_by: caller（実装者の観測結果を受けた検証方針の変更。利用者の製品合意ではない）
