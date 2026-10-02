# Cargo ワークスペースへの移行

## Context

現在の remote-merge は、CLI・TUI・Agent・SSH を単一パッケージに持つ。
利用者は将来の core・agent・ssh・tui などへの分割を見据えた monorepo 化を希望している。
依存調査では共有処理と表示処理の循環があり、配置変更と責務分割を分けて判断する必要がある。
既存のテスト方針にも配置の指定があるため、設定の変更だけでなく仕様の改訂範囲も先に決める。

Position: ラウンド1・2の回答をA2〜A6に記録した。未決事項はない。決定記録の確認・承認後に配置変更の計画を作る。

## Agreements

- A1 monorepo 化に向けて依存関係と配置変更の影響を調査する。
  - why: 既存機能の動作を保ったまま進めるため、最初の変更範囲と検査への影響を明らかにする。
  - decided_by: user
- A2 最初の成果物は、既存パッケージを crates/remote-merge へ移す Cargo ワークスペース化に限定する。パッケージ名・バージョン・振る舞いを維持し、crate の抽出と依存整理は後続の別作業にする。
  - why: 配置変更と責務分割を分け、問題が出たときの原因を切り分ける。
  - decided_by: user (took the recommendation)
- A3 ワークスペース化の開発方針は新しいIRにせず、PROJECT.mdとこの決定記録に置く。方針を固定するためのテストは追加しない。
  - why: 開発方針はサービス自体の仕様ではなく、IR化によってテスト化する必要のない概念にまでテストを要求することを避ける。
  - decided_by: user
- A4 独立したDocker E2Eのパッケージはtests/container-e2eに残し、通常テストだけをcrates/remote-mergeへ移す。Docker E2Eの独立ワークスペースと専用の実行手順を維持する。
  - why: 通常テストとDocker専用テストを混ぜず、配置変更で専用コマンドの修正箇所を増やさない。
  - decided_by: user (took the recommendation)
- A5 proptestが見つけた失敗入力の保存先は、リポジトリ直下のproptest-regressions/を維持する。既存テストの保存先指定だけを新しいパッケージ配置に合わせて補正する。
  - why: パッケージ移動によって失敗入力の保存先を変えず、再現のための入力を従来どおりGitで管理できるようにする。
  - decided_by: user (took the recommendation)
- A6 既存のテストIRの具体的な配置指定だけを新配置へ追従させる。検証方針・要件IDは変えず、新しい要件・テストは追加しない。開発方針を既存IRから外す整理は今回の移動とは分ける。
  - why: 配置変更後も検査範囲の記述を実態と一致させ、方針の再整理やテスト追加を同時に行わない。
  - decided_by: user (took the recommendation)

## Rejected

- R1 ワークスペースの配置・検証・既存コマンドの維持について、docs/ir/development/workspace.mdを新設する案は採用しない。
  - why: 開発方針はサービスの仕様と分けて扱う。配置変更の成立はCargoと既存テストで検証し、方針そのものを固定するテストを増やさない。
  - decided_by: user
