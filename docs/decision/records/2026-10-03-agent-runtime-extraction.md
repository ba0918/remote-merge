# Agent実行処理と転送の内部crateへの抽出

## Context

通信型とframingは既にprotocolに所属し、SSH実装も内部crateへ移行している。
今回はAgentの実行処理と転送の七ファイルを既存157テストと共に移す。
機械的移動として扱い、正式な計画や新しいIRは作らない。

Position: 利用者の自律的な移行の委任を受け、呼び出し側が以下の設計と閉じた範囲を確定した。実装者の照合後、独立した担当がレビューと記録を行う。

## Agreements

- A1 振る舞い・公開API・依存版とfeature・テスト・製品版0.2.13と通信版v4を維持する移行の設計、継続的レビューとローカル統合・後片付けは確認なしに進める委任を受けた。今回の実装者は確定したAgent実行処理だけを移し、新IR・要件・テストを作らない。独立検査と統合は呼び出し側が行う。
  - why: 所属変更を入力・エラー・metadata・書き込み・symlink走査・transport終了順序の意味の変更と混ぜず、実装と独立レビューの責任を分ける。
  - decided_by: user

## Delegated

- D1 remote-merge-agentを内部版0.1.0・edition 2021・publish=falseの通常member/default-memberとし、client・server・dispatch・file_io・tree_scan・ssh_transport・testsをnested agentのまま移す。private run_agent_loopとテストの参照を保ち、coreのfilter/treeをprivate root別名で再利用する。テストの既存parser参照にはtest cfgだけでSSH別名とdev依存を使う。
  - why: 既存の閉じたclusterと157テストをコードベース探索の段階で採用し、Rust再公開・Cargo path依存で型と内部参照を維持できる。新しい抽象化・helper公開・test-utils featureは不要である。
  - decided_by: caller（A1の設計委任に基づく確定設計と再利用判断）
- D2 新ownerのprotocol/framingは下位protocolの同じ項目を再公開する。製品のprotocol.rsとCLI_VERSION、deployとbuild.rsのTARGETは変更せず、内部版0.1.0をhandshakeに使う定数やversion macroを追加しない。Unix/nonUnixの既存production gateと元のtest gateをそのまま維持する。
  - why: 通信版と製品版の正本を変えず、配置処理やbuild時metadataの責務をこの移動へ混ぜない。元のclient単体テストのUnix参照も新しい環境対応として書き換えない。
  - decided_by: caller（段階的移行の境界）
- D3 32イベントとserverのDEBUG enabled guardの計33箇所にremote_merge::agent::<元module>の旧targetを明記する。正確なtarget追加だけを除いて全本文とGit modeを比較し、テスト本文・コメント・cfgとprivate可視性を保つ。移す157件だけのpackage/lib binary ownerを対応付け、nested agent接頭辞と他2952件の所在・名前・無視状態を一切変えない。
  - why: イベントだけでなくguardのフィルターも維持し、公開範囲の拡大と検査漏れを防ぐ。ファイル名・行番号・module_pathは配置変更に伴い変わり得るがtarget変更として扱わない。
  - decided_by: caller（A1の具体化と検証方法）
- D4 比較元を4542517039f404edb4bc14d555389e7f08cb1daaに固定し、旧両記録を除いて現在の比較を実装者と独立担当が記録する。既存の順序付き検査・本文とAPI比較・配布前後比較を行い、必須の変異検査は設けない。
  - why: 前回の移動を今回の根拠とせず、元のprivate roundtripとprocess/channel境界の検査を維持する。未取得の動的ログや未実行の環境を検証済みと扱わない。
  - decided_by: caller（PROJECT.mdの照合とA1に従う）
- D5 次のdeployやengineなどの抽出は今回行わず、製品main/runtime・SSH/config/settings/core・protocol/framing・他のテストとIR・README・scripts/hooks/CIは変更しない。
  - why: 実行処理の七ファイルを移す範囲を超える責務や安全性の判断を混ぜない。
  - decided_by: caller（閉じた実装範囲）
