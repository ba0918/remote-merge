# 最初のcore crate抽出

## Context

Cargoワークスペースへの配置変更がmainに統合された。
次はtree・diff・error・filterをremote-merge-coreへ抽出する。
製品の挙動と既存の公開パスを保ち、残りの責務分割は別の作業として扱う。
依存とテストの配置を調べ、抽出に必要な具体的な判断を確認する。

Position: 依存調査とQ1〜Q3の回答が完了した。未決事項はない。決定記録の承認・コミット後に実装計画を作成する。再公開による互換性と配布経路は実装時のビルド・既存テストで確認する。

## Agreements

- A1 最初の細分化はtree・diff・error・filterをremote-merge-coreへ抽出する作業とする。既存の振る舞いとremote_merge経由の公開パスを維持する。
  - why: 表示や接続の責務と共有処理を段階的に分け、利用側を一度に書き換えずに進める。
  - decided_by: user (took the recommendation)
- A2 開発方針のIRは新設せず、方針を固定するためのテストは追加しない。
  - why: crate分割は開発上の配置と責務の判断であり、サービスの仕様を追加する作業ではない。前回の移行で利用者が定めた境界を引き継ぐ。
  - decided_by: user
- A3 抽出対象の型・振る舞い・既存の外部依存の指定を維持する。TOMLを含むerrorの依存もcoreへ持ち込み、エラー型の分離は今回行わない。既存の170件の単体テストは対象モジュールと共に移し、製品の統合・契約テストは現在の製品パッケージに残す。
  - why: 型の分割やテストの公開範囲変更を混ぜず、抽出前後を同じ既存テストで比較する。
  - decided_by: user (took the recommendation)
- A4 remote-merge-coreは版0.1.0・publish=falseの内部crateとする。製品は版0.2.13を維持し、checkoutからのソースインストールと既存のbinary配布を保つ。crates.io公開への対応は別作業とする。
  - why: 現在の配布経路を保ち、今回の責務分割にcrate公開の手続きを混ぜない。
  - decided_by: user (took the recommendation)
- A5 coreと製品の両方をワークスペースのmembersとdefault-membersに含める。ルートの通常検査は両crateを対象にし、移した単体テストも検査する。
  - why: 依存crateの単体テストは依存のビルドだけでは実行されないため、抽出による検査漏れを防ぐ。
  - decided_by: user (took the recommendation)

## Prohibitions

- P1 今回は新しい製品機能・バグ修正・依存のバージョン更新・残りのcrate抽出・テストの期待値や印の変更を行わない。
  - why: 振る舞いを維持する最初の4モジュールの抽出と、後続の細分化を切り離す。
  - decided_by: user（A1・A3の範囲）

## Delegated

- D1 再利用する層は、既存4モジュールの実装、Rustのpub useによる互換性、Cargoのworkspaceとpath依存、既存の単体・契約・統合テスト、既存のエラー型とアダプタ変換とする。新しい実行時ライブラリや互換ラッパーは導入しない。
  - why: 調査範囲では4モジュール間以外の製品モジュール依存がなく、既存の仕組みで抽出範囲と公開パスの維持を実現できると推定する。
  - decided_by: planner（A1・A3の具体化）
- D2 coreの通常依存はchrono・glob-match・tracing・thiserror・toml・anyhow・similar・serde・sha2とし、現在の指定を引き継ぐ。既存のdiff単体テストが使用するserde_jsonはcoreのdev-dependencyに置く。製品側で引き続き必要な依存は残し、依存の共通化やfeature縮小は行わない。
  - why: 対象のソースと既存テストの参照を調査した結果から、内容を変えずに実行できる依存範囲を具体化する。errorにはrussh依存がなく、coreにSSHや表示ライブラリを加える必要はない。
  - decided_by: planner（A3の具体化）
- D3 再公開は既存4モジュールと同じ名前で行い、互換ラッパーや型の複製を作らない。型の定義元crateが変わるため、Rustのtype_nameやコンパイラ診断に出る定義元の文字列まで固定するものではない。
  - why: 既存の利用側のimportと型の同一性を保つにはpub useで同じ型を公開する必要がある。調査範囲に定義元文字列への依存は見つからなかった。
  - decided_by: planner（A1の公開パス維持の具体化）
