# ワークスペース移行の実装・検証手順

## Context

[配置変更の決定](./2026-10-02-cargo-workspace.md#Agreements)が承認され、既存パッケージだけを移す範囲が確定した。
既存コマンドの対象、テスト収集、保存先、変更照合を保つため、配置と検証の具体的な手順を計画にする。
ここでは開発方針のIRや構造を固定するテストを増やさず、Cargoと既存の検証手段を使う。

Position: 計画の具体化に伴う判断を記録した。配置変更の決定とあわせて計画をレビューし、利用者の承認を受ける。

## Delegated

- D1 ルートは仮想ワークスペースとし、crates/remote-mergeを唯一のmemberとdefault-memberにする。resolverはedition 2021に対応する2を明示し、release profile・Cargo.lock・.cargoはルートに残す。workspace.dependenciesへの共通化はしない。
  - why: 既存のルートCargoコマンドの対象とtarget配置を維持するため。パッケージの移動だけで新しい共通化を加える必要はない。
  - decided_by: planner（配置変更のA2・独立E2E維持のA4の具体化）
- D2 移動前後のmetadata・テスト名と無視状態の集合・インストールしたbinaryの表示を比較し、既存コードとテストは許可した保存先指定以外のblobとモードの一致で確認する。
  - why: 件数やテストの成功だけでは、ターゲットの収集漏れや製品コードの変更を見逃す。新しい構造固定テストは不要である。
  - decided_by: planner（振る舞い維持のA2・新テストを増やさないA3・保存先維持のA5の具体化）
- D3 変異検査はbackup_timestampの既存の同等変異に絞り、一覧のパスと既存のscripts/mutants.shによる実行・kotowariの照合を移動前後で確認する。全変異や既存FLAGの再判定は行わない。
  - why: 確かめる対象はワークスペース化で変わるツールのパスとテスト選択であり、変更していない製品ロジック全体の再審査ではない。
  - decided_by: planner（配置変更に限定するA2の具体化）
- D4 変更照合の比較元は0700f9f64e35de5b91a09beda48c7b85a37f289bとし、実装者と独立レビュー担当がそれぞれ記録を作る。候補headは記録のコミット後に確定し、checkとreview-phaseのchangesを実行する。
  - why: 承認済みの移行方針以降の変更をブランチ全体で照合し、実装者の自己申告を独立レビューの代わりにしない。
  - decided_by: planner（既存のPROJECT.mdのChange conformanceに従う）
- D5 scripts/mutants.shの内側のsystemdサービスにも隔離先のHOME・XDG_CONFIG_HOME・XDG_DATA_HOME・TMPDIRを明示的に引き継ぐよう、移動前の比較採取に先立って補正する。ユーザーマネージャの共有環境は変更しない。
  - why: 現在のスクリプトはこれらの変数を引き継がず、外側だけで環境を隔離しても内側のサービスへ届かない。移動前後を同じ隔離条件で検証するために必要である。
  - decided_by: planner（検証のための限定した追加変更として利用者の承認対象に含める）
