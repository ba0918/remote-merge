# core抽出後の細分化

## Context

coreへの4モジュールの抽出がmainへ統合された。
利用者は続く細分化を希望しているが、次に抽出する範囲は未決定である。
AgentとSSHは表示処理に依存しない一方、SSHが製品の設定型を使用するため、一括抽出には追加の境界判断が必要である。
先に通信プロトコルだけを抽出する案と、より広い抽出を比較する。

Position: Q1・Q2の推奨を利用者が採用した。未決事項はない。決定記録の確認・承認とコミット後に実装計画を作成する。ソース変更は未着手。

## Agreements

- A1 core抽出をmainへ統合し、そこを基点として続くcrate細分化を検討する。
  - why: 検証済みの段階を統合してから、次の変更範囲を切り分ける。
  - decided_by: user
- A2 次は通信プロトコルをremote-merge-protocolへ抽出し、既存14単体テストも対象コードと共に移す。framing・deployment・SSH・設定・binaryは今回移さず、製品の既存公開パスと通信動作を維持する。
  - why: より広いAgent・SSH抽出に必要な設定型の所有先や内部APIの公開範囲の判断を切り離し、閉じた小さな依存境界から進める。
  - decided_by: user (took the recommendation)
- A3 新しいcrateは版0.1.0・publish=falseの内部crateとし、ワークスペースのmembersとdefault-membersへ追加する。製品の版0.2.13・protocol v4・既存の版表示とAgent互換性判定を維持する。
  - why: 移した単体テストを通常検査から落とさず、内部crateの版を製品版として表示する誤りを防ぐ。
  - decided_by: user (took the recommendation)
- A4 今回も開発方針のIR・新しい要件・新しいテストは追加しない。既存テストの期待値と印、プロトコルの通信形式、外部依存の版とfeatureは維持する。
  - why: 段階的なcrate抽出と製品仕様・検査方針の変更を混ぜず、既存の検査で挙動の維持を確認する。
  - decided_by: user (took the recommendation)

## Delegated

- D1 新crateのdefine_protocol_versionマクロの単一の版リテラルからPROTOCOL_VERSIONと公開マクロproduct_cli_versionを生成する。product_cli_versionは引数なしでconcat!・env!("CARGO_PKG_VERSION")・stringify!を展開し、呼出元パッケージの版とprotocol版から従来と同じ文字列を作る。製品側のagent::protocolは新crateの型・定数・関数を再公開し、pub const CLI_VERSION: &str = remote_merge_protocol::product_cli_version!();を定義する。新crateには製品版と紛らわしいCLI_VERSION定数を置かない。
  - why: 現在のCLI_VERSIONはCARGO_PKG_VERSIONから作られるため、そのまま新crateへ移すと製品0.2.13ではなく内部版0.1.0を表示し、Agentの互換性判定も変わる。
  - decided_by: planner（A2・A3の具体化）
- D2 実装は既存のプロトコル型・シリアライズ・ハンドシェイク処理と既存テストを再利用し、Cargoのpath依存とRustの再公開を採用する。新しい実行時ライブラリ、型ラッパー、プロトコル版の複製を導入しない。
  - why: 対象は既存のanyhow・serde・rmp-serdeだけに依存し、serde_jsonは既存テスト用であり、別の処理や依存を追加せず抽出できると推定する。コンパイルと版表示は実装時に確認する。
  - decided_by: planner（A2・A4の具体化）
