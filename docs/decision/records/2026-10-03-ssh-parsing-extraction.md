# SSHのパースとバッチ読み込みの内部crateへの抽出

## Context

接続設定型の所属は既に分離されている。
次は既存のtree_parserとbatch_readだけを独立させ、接続や認証など残るSSH処理は製品に残す。
内容を保つ小さい抽出として扱い、正式な実装計画や新しいIRは作らない。

Position: 利用者が限定した抽出を採用した。実装者の照合後、独立した担当がレビューと記録を行う。

## Agreements

- A1 tree_parser.rsとbatch_read.rs、および内包する既存単体テストを移す。製品のssh::batch_readは公開のまま再公開し、ssh::tree_parserはpub(crate)の再公開を保つ。利用側のimport・型・処理・テストは変更しない。
  - why: 閉じたパースとコマンド生成・結果処理の所属変更を、接続や認証の責務の変更から切り離す。
  - decided_by: user (took the recommendation)
- A2 remote-merge-sshは内部版0.1.0・edition 2021・publish=falseとし、membersとdefault-membersへ追加する。製品版0.2.13・通信版v4と既存内部crateの版は変えない。
  - why: 既存の配布と通常検査を維持し、移した単体テストを依存のビルドだけで済ませない。
  - decided_by: user
- A3 移動内容の変更は三つのwarnに既存のtracing targetを明記する箇所だけとする。tree_parserはremote_merge::ssh::tree_parser、batch_readの二箇所はremote_merge::ssh::batch_readを保つ。メッセージ・レベル・フィールドは変えず、新IR・要件・テスト・認証処理やhelper・外部依存更新を追加しない。
  - why: 定義元crateが変わっても既存EnvFilterの対象を変えない。sourceのファイル名・行番号・module_pathは配置変更に伴い変わり得るが、targetの変更として扱わない。
  - decided_by: user

## Delegated

- D1 既存parserとbatch処理はコードベース探索の段階で採用し、既存coreのfilter・tree型も同じ段階で再利用する。Rustの再公開、Cargoのpath依存、既存tracingのtarget指定を採用し、新しいロジック・型ラッパー・ライブラリは作らない。新crateはcore・既存指定のchronoとserde feature・tracing、テスト用tempfileだけを使う。
  - why: 既存の層と標準の接続方法で閉じた抽出を実現でき、依存共通化やsettings依存・feature転送は不要である。
  - decided_by: caller（A1〜A3の具体化と再利用判断）
- D2 実測した移動テストだけのpackage・library target・test binaryとssh::tree_parser/batch_readから新crateルートへの接頭辞変更を対応付ける。他のテスト名・所在・無視状態は変えない。三つのtarget行だけを除いて移動元と移動先の全bytesとGit modeを比較する。
  - why: 総3109件だけでは検査漏れを判断できず、承認した所在変更とtarget保持の差分を限定して確認する必要がある。
  - decided_by: caller（A1・A3の検証方法）
- D3 ブランチ全体の比較元をbfba1f44890584be909b6cb570eb3128423571e9に固定し、旧両変更記録を除いて今回の比較を実装者と独立したレビュー担当がそれぞれ記録する。既存チェック・テスト・ログtargetのローカル比較・配布の前後比較を行い、必須の変異検査は設けない。
  - why: 前回の抽出記録を今回の根拠として扱わず、挙動を維持する配置変更を既存の検査と限定した比較で確かめる。
  - decided_by: caller（PROJECT.mdの変更照合とA3に従う）
- D4 残るclient・known_hosts・preferredなどのSSHファイル、設定型、認証helper、exec_strictや制限・private APIの将来の境界は今回移動も公開範囲の変更も行わない。
  - why: 二つの閉じたモジュールの抽出を超える意味や責務の判断を混ぜない。
  - decided_by: caller（A1の閉じた範囲の具体化）
